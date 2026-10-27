use crate::{
    Dashboard, HubError, Installation, LaunchPlan, OperationLog, Result, Session, Workspace,
    WorkspaceInput, paths,
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

pub struct Hub {
    pub(crate) pool: SqlitePool,
    pub(crate) data_dir: PathBuf,
    pub(crate) mutation: Mutex<()>,
    pub(crate) plans: Mutex<HashMap<String, LaunchPlan>>,
    _lock: File,
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
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data_dir.join("workspace.lock"))
            .map_err(|_| HubError::new("PERMISSION_DENIED", "无法打开工作区锁。"))?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| {
            HubError::new(
                "WORKSPACE_LOCKED",
                "另一个 Cataclysm Hub 实例正在使用此工作区。",
            )
        })?;
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
        if version > 1 {
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
        let hub = Arc::new(Self {
            pool,
            data_dir,
            mutation: Mutex::new(()),
            plans: Mutex::new(HashMap::new()),
            _lock: lock,
        });
        // A previous child may still be alive. Never infer exit or a stable save from app restart.
        let mut tx = hub.pool.begin().await.map_err(HubError::db)?;
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
        tx.commit().await.map_err(HubError::db)?;
        Ok(hub)
    }

    pub async fn dashboard(&self) -> Result<Dashboard> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let workspace = sqlx::query_as::<_, Workspace>("SELECT * FROM workspace LIMIT 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(HubError::db)?;
        let installations = sqlx::query_as::<_, Installation>(
            "SELECT * FROM installation ORDER BY imported_at DESC, id",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(HubError::db)?;
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
        tx.commit().await.map_err(HubError::db)?;
        Ok(Dashboard {
            workspace,
            installations,
            sessions,
            operations,
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
