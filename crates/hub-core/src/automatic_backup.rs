//! Opt-in backup of a controlled instance after a normal process exit.
use crate::{
    Hub, HubError, Result, RetentionPolicy,
    database::{id, now},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AutomaticBackupSettings {
    pub enabled: bool,
    pub min_interval_minutes: u32,
    pub retention: RetentionPolicy,
}
impl Default for AutomaticBackupSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_interval_minutes: 60,
            retention: RetentionPolicy {
                keep_recent: 7,
                daily_days: 14,
                weekly_weeks: 8,
            },
        }
    }
}
#[derive(Debug, Serialize, TS)]
pub struct AutomaticBackupPolicy {
    pub instance_id: String,
    #[ts(type = "number")]
    pub revision: i64,
    pub settings: AutomaticBackupSettings,
    pub updated_at: String,
    pub last_status: Option<String>,
    pub last_task_id: Option<String>,
    pub last_error_code: Option<String>,
}
fn invalid() -> HubError {
    HubError::new(
        "AUTO_BACKUP_POLICY_INVALID",
        "自动备份间隔上限10080分钟；保留规则上限为最近1000份、每日366天、每周104周。",
    )
}
impl AutomaticBackupSettings {
    fn validate(&self) -> Result<()> {
        if self.min_interval_minutes > 10080
            || self.retention.keep_recent > 1000
            || self.retention.daily_days > 366
            || self.retention.weekly_weeks > 104
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Hub {
    pub async fn automatic_backup_policy(
        &self,
        instance_id: &str,
    ) -> Result<AutomaticBackupPolicy> {
        self.instance(instance_id).await?;
        // Read the policy and absent-policy epoch in one SQLite snapshot.
        // Separate reads could return an old default with a newly created policy's token.
        let (revision, encoded, updated, epoch): (Option<i64>, Option<String>, Option<String>, i64) =
            sqlx::query_as("SELECT p.revision,p.settings,p.updated_at,h.revision FROM hub_preferences h LEFT JOIN automatic_backup_policy p ON p.instance_id=? WHERE h.id=1")
                .bind(instance_id).fetch_one(&self.pool).await.map_err(HubError::db)?;
        let (revision, settings, updated_at) = match (revision, encoded, updated) {
            (Some(r), Some(s), Some(t)) => (r, serde_json::from_str(&s).map_err(|_| invalid())?, t),
            (None, None, None) => (epoch, AutomaticBackupSettings::default(), String::new()),
            _ => return Err(invalid()),
        };
        let last: Option<(String,Option<String>,Option<String>)> = sqlx::query_as("SELECT COALESCE(t.state,r.status),r.task_id,COALESCE(t.error_code,r.error_code) FROM automatic_backup_run r LEFT JOIN backup_task t ON t.id=r.task_id WHERE r.instance_id=? ORDER BY r.created_at DESC,r.session_id DESC LIMIT 1").bind(instance_id).fetch_optional(&self.pool).await.map_err(HubError::db)?;
        settings.validate()?;
        let (status, task, error) = last.map_or((None, None, None), |(s, t, e)| (Some(s), t, e));
        Ok(AutomaticBackupPolicy {
            instance_id: instance_id.into(),
            revision,
            settings,
            updated_at,
            last_status: status,
            last_task_id: task,
            last_error_code: error,
        })
    }
    pub async fn save_automatic_backup_policy(
        &self,
        instance_id: &str,
        expected_revision: i64,
        settings: AutomaticBackupSettings,
        confirmed: bool,
    ) -> Result<AutomaticBackupPolicy> {
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请确认自动备份范围与尽力快照风险。",
            ));
        }
        settings.validate()?;
        let _gate = self.mutation.lock().await;
        if settings.enabled {
            self.check_resource_claims().await?;
        }
        if settings.enabled {
            let instance = self.instance(instance_id).await?;
            if self
                .inspect_installation_recognition(instance.installation_id)
                .await?
                .recommended_profile
                .is_none()
            {
                return Err(HubError::new(
                    "AUTO_BACKUP_PROFILE_UNVERIFIED",
                    "此安装未匹配支持的 macOS 原生样本；可保存关闭状态和保留规则，暂不能开启自动备份。",
                ));
            }
        }
        let old = self.automatic_backup_policy(instance_id).await?;
        if old.revision != expected_revision {
            return Err(HubError::new(
                "AUTO_BACKUP_POLICY_CHANGED",
                "策略已变化，请重新读取后保存。",
            ));
        }
        // Use the existing global epoch so reset/recreation cannot reuse a draft token.
        let next_revision = self
            .preferences()
            .await?
            .revision
            .max(old.revision)
            .checked_add(1)
            .ok_or(HubError::new(
                "PREFERENCES_INVALID",
                "配置版本号无效，未保存策略。",
            ))?;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO automatic_backup_policy VALUES (?,?,?,?) ON CONFLICT(instance_id) DO UPDATE SET revision=excluded.revision,settings=excluded.settings,updated_at=excluded.updated_at").bind(instance_id).bind(next_revision).bind(serde_json::to_string(&settings).map_err(|_|invalid())?).bind(now()).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("UPDATE hub_preferences SET revision=? WHERE id=1")
            .bind(next_revision)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(instance_id)
            .bind("backup.policy")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        self.automatic_backup_policy(instance_id).await
    }
    async fn check_automatic_backup_interval(
        &self,
        instance_id: &str,
        source_root: &str,
        minutes: u32,
    ) -> Result<()> {
        let latest: Option<String> = sqlx::query_scalar("SELECT created_at FROM backup_archive WHERE play_instance_id=? AND source_root=? AND verification='verified' ORDER BY created_at DESC LIMIT 1")
            .bind(instance_id).bind(source_root).fetch_optional(&self.pool).await.map_err(HubError::db)?;
        if let Some(time) = latest {
            let at = chrono::DateTime::parse_from_rfc3339(&time).map_err(|_| invalid())?;
            if (chrono::Utc::now() - at.with_timezone(&chrono::Utc)).num_minutes()
                < i64::from(minutes)
            {
                return Err(HubError::new(
                    "AUTO_BACKUP_MIN_INTERVAL",
                    "尚未到最小备份间隔。",
                ));
            }
        }
        Ok(())
    }
    pub(crate) async fn backup_after_exit(self: &Arc<Self>, session_id: &str) -> Result<()> {
        let _gate = self.mutation.lock().await;
        let session = sqlx::query_as::<_, crate::Session>("SELECT * FROM session WHERE id=?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await
            .map_err(HubError::db)?;
        if session.state != "exited_unverified" || session.exit_code != Some(0) {
            return Ok(());
        }
        let Some(instance_id) = session.play_instance_id else {
            return Ok(());
        };
        let policy = self.automatic_backup_policy(&instance_id).await?;
        if !policy.settings.enabled {
            return Ok(());
        }
        // A policy saved after this exit must not authorize a historical event.
        let finished = session
            .finished_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok());
        let configured = chrono::DateTime::parse_from_rfc3339(&policy.updated_at).ok();
        if !finished
            .zip(configured)
            .is_some_and(|(exit, saved)| exit > saved)
        {
            return Ok(());
        }
        let inserted =
            sqlx::query("INSERT OR IGNORE INTO automatic_backup_run VALUES (?,?,?,?,?,?,?)")
                .bind(session_id)
                .bind(&instance_id)
                .bind(policy.revision)
                .bind(Option::<String>::None)
                .bind("checking")
                .bind(Option::<String>::None)
                .bind(now())
                .execute(&self.pool)
                .await
                .map_err(HubError::db)?;
        if inserted.rows_affected() == 0 {
            return Ok(());
        }
        let result = async {
            self.check_resource_claims().await?;
            let instance = self.instance(&instance_id).await?;
            if session.instance_data_root.as_deref() != Some(&instance.data_root)
                || session.installation_id != instance.installation_id
            {
                return Err(HubError::new(
                    "AUTO_BACKUP_CONTEXT_CHANGED",
                    "实例来源已变化，未自动备份。",
                ));
            }
            let recognized = self
                .inspect_installation_recognition(instance.installation_id.clone())
                .await?;
            if recognized.recommended_profile.is_none()
                || recognized.recommended_profile != session.launch_profile_id
            {
                return Err(HubError::new(
                    "AUTO_BACKUP_PROFILE_UNVERIFIED",
                    "仅已识别的受控原生启动方案支持退出后自动备份。",
                ));
            }
            self.check_automatic_backup_interval(
                &instance_id,
                &instance.data_root,
                policy.settings.min_interval_minutes,
            )
            .await?;
            let plan = self.prepare_instance_backup_inner(&instance_id).await?;
            if !plan
                .entries
                .iter()
                .any(|e| e.file_type == "file" && e.relative_path.starts_with("save/"))
            {
                self.backup_plans.lock().await.remove(&plan.id);
                return Err(HubError::new(
                    "AUTO_BACKUP_EMPTY_SAVE",
                    "实例save为空，未将配置备份当作游戏进度保护。",
                ));
            }
            self.create_backup_inner(&plan.id, &plan.plan_digest, session_id)
                .await
        }
        .await;
        let (task, status, error) = match result {
            Ok(task) => (Some(task.id), "started", None),
            Err(e) => (None, "skipped", Some(e.code)),
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query(
            "UPDATE automatic_backup_run SET task_id=?,status=?,error_code=? WHERE session_id=?",
        )
        .bind(&task)
        .bind(status)
        .bind(error)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(session_id)
            .bind("backup.automatic")
            .bind(status)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, Arc<Hub>, String) {
        let t = tempfile::tempdir().unwrap();
        for dir in ["saves", "backups", "content", "game", "instance"] {
            std::fs::create_dir(t.path().join(dir)).unwrap();
        }
        let hub = Hub::open(&t.path().join("metadata")).await.unwrap();
        let path = |s: &str| t.path().join(s).to_str().unwrap().to_owned();
        hub.initialize(crate::WorkspaceInput {
            save_dir: path("saves"),
            backup_dir: path("backups"),
            content_dir: path("content"),
        })
        .await
        .unwrap();
        let exe = t.path().join("game/test.exe");
        std::fs::write(&exe, b"synthetic never executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let game = hub
            .import_installation(exe.to_str().unwrap(), "fixture")
            .await
            .unwrap();
        let i = hub
            .register_play_instance(crate::PlayInstanceInput {
                installation_id: game.id,
                name: "test".into(),
                data_root: path("instance"),
            })
            .await
            .unwrap();
        (t, hub, i.id)
    }
    #[tokio::test]
    async fn policy_requires_consent_revision_and_bounds_and_resets_with_backup() {
        let (t, hub, i) = fixture().await;
        let policy = hub.automatic_backup_policy(&i).await.unwrap();
        assert!(!policy.settings.enabled);
        assert_eq!(policy.revision, 0);
        assert_eq!(
            hub.save_automatic_backup_policy(&i, 0, policy.settings.clone(), false)
                .await
                .unwrap_err()
                .code,
            "CONFIRMATION_REQUIRED"
        );
        let mut invalid = policy.settings.clone();
        invalid.min_interval_minutes = 10081;
        assert_eq!(
            hub.save_automatic_backup_policy(&i, 0, invalid, true)
                .await
                .unwrap_err()
                .code,
            "AUTO_BACKUP_POLICY_INVALID"
        );
        let settings = policy.settings;
        let mut unsupported = settings.clone();
        unsupported.enabled = true;
        assert_eq!(
            hub.save_automatic_backup_policy(&i, 0, unsupported, true)
                .await
                .unwrap_err()
                .code,
            "AUTO_BACKUP_PROFILE_UNVERIFIED"
        );
        let saved = hub
            .save_automatic_backup_policy(&i, 0, settings.clone(), true)
            .await
            .unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(
            hub.save_automatic_backup_policy(&i, 0, settings, true)
                .await
                .unwrap_err()
                .code,
            "AUTO_BACKUP_POLICY_CHANGED"
        );
        // Simulate a previously recognized installation whose policy was enabled.
        let enabled_settings = AutomaticBackupSettings {
            enabled: true,
            ..Default::default()
        };
        sqlx::query("UPDATE automatic_backup_policy SET settings=? WHERE instance_id=?")
            .bind(serde_json::to_string(&enabled_settings).unwrap())
            .bind(&i)
            .execute(&hub.pool)
            .await
            .unwrap();
        assert!(
            hub.automatic_backup_policy(&i)
                .await
                .unwrap()
                .settings
                .enabled
        );
        let prefs = hub.preferences().await.unwrap();
        let reset = hub.reset_preferences(prefs.revision, true).await.unwrap();
        assert!(
            !hub.automatic_backup_policy(&i)
                .await
                .unwrap()
                .settings
                .enabled
        );
        let bytes = std::fs::read(&reset.backup_path).unwrap();
        let snapshot: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(snapshot["automatic_backup_policies"][0][0], i);
        let before: AutomaticBackupSettings = serde_json::from_str(
            snapshot["automatic_backup_policies"][0][2]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert!(before.enabled);
        assert!(hub.automatic_backup_policy(&i).await.unwrap().revision > saved.revision);
        assert!(t.path().join("instance").is_dir());
    }
    async fn seed_enabled(hub: &Hub, instance_id: &str, settings: AutomaticBackupSettings) {
        sqlx::query("INSERT INTO automatic_backup_policy VALUES (?,1,?,?)")
            .bind(instance_id)
            .bind(serde_json::to_string(&settings).unwrap())
            .bind(now())
            .execute(&hub.pool)
            .await
            .unwrap();
    }
    async fn seed_session(hub: &Hub, i: &str, code: Option<i64>, state: &str) -> String {
        let instance = hub.instance(i).await.unwrap();
        let session = id();
        sqlx::query("INSERT INTO session (id,installation_id,state,exit_code,started_at,finished_at,play_instance_id,launch_profile_id,instance_data_root) VALUES (?,?,?,?,?,?,?,?,?)").bind(&session).bind(instance.installation_id).bind(state).bind(code).bind(now()).bind(now()).bind(i).bind("cdda-0i-1-macos-trial-v2").bind(instance.data_root).execute(&hub.pool).await.unwrap();
        session
    }
    #[tokio::test]
    async fn disabled_unknown_exit_and_unrecognized_program_never_create_archives_or_retry() {
        let (_temp, hub, i) = fixture().await;
        let session = seed_session(&hub, &i, Some(0), "exited_unverified").await;
        hub.backup_after_exit(&session).await.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM automatic_backup_run")
            .fetch_one(&hub.pool)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let settings = AutomaticBackupSettings {
            enabled: true,
            ..Default::default()
        };
        seed_enabled(&hub, &i, settings).await;
        let session = seed_session(&hub, &i, Some(0), "exited_unverified").await;
        let unknown = seed_session(&hub, &i, None, "exited_unverified").await;
        hub.backup_after_exit(&unknown).await.unwrap();
        hub.backup_after_exit(&session).await.unwrap();
        hub.backup_after_exit(&session).await.unwrap();
        let p = hub.automatic_backup_policy(&i).await.unwrap();
        assert_eq!(
            p.last_error_code.as_deref(),
            Some("AUTO_BACKUP_PROFILE_UNVERIFIED")
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM automatic_backup_run")
            .fetch_one(&hub.pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert!(hub.dashboard().await.unwrap().backups.is_empty());
    }
    #[tokio::test]
    async fn active_sessions_block_enabling_but_allow_disabling() {
        let (_temp, hub, i) = fixture().await;
        let session = seed_session(&hub, &i, None, "running").await;
        let settings = AutomaticBackupSettings {
            enabled: true,
            ..Default::default()
        };
        assert!(
            hub.save_automatic_backup_policy(&i, 0, settings, true)
                .await
                .is_err()
        );
        hub.save_automatic_backup_policy(&i, 0, AutomaticBackupSettings::default(), true)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM session WHERE id=?")
                .bind(session)
                .fetch_one(&hub.pool)
                .await
                .unwrap(),
            "running"
        );
    }
    #[tokio::test]
    async fn schema_sixteen_upgrade_backs_up_and_preserves_instance_identity() {
        let (t, hub, i) = fixture().await;
        sqlx::raw_sql("DROP TABLE automatic_backup_run; DROP TABLE automatic_backup_policy; PRAGMA user_version=16;").execute(&hub.pool).await.unwrap();
        drop(hub);
        let hub = Hub::open(&t.path().join("metadata")).await.unwrap();
        assert_eq!(hub.instance(&i).await.unwrap().id, i);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&hub.pool)
                .await
                .unwrap(),
            17
        );
        assert!(
            std::fs::read_dir(t.path().join("metadata"))
                .unwrap()
                .any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("metadata-v16-"))
        );
    }
    #[tokio::test]
    async fn other_running_session_blocks_automatic_work_and_retries() {
        let (_temp, hub, i) = fixture().await;
        seed_enabled(
            &hub,
            &i,
            AutomaticBackupSettings {
                enabled: true,
                ..Default::default()
            },
        )
        .await;
        let exited = seed_session(&hub, &i, Some(0), "exited_unverified").await;
        seed_session(&hub, &i, None, "running").await;
        hub.backup_after_exit(&exited).await.unwrap();
        hub.backup_after_exit(&exited).await.unwrap();
        let p = hub.automatic_backup_policy(&i).await.unwrap();
        assert_eq!(p.last_status.as_deref(), Some("skipped"));
        assert!(p.last_error_code.is_some());
        assert!(hub.dashboard().await.unwrap().backup_tasks.is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM automatic_backup_run")
                .fetch_one(&hub.pool)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn failed_schema_seventeen_upgrade_rolls_back_and_keeps_schema_sixteen_backup() {
        let (t, hub, _i) = fixture().await;
        sqlx::raw_sql("DROP TABLE automatic_backup_run; DROP TABLE automatic_backup_policy; CREATE TABLE automatic_backup_run (sentinel TEXT); INSERT INTO automatic_backup_run VALUES ('preserve'); PRAGMA user_version=16;").execute(&hub.pool).await.unwrap();
        drop(hub);
        assert_eq!(
            Hub::open(&t.path().join("metadata"))
                .await
                .err()
                .unwrap()
                .code,
            "DB_MIGRATION_FAILED"
        );
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(t.path().join("metadata/metadata.sqlite")),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&pool)
                .await
                .unwrap(),
            16
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT sentinel FROM automatic_backup_run")
                .fetch_one(&pool)
                .await
                .unwrap(),
            "preserve"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sqlite_master WHERE name='automatic_backup_policy'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn interval_uses_only_verified_backups_of_this_instance_and_source() {
        let (t, hub, i) = fixture().await;
        std::fs::write(t.path().join("instance/protected.dat"), b"protected bytes").unwrap();
        let source = hub.instance(&i).await.unwrap().data_root;
        hub.check_automatic_backup_interval(&i, &source, 60)
            .await
            .unwrap();
        let p = hub.prepare_instance_backup(&i).await.unwrap();
        let task = hub
            .create_backup(&p.id, &p.plan_digest, &id(), true)
            .await
            .unwrap();
        for _ in 0..1000 {
            let task = hub.backup_task(&task.id).await.unwrap();
            if task.state == "succeeded" {
                break;
            }
            assert!(matches!(task.state.as_str(), "running" | "cancelling"));
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(hub.backup_task(&task.id).await.unwrap().state, "succeeded");
        assert_eq!(
            hub.check_automatic_backup_interval(&i, &source, 60)
                .await
                .unwrap_err()
                .code,
            "AUTO_BACKUP_MIN_INTERVAL"
        );
        hub.check_automatic_backup_interval(&i, &source, 0)
            .await
            .unwrap();
        hub.check_automatic_backup_interval(&i, "/unrelated", 60)
            .await
            .unwrap();
        sqlx::query("UPDATE backup_archive SET verification='failed'")
            .execute(&hub.pool)
            .await
            .unwrap();
        hub.check_automatic_backup_interval(&i, &source, 60)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn reset_never_reuses_policy_revisions_for_old_drafts() {
        for existing in [false, true] {
            let (_temp, hub, i) = fixture().await;
            if existing {
                hub.save_automatic_backup_policy(&i, 0, AutomaticBackupSettings::default(), true)
                    .await
                    .unwrap();
            }
            let old = hub.automatic_backup_policy(&i).await.unwrap();
            let prefs = hub.preferences().await.unwrap();
            hub.reset_preferences(prefs.revision, true).await.unwrap();
            if existing {
                let fresh = hub.automatic_backup_policy(&i).await.unwrap();
                hub.save_automatic_backup_policy(
                    &i,
                    fresh.revision,
                    AutomaticBackupSettings::default(),
                    true,
                )
                .await
                .unwrap();
            }
            let mut stale = old.settings;
            stale.min_interval_minutes = 999;
            assert_eq!(
                hub.save_automatic_backup_policy(&i, old.revision, stale, true)
                    .await
                    .unwrap_err()
                    .code,
                "AUTO_BACKUP_POLICY_CHANGED"
            );
        }
    }
    #[tokio::test]
    async fn enabling_a_policy_does_not_backfill_a_prior_exit() {
        let (_temp, hub, i) = fixture().await;
        let session = seed_session(&hub, &i, Some(0), "exited_unverified").await;
        seed_enabled(
            &hub,
            &i,
            AutomaticBackupSettings {
                enabled: true,
                ..Default::default()
            },
        )
        .await;
        hub.backup_after_exit(&session).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM automatic_backup_run")
                .fetch_one(&hub.pool)
                .await
                .unwrap(),
            0
        );
        assert!(hub.dashboard().await.unwrap().backup_tasks.is_empty());
    }
    #[tokio::test]
    async fn policy_reads_keep_reset_epochs_and_settings_in_one_snapshot() {
        let (_temp, hub, i) = fixture().await;
        let writer = Arc::clone(&hub);
        let instance_id = i.clone();
        let settings = serde_json::to_string(&AutomaticBackupSettings::default()).unwrap();
        tokio::join!(
            async {
                for epoch in 1_i64..=32 {
                    let mut tx = writer.pool.begin().await.unwrap();
                    sqlx::query("UPDATE hub_preferences SET revision=? WHERE id=1")
                        .bind(epoch)
                        .execute(&mut *tx)
                        .await
                        .unwrap();
                    if epoch % 2 == 1 {
                        sqlx::query("INSERT INTO automatic_backup_policy VALUES (?,?,?,?)")
                            .bind(&instance_id)
                            .bind(epoch)
                            .bind(&settings)
                            .bind(now())
                            .execute(&mut *tx)
                            .await
                            .unwrap();
                    } else {
                        sqlx::query("DELETE FROM automatic_backup_policy WHERE instance_id=?")
                            .bind(&instance_id)
                            .execute(&mut *tx)
                            .await
                            .unwrap();
                    }
                    tx.commit().await.unwrap();
                    tokio::task::yield_now().await;
                }
            },
            async {
                for _ in 0..64 {
                    let p = hub.automatic_backup_policy(&i).await.unwrap();
                    assert_eq!(
                        p.updated_at.is_empty(),
                        p.revision % 2 == 0,
                        "mixed policy and epoch snapshot"
                    );
                    tokio::task::yield_now().await;
                }
            }
        );
    }
}
