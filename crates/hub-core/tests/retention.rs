use hub_core::{Hub, RetentionPolicy, WorkspaceInput};
use std::path::PathBuf;
struct Fixture {
    temp: tempfile::TempDir,
    hub: std::sync::Arc<Hub>,
    db: sqlx::SqlitePool,
    workspace: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for name in ["saves", "backups", "content"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        std::fs::write(temp.path().join("saves/source"), b"source unchanged").unwrap();
        std::fs::write(
            temp.path().join("backups/unregistered"),
            b"unregistered unchanged",
        )
        .unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let workspace = hub
            .initialize(WorkspaceInput {
                save_dir: temp.path().join("saves").to_str().unwrap().into(),
                backup_dir: temp.path().join("backups").to_str().unwrap().into(),
                content_dir: temp.path().join("content").to_str().unwrap().into(),
            })
            .await
            .unwrap()
            .id;
        let db = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(temp.path().join("metadata/metadata.sqlite")),
        )
        .await
        .unwrap();
        Self {
            temp,
            hub,
            db,
            workspace,
        }
    }
    async fn seed(&self, count: usize) -> Vec<String> {
        let mut tx = self.db.begin().await.unwrap();
        let mut ids = vec![];
        for index in 0..count {
            let id = uuid::Uuid::new_v4().to_string();
            let date = format!("2025-01-01T00:{:02}:{:02}Z", index / 60 % 60, index % 60);
            sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES (?,?,?, ?,?,'backup.create','succeeded','finished',?)").bind(&id).bind(&id).bind("digest").bind(&self.workspace).bind(&id).bind(&date).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO backup_archive (id,task_id,workspace_id,relative_dir,created_at,entry_count,file_count,total_bytes,consistency,verification,verified_at,archive_sha256,manifest_sha256) VALUES (?,?,?,?,?,0,0,'10','best_effort','verified',?,'archive-hash','manifest-hash')").bind(&id).bind(&id).bind(&self.workspace).bind(&id).bind(&date).bind(&date).execute(&mut *tx).await.unwrap();
            ids.push(id);
        }
        tx.commit().await.unwrap();
        ids
    }
}
fn zero() -> RetentionPolicy {
    RetentionPolicy {
        keep_recent: 0,
        daily_days: 0,
        weekly_weeks: 0,
    }
}
#[tokio::test]
async fn preview_covers_more_than_dashboard_limit_and_never_writes_or_authorizes_deletion() {
    let f = Fixture::new().await;
    let ids = f.seed(111).await;
    sqlx::query("UPDATE backup_archive SET display_name='升级前' WHERE id=?")
        .bind(&ids[0])
        .execute(&f.db)
        .await
        .unwrap();
    sqlx::query("UPDATE backup_archive SET retention_locked=1 WHERE id=?")
        .bind(&ids[1])
        .execute(&f.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO restore_task (id,request_key,request_digest,workspace_id,backup_id,destination,state,phase,created_at) VALUES ('restore','restore','digest',?,?,'unused','succeeded','finished','2025-01-01T00:00:00Z')").bind(&f.workspace).bind(&ids[2]).execute(&f.db).await.unwrap();
    let before = std::fs::read(f.temp.path().join("metadata/metadata.sqlite")).unwrap();
    let audits = f.hub.dashboard().await.unwrap().operations.len();
    assert_eq!(f.hub.dashboard().await.unwrap().backups.len(), 100);
    let result = f.hub.preview_retention(zero()).await.unwrap();
    assert_eq!(result.items.len(), 111);
    assert_eq!(result.protected_count, 4);
    assert_eq!(result.candidate_count, 107);
    assert_eq!(result.candidate_logical_bytes, "1070");
    assert!(!result.deletion_available);
    assert_eq!(result.version_context, "unverified");
    assert_eq!(
        std::fs::read(f.temp.path().join("metadata/metadata.sqlite")).unwrap(),
        before
    );
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), audits);
    assert_eq!(
        std::fs::read(f.temp.path().join("saves/source")).unwrap(),
        b"source unchanged"
    );
    assert_eq!(
        std::fs::read(f.temp.path().join("backups/unregistered")).unwrap(),
        b"unregistered unchanged"
    );
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
}
#[tokio::test]
async fn more_than_preview_budget_never_returns_a_truncated_candidate_list() {
    let f = Fixture::new().await;
    f.seed(5001).await;
    assert_eq!(
        f.hub.preview_retention(zero()).await.unwrap_err().code,
        "RETENTION_LIMIT_EXCEEDED"
    );
}
#[tokio::test]
async fn malformed_date_does_not_silently_disappear_from_the_retention_scope() {
    let f = Fixture::new().await;
    let ids = f.seed(2).await;
    sqlx::query("UPDATE backup_archive SET created_at='invalid' WHERE id=?")
        .bind(&ids[0])
        .execute(&f.db)
        .await
        .unwrap();
    assert_eq!(
        f.hub.preview_retention(zero()).await.unwrap_err().code,
        "RETENTION_METADATA_INVALID"
    );
}
#[tokio::test]
async fn preview_requires_an_initialized_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let hub = Hub::open(&PathBuf::from(temp.path()).join("metadata"))
        .await
        .unwrap();
    assert_eq!(
        hub.preview_retention(zero()).await.unwrap_err().code,
        "WORKSPACE_REQUIRED"
    );
}
