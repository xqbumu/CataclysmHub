use crate::{
    Dashboard, HubError, Installation, OperationLog, Result, Session, Workspace, WorkspaceInput,
    paths,
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};
use tokio::sync::Mutex;

pub struct Hub {
    pub(crate) pool: SqlitePool,
    pub(crate) data_dir: PathBuf,
    pub(crate) mutation: Mutex<()>,
    pub(crate) candidate_sessions:
        Mutex<HashMap<String, Arc<crate::candidate_launch::CandidateBinding>>>,
    pub(crate) plans: Mutex<HashMap<String, crate::process::PreparedLaunch>>,
    pub(crate) config_change_running: Mutex<std::collections::HashSet<String>>,
    pub(crate) config_change_plans:
        Mutex<HashMap<String, crate::config_change::PreparedConfigChange>>,
    pub(crate) restore_plans: Mutex<HashMap<String, crate::managed_restore::PreparedRestore>>,
    pub(crate) backup_plans: Mutex<HashMap<String, crate::backup::PreparedPlan>>,
    pub(crate) runtime_recovery: Mutex<HashMap<String, crate::task_recovery::RuntimeFailure>>,
    pub(crate) scan_controls: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) copy_plans: Mutex<HashMap<String, crate::managed_installation_copy::PreparedCopy>>,
    pub(crate) copy_controls: Mutex<HashMap<String, Arc<crate::installation_copy::CopyControl>>>,
    pub(crate) content_install_plans:
        Mutex<HashMap<String, Arc<crate::content_install::PreparedContentInstall>>>,
    _lock: WorkspaceLock,
}
// Only this guard owns the lock lifecycle. A transient descriptor inherited
// during process creation must not extend it beyond the final Hub owner.
struct WorkspaceLock(File);
impl WorkspaceLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|_| HubError::new("PERMISSION_DENIED", "无法打开工作区锁。"))?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
            HubError::new(
                "WORKSPACE_LOCKED",
                "另一个 Cataclysm Hub 实例正在使用此工作区。",
            )
        })?;
        Ok(Self(file))
    }
}
impl Drop for WorkspaceLock {
    fn drop(&mut self) {
        // Closing one descriptor does not release a Unix flock while a dup/fork
        // reference survives. Explicit unlock releases our shared lock first.
        if fs2::FileExt::unlock(&self.0).is_err() {
            eprintln!("WORKSPACE_UNLOCK_FAILED");
        }
    }
}

pub(crate) fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub(crate) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Hub {
    pub async fn open(data_dir: &Path) -> Result<Arc<Self>> {
        std::fs::create_dir_all(data_dir)
            .map_err(|_| HubError::new("PERMISSION_DENIED", "无法创建应用元数据目录。"))?;
        let data_dir = std::fs::canonicalize(data_dir)
            .map_err(|_| HubError::new("PATH_INVALID", "无法访问应用元数据目录。"))?;
        let lock = WorkspaceLock::acquire(&data_dir.join("workspace.lock"))?;
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(data_dir.join("metadata.sqlite"))
                    .create_if_missing(true)
                    .foreign_keys(true)
                    .busy_timeout(Duration::from_secs(5)),
            )
            .await
            .map_err(HubError::db)?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .map_err(HubError::db)?;
        if version > 15 {
            return Err(HubError::new(
                "SCHEMA_UNSUPPORTED",
                "元数据来自更新版本，当前应用不会修改它。",
            ));
        }
        let integrity: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await
            .map_err(HubError::db)?;
        if integrity != "ok" {
            return Err(HubError::new(
                "DATABASE_CORRUPT",
                "元数据校验失败，请保留数据库并恢复元数据备份。",
            ));
        }
        if version == 0 {
            let mut tx = pool.begin().await.map_err(HubError::db)?;
            sqlx::raw_sql(include_str!("schema.sql"))
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
            tx.commit().await.map_err(HubError::db)?;
        }
        if version > 0 && version < 15 {
            let backup = data_dir.join(format!("metadata-v{version}-{}.sqlite", id()));
            sqlx::query("VACUUM INTO ?")
                .bind(paths::text(&backup)?)
                .execute(&pool)
                .await
                .map_err(|_| {
                    HubError::new(
                        "MIGRATION_BACKUP_FAILED",
                        "无法创建迁移前元数据备份，升级已中止。",
                    )
                })?;
            let snapshot = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(
                    SqliteConnectOptions::new()
                        .filename(&backup)
                        .read_only(true),
                )
                .await
                .map_err(HubError::db)?;
            let check: String = sqlx::query_scalar("PRAGMA quick_check")
                .fetch_one(&snapshot)
                .await
                .map_err(HubError::db)?;
            snapshot.close().await;
            if check != "ok" {
                return Err(HubError::new(
                    "MIGRATION_BACKUP_FAILED",
                    "元数据备份校验失败。",
                ));
            }
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&backup)
                .and_then(|file| file.sync_all())
                .map_err(|_| {
                    HubError::new("MIGRATION_BACKUP_FAILED", "无法持久保存元数据备份。")
                })?;
        }
        if version < 15 {
            let mut tx = pool.begin().await.map_err(HubError::db)?;
            if version < 2 {
                sqlx::raw_sql(include_str!("migration-002.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 3 {
                sqlx::raw_sql(include_str!("migration-003.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 4 {
                sqlx::raw_sql(include_str!("migration-004.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 5 {
                sqlx::raw_sql(include_str!("migration-005.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 6 {
                sqlx::raw_sql(include_str!("migration-006.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 7 {
                sqlx::raw_sql(include_str!("migration-007.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 8 {
                sqlx::raw_sql(include_str!("migration-008.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 9 {
                sqlx::raw_sql(include_str!("migration-009.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 10 {
                sqlx::raw_sql(include_str!("migration-010.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 11 {
                sqlx::raw_sql(include_str!("migration-011.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 12 {
                sqlx::raw_sql(include_str!("migration-012.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 13 {
                sqlx::raw_sql(include_str!("migration-013.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            if version < 14 {
                sqlx::raw_sql(include_str!("migration-014.sql"))
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| {
                        HubError::new(
                            "DB_MIGRATION_FAILED",
                            "元数据升级失败，原版本和备份已保留。",
                        )
                    })?;
            }
            sqlx::raw_sql(include_str!("migration-015.sql"))
                .execute(&mut *tx)
                .await
                .map_err(|_| {
                    HubError::new(
                        "DB_MIGRATION_FAILED",
                        "元数据升级失败，原版本和备份已保留。",
                    )
                })?;
            tx.commit().await.map_err(HubError::db)?;
        }
        let hub = Arc::new(Self {
            pool,
            data_dir,
            mutation: Mutex::new(()),
            config_change_running: Mutex::new(std::collections::HashSet::new()),
            config_change_plans: Mutex::new(HashMap::new()),
            content_install_plans: Mutex::new(HashMap::new()),
            plans: Mutex::new(HashMap::new()),
            candidate_sessions: Mutex::new(HashMap::new()),
            backup_plans: Mutex::new(HashMap::new()),
            restore_plans: Mutex::new(HashMap::new()),
            scan_controls: Mutex::new(HashMap::new()),
            runtime_recovery: Mutex::new(HashMap::new()),
            copy_plans: Mutex::new(HashMap::new()),
            copy_controls: Mutex::new(HashMap::new()),
            _lock: lock,
        });
        // A previous child may still be alive. Never infer exit or a stable save from app restart.
        let mut tx = hub.pool.begin().await.map_err(HubError::db)?;
        let interrupted_config: Vec<String> =
            sqlx::query_scalar("SELECT id FROM config_change WHERE state='applying'")
                .fetch_all(&mut *tx)
                .await
                .map_err(HubError::db)?;
        for task in interrupted_config {
            sqlx::query("UPDATE config_change SET state='recovery_required',error_code='CONFIG_CHANGE_INTERRUPTED' WHERE id=?").bind(&task).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("config.apply")
                .bind("recovery_required")
                .bind("CONFIG_CHANGE_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let interrupted: Vec<String> =
            sqlx::query_scalar("SELECT id FROM session WHERE state IN ('starting','running')")
                .fetch_all(&mut *tx)
                .await
                .map_err(HubError::db)?;
        sqlx::query(
            "UPDATE session SET state='interrupted_unknown' WHERE state IN ('starting','running')",
        )
        .execute(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for task in interrupted {
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("process.reconcile")
                .bind("interrupted_unknown")
                .bind("PROCESS_STATUS_UNKNOWN")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let interrupted_tasks: Vec<String> =
            sqlx::query_scalar("SELECT id FROM task WHERE state IN ('running','cancelling')")
                .fetch_all(&mut *tx)
                .await
                .map_err(HubError::db)?;
        for task_id in interrupted_tasks {
            sqlx::query("UPDATE task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,revision=revision+1 WHERE id=?")
                .bind(now()).bind(&task_id).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task_id)
                .bind("world.scan")
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let interrupted_backups: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,operation FROM backup_task WHERE state IN ('running','cancelling')",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for (task_id, operation) in interrupted_backups {
            sqlx::query("UPDATE backup_task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,revision=revision+1 WHERE id=?")
                .bind(now()).bind(&task_id).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task_id)
                .bind(operation)
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let interrupted_restores: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM restore_task WHERE state IN ('running','cancelling')",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for task in interrupted_restores {
            sqlx::query("UPDATE restore_task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,revision=revision+1 WHERE id=?").bind(now()).bind(&task).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("backup.restore_new")
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let interrupted_inspections: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM inspection_task WHERE state IN ('running','cancelling')",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for task in interrupted_inspections {
            sqlx::query("UPDATE inspection_task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,result_json=NULL,revision=revision+1 WHERE id=?").bind(now()).bind(&task).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("backup.inspect_restored")
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let copies: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM installation_copy_task WHERE state IN ('running','cancelling')",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for task in copies {
            sqlx::query("UPDATE installation_copy_task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,revision=revision+1 WHERE id=?").bind(now()).bind(&task).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("installation.copy")
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        let copies: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM content_install_task WHERE state IN ('running','cancelling')",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for task in copies {
            sqlx::query("UPDATE content_install_task SET state='recovery_required',phase='interrupted',error_code='TASK_INTERRUPTED',finished_at=?,revision=revision+1 WHERE id=?").bind(now()).bind(&task).execute(&mut *tx).await.map_err(HubError::db)?;
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(task)
                .bind("content.install")
                .bind("recovery_required")
                .bind("TASK_INTERRUPTED")
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        tx.commit().await.map_err(HubError::db)?;
        Ok(hub)
    }

    pub async fn dashboard(&self) -> Result<Dashboard> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let workspace = sqlx::query_as::<_, Workspace>("SELECT * FROM workspace LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(HubError::db)?;
        let mut installations = sqlx::query_as::<_, Installation>(
            "SELECT * FROM installation ORDER BY imported_at DESC, id",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        for installation in &mut installations {
            crate::installation_recognition::describe_registered(installation);
        }
        let sessions = sqlx::query_as::<_, Session>(
            "SELECT * FROM session ORDER BY started_at DESC, id LIMIT 50",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let operations = sqlx::query_as::<_, OperationLog>(
            "SELECT * FROM operation_log ORDER BY created_at DESC, id LIMIT 100",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let mut scan_tasks = sqlx::query_as::<_, crate::ScanTask>("SELECT id,request_key,workspace_id,operation,state,phase,processed_entries,revision,error_code,created_at,finished_at FROM task ORDER BY created_at DESC,id LIMIT 20")
            .fetch_all(&mut *tx).await.map_err(HubError::db)?;
        let mut backup_tasks = sqlx::query_as::<_, crate::BackupTask>("SELECT id,request_key,workspace_id,backup_id,operation,state,phase,processed_entries,revision,error_code,created_at,finished_at,play_instance_id,source_root FROM backup_task ORDER BY created_at DESC,id LIMIT 20").fetch_all(&mut *tx).await.map_err(HubError::db)?;
        let backups = sqlx::query_as::<_, crate::BackupRecord>(
            "SELECT * FROM backup_archive ORDER BY created_at DESC,id LIMIT 100",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let mut restore_tasks = sqlx::query_as::<_, crate::RestoreTask>(&format!(
            "SELECT {} FROM restore_task ORDER BY created_at DESC,id LIMIT 20",
            crate::managed_restore::COLUMNS
        ))
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let inspection_rows = sqlx::query_as::<_, crate::managed_inspection::InspectionRow>(
            "SELECT * FROM inspection_task ORDER BY created_at DESC,id LIMIT 50",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let mut inspection_tasks = inspection_rows
            .into_iter()
            .map(|row| row.into_task())
            .collect::<Result<Vec<_>>>()?;
        let mut copy_tasks = sqlx::query_as::<_, crate::InstallationCopyTask>(
            "SELECT * FROM installation_copy_task ORDER BY created_at DESC,id LIMIT 20",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
        let mut resource_claims = crate::resource_claims::read_claims(&mut tx).await?;
        tx.commit().await.map_err(HubError::db)?;
        self.overlay_claims(&mut resource_claims).await;
        for task in &mut copy_tasks {
            self.overlay_copy(task).await;
        }
        for task in &mut scan_tasks {
            self.overlay_scan(task).await;
        }
        for task in &mut backup_tasks {
            self.overlay_backup(task).await;
        }
        for task in &mut restore_tasks {
            self.overlay_restore(task).await;
        }
        for task in &mut inspection_tasks {
            self.overlay_inspection(task).await;
        }
        Ok(Dashboard {
            copy_tasks,
            resource_claims,
            workspace,
            installations,
            play_instances: self.play_instances().await?,
            sessions,
            operations,
            scan_tasks,
            backup_tasks,
            backups,
            restore_tasks,
            inspection_tasks,
        })
    }

    pub async fn initialize(&self, input: WorkspaceInput) -> Result<Workspace> {
        let _guard = self.mutation.lock().await;
        if self.dashboard().await?.workspace.is_some() {
            return Err(HubError::new(
                "WORKSPACE_EXISTS",
                "工作区已配置，不会覆盖现有路径。",
            ));
        }
        let save = paths::canonical(&input.save_dir, true)?;
        let backup = paths::canonical(&input.backup_dir, true)?;
        let content = paths::canonical(&input.content_dir, true)?;
        let dirs = [&save, &backup, &content, &self.data_dir];
        for (index, a) in dirs.iter().enumerate() {
            for b in dirs.iter().skip(index + 1) {
                if paths::overlap(a, b) {
                    return Err(HubError::new(
                        "PATH_OVERLAP",
                        "存档、备份、资源和应用元数据目录必须分离，且不能相互嵌套。",
                    ));
                }
            }
        }
        std::fs::read_dir(&save)
            .map_err(|_| HubError::new("PERMISSION_DENIED", "存档目录不可读。"))?;
        paths::writable(&backup)?;
        paths::writable(&content)?;
        let workspace = Workspace {
            id: id(),
            save_dir: paths::text(&save)?,
            backup_dir: paths::text(&backup)?,
            content_dir: paths::text(&content)?,
            created_at: now(),
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO workspace VALUES (?,?,?,?,?)")
            .bind(&workspace.id)
            .bind(&workspace.save_dir)
            .bind(&workspace.backup_dir)
            .bind(&workspace.content_dir)
            .bind(&workspace.created_at)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&workspace.id)
            .bind("workspace.initialize")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(workspace)
    }

    pub async fn acknowledge_interrupted(&self, session_id: &str) -> Result<()> {
        let _guard = self.mutation.lock().await;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let changed = sqlx::query("UPDATE session SET state='resolved_unknown' WHERE id=? AND state='interrupted_unknown'")
            .bind(session_id).execute(&mut *tx).await.map_err(HubError::db)?.rows_affected();
        if changed != 1 {
            return Err(HubError::new(
                "SESSION_STATE_CHANGED",
                "该会话已更新，请刷新状态。",
            ));
        }
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(session_id)
            .bind("process.user_acknowledged")
            .bind("resolved_unknown")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
}

#[cfg(test)]
mod lock_regression {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_hub_unlocks_despite_a_transient_duplicate() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("metadata");
        let hub = Hub::open(&data).await.unwrap();
        let duplicate = hub._lock.0.try_clone().unwrap();
        assert_eq!(Arc::strong_count(&hub), 1);
        assert_eq!(
            Hub::open(&data).await.err().unwrap().code,
            "WORKSPACE_LOCKED"
        );
        drop(hub);
        let reopened = Hub::open(&data)
            .await
            .expect("final owner drop must explicitly release its shared kernel lock");
        drop(duplicate);
        assert_eq!(
            Hub::open(&data).await.err().unwrap().code,
            "WORKSPACE_LOCKED"
        );
        drop(reopened);
        Hub::open(&data).await.unwrap();
    }
    #[tokio::test]
    async fn shared_hub_owners_keep_the_lock_until_the_last_owner_drops() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("metadata");
        let hub = Hub::open(&data).await.unwrap();
        let worker_owner = Arc::clone(&hub);
        drop(hub);
        assert_eq!(
            Hub::open(&data).await.err().unwrap().code,
            "WORKSPACE_LOCKED"
        );
        drop(worker_owner);
        Hub::open(&data).await.unwrap();
    }
}
