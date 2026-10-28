use crate::{Hub, HubError, Result};
use serde::Serialize;
use ts_rs::TS;

/// Live protection derived from durable owner state, without a second lock lifecycle.
/// Directory claims describe known paths, not a complete or verified write boundary.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct ResourceClaim {
    pub owner_kind: String,
    pub owner_id: String,
    pub operation: String,
    pub state: String,
    pub scope: String,
    pub access: String,
    pub path: Option<String>,
    pub reason: String,
}

pub(crate) async fn read_claims(
    connection: &mut sqlx::SqliteConnection,
) -> Result<Vec<ResourceClaim>> {
    sqlx::query_as(include_str!("resource_claims.sql"))
        .fetch_all(connection)
        .await
        .map_err(HubError::db)
}

impl Hub {
    pub(crate) async fn overlay_claims(&self, claims: &mut Vec<ResourceClaim>) {
        let recovery = self.runtime_recovery.lock().await;
        // A recovery overlay is only installed after the worker has ended. Sessions
        // never use that overlay: an uncertain child must continue to protect data.
        claims.retain(|claim| {
            !recovery.get(&claim.owner_id).is_some_and(|failure| {
                claim.owner_kind
                    == match failure.kind {
                        crate::task_recovery::TaskKind::Scan => "scan",
                        crate::task_recovery::TaskKind::Backup => "backup",
                        crate::task_recovery::TaskKind::Restore => "restore",
                        crate::task_recovery::TaskKind::Inspection => "inspection",
                        crate::task_recovery::TaskKind::InstallationCopy => "installation_copy",
                    }
            })
        });
    }

    pub(crate) async fn check_resource_claims(&self) -> Result<()> {
        let mut connection = self.pool.acquire().await.map_err(HubError::db)?;
        let mut claims = read_claims(&mut connection).await?;
        drop(connection);
        self.overlay_claims(&mut claims).await;
        if claims.iter().any(|claim| claim.owner_kind == "session") {
            return Err(HubError::new(
                "GAME_RUNNING_OR_UNKNOWN",
                "游戏会话仍占用资源或写入范围未知，请先检查游戏及其子进程已关闭。",
            ));
        }
        if !claims.is_empty() {
            return Err(HubError::new(
                "TASK_CONFLICT",
                "活动任务仍占用资源，请等待完成或取消后再操作。",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_recovery::{RuntimeFailure, TaskKind};

    async fn fixture() -> (tempfile::TempDir, std::sync::Arc<Hub>) {
        let temp = tempfile::tempdir().unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        sqlx::query("INSERT INTO workspace VALUES ('w','/saves','/backups','/content','now')")
            .execute(&hub.pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO installation VALUES ('i','game','/game/exe','/game','hash',NULL,NULL,'test','test','now')")
            .execute(&hub.pool).await.unwrap();
        (temp, hub)
    }

    async fn owners(hub: &Hub) {
        sqlx::raw_sql("INSERT INTO session (id,installation_id,state,started_at,instance_data_root) VALUES ('s','i','running','now','/instance');
            INSERT INTO task (id,request_key,workspace_id,operation,state,phase,created_at) VALUES ('t','t','w','world.scan','cancelling','scan','now');
            INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES ('b','b','hash','w','a','backup.create','running','copy','now');
            INSERT INTO backup_archive (id,task_id,workspace_id,relative_dir,created_at,entry_count,file_count,total_bytes,consistency,verification,verified_at,archive_sha256,manifest_sha256) VALUES ('a','b','w','a','now',0,0,'0','best_effort','verified','now','hash','hash');
            INSERT INTO restore_task (id,request_key,request_digest,workspace_id,backup_id,destination,state,phase,created_at) VALUES ('r','r','hash','w','a','/restored','running','copy','now');
            INSERT INTO inspection_task (id,request_key,request_digest,restore_task_id,state,phase,created_at) VALUES ('c','c','hash','r','running','read','now');")
            .execute(&hub.pool).await.unwrap();
    }

    #[tokio::test]
    async fn all_owners_expose_global_and_directory_protection() {
        let (_temp, hub) = fixture().await;
        assert!(hub.dashboard().await.unwrap().resource_claims.is_empty());
        owners(&hub).await;
        let claims = hub.dashboard().await.unwrap().resource_claims;
        assert_eq!(claims.len(), 14);
        assert_eq!(claims.iter().filter(|c| c.scope == "global").count(), 5);
        for (kind, path, access) in [
            ("session", "/game", "write"),
            ("session", "/instance", "write"),
            ("scan", "/saves", "read"),
            ("backup", "/saves", "read"),
            ("backup", "/backups", "write"),
            ("restore", "/restored", "write"),
            ("restore", "/backups", "read"),
            ("inspection", "/restored", "read"),
            ("inspection", "/backups", "read"),
        ] {
            assert!(claims.iter().any(|c| c.owner_kind == kind
                && c.path.as_deref() == Some(path)
                && c.access == access));
        }
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "GAME_RUNNING_OR_UNKNOWN"
        );
        sqlx::query("UPDATE session SET state='exited_unverified'")
            .execute(&hub.pool)
            .await
            .unwrap();
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "TASK_CONFLICT"
        );
        // Cancelling is still active; only the worker's terminal transition releases it.
        for table in ["task", "backup_task", "restore_task", "inspection_task"] {
            sqlx::query(&format!("UPDATE {table} SET state='cancelled'"))
                .execute(&hub.pool)
                .await
                .unwrap();
        }
        hub.check_resource_claims().await.unwrap();
        assert!(hub.dashboard().await.unwrap().resource_claims.is_empty());
    }

    #[tokio::test]
    async fn restart_releases_ended_workers_but_retains_unknown_game_until_acknowledged() {
        let (temp, hub) = fixture().await;
        owners(&hub).await;
        drop(hub);
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let claims = hub.dashboard().await.unwrap().resource_claims;
        assert_eq!(claims.len(), 3);
        assert!(
            claims
                .iter()
                .all(|c| c.owner_kind == "session" && c.state == "interrupted_unknown")
        );
        hub.acknowledge_interrupted("s").await.unwrap();
        assert!(hub.dashboard().await.unwrap().resource_claims.is_empty());
    }

    #[tokio::test]
    async fn recovery_overlay_releases_only_the_ended_owner_kind() {
        let (_temp, hub) = fixture().await;
        owners(&hub).await;
        hub.runtime_recovery.lock().await.insert(
            "t".into(),
            RuntimeFailure {
                kind: TaskKind::Scan,
                finished_at: "now".into(),
            },
        );
        let claims = hub.dashboard().await.unwrap().resource_claims;
        assert!(!claims.iter().any(|c| c.owner_kind == "scan"));
        assert_eq!(claims.iter().filter(|c| c.scope == "global").count(), 4);
        // Even a colliding recovery identifier cannot erase an unknown game claim.
        hub.runtime_recovery.lock().await.insert(
            "s".into(),
            RuntimeFailure {
                kind: TaskKind::Scan,
                finished_at: "now".into(),
            },
        );
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "GAME_RUNNING_OR_UNKNOWN"
        );
    }

    #[tokio::test]
    async fn protection_is_not_limited_to_recent_session_history() {
        let (_temp, hub) = fixture().await;
        sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('old','i','interrupted_unknown','0000')")
            .execute(&hub.pool).await.unwrap();
        for index in 0..51 {
            sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES (?,'i','exited_unverified','9999')")
                .bind(format!("ended-{index}")).execute(&hub.pool).await.unwrap();
        }
        let dashboard = hub.dashboard().await.unwrap();
        assert!(dashboard.sessions.iter().all(|s| s.id != "old"));
        assert!(
            dashboard
                .resource_claims
                .iter()
                .any(|c| c.owner_id == "old")
        );
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "GAME_RUNNING_OR_UNKNOWN"
        );
        hub.acknowledge_interrupted("old").await.unwrap();
        hub.check_resource_claims().await.unwrap();
    }
    #[tokio::test]
    async fn failed_acknowledgement_audit_cannot_release_game_protection() {
        let (_temp, hub) = fixture().await;
        sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('old','i','interrupted_unknown','now')")
            .execute(&hub.pool).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_ack BEFORE INSERT ON operation_log WHEN NEW.action='process.user_acknowledged' BEGIN SELECT RAISE(ABORT,'injected'); END")
            .execute(&hub.pool).await.unwrap();
        assert!(hub.acknowledge_interrupted("old").await.is_err());
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "GAME_RUNNING_OR_UNKNOWN"
        );
        let dashboard = hub.dashboard().await.unwrap();
        assert!(
            dashboard
                .resource_claims
                .iter()
                .all(|c| c.state == "interrupted_unknown")
        );
        assert!(dashboard.operations.is_empty());
    }
}
