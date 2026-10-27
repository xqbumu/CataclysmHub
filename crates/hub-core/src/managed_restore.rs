use crate::{
    BackupRecord, Hub, HubError, RestorePlan, RestoreTask, Result,
    backup_format::{io_error, sha, verify_dir},
    database::{id, now},
    paths,
    restore::{PendingRestore, RestoreDestination, stage_restore},
};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

pub(crate) const COLUMNS: &str = "id,request_key,workspace_id,backup_id,destination,state,phase,revision,error_code,created_at,finished_at";
pub(crate) struct PreparedRestore {
    plan: RestorePlan,
    workspace_id: String,
    source: Dir,
    parent: Dir,
    name: String,
}
fn stale() -> HubError {
    HubError::new(
        "PLAN_STALE",
        "恢复预览已失效或路径发生变化，请重新选择并预览。",
    )
}
fn phase_name(phase: u8) -> &'static str {
    match phase {
        1 => "verifying_archive",
        2 => "extracting",
        3 => "checking_restored",
        _ => "preflight",
    }
}
fn same_parent(parent: &Dir, path: &Path) -> Result<()> {
    let held = same_file::Handle::from_file(parent.try_clone().map_err(io_error)?.into_std_file())
        .map_err(io_error)?;
    let current = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(io_error)?;
    let current = same_file::Handle::from_file(current.into_std_file()).map_err(io_error)?;
    if held != current {
        return Err(stale());
    }
    Ok(())
}
fn source_failure_status(code: &str) -> Option<&'static str> {
    match code {
        "HASH_MISMATCH" | "ARCHIVE_INVALID" | "SOURCE_CHANGED" => Some("failed"),
        "BACKUP_IO_FAILED" | "PERMISSION_DENIED" => Some("unavailable"),
        _ => None,
    }
}
impl Hub {
    pub async fn prepare_restore(&self, backup_id: &str, parent_dir: &str) -> Result<RestorePlan> {
        let _gate = self.mutation.lock().await;
        self.check_sessions("").await?;
        let workspace = self.dashboard().await?.workspace.ok_or_else(stale)?;
        let record: BackupRecord =
            sqlx::query_as("SELECT * FROM backup_archive WHERE id=? AND workspace_id=?")
                .bind(backup_id)
                .bind(&workspace.id)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
                .ok_or(HubError::new("BACKUP_NOT_FOUND", "备份记录不存在。"))?;
        if record.relative_dir != record.id || uuid::Uuid::parse_str(&record.id).is_err() {
            return Err(stale());
        }
        let parent_path = paths::canonical(parent_dir, true)?;
        let parent_text = paths::text(&parent_path)?;
        let plan_id = id();
        let name = format!("restored-{plan_id}");
        let destination = parent_path.join(&name);
        for protected in [
            &workspace.save_dir,
            &workspace.backup_dir,
            &workspace.content_dir,
            &paths::text(&self.data_dir)?,
        ] {
            let root = paths::canonical(protected, true)?;
            if root != Path::new(protected) || paths::overlap(&destination, &root) {
                return Err(HubError::new(
                    "RESTORE_PATH_OVERLAP",
                    "恢复目标不能位于当前存档、备份、资源或元数据目录中。",
                ));
            }
        }
        let backup_root = workspace.backup_dir.clone();
        let job_record = record.clone();
        let checked = tokio::task::spawn_blocking(move || {
            let base = Dir::open_ambient_dir(&backup_root, cap_std::ambient_authority())
                .map_err(io_error)?;
            let source = base
                .open_dir_nofollow(&job_record.relative_dir)
                .map_err(io_error)?;
            let (_, verification) = verify_dir(
                &source,
                Some(&job_record.archive_sha256),
                &AtomicBool::new(false),
            )?;
            if verification.backup_id != job_record.id
                || verification.manifest_sha256 != job_record.manifest_sha256
            {
                return Err(crate::backup_format::invalid());
            }
            Ok((source, verification))
        })
        .await
        .map_err(|_| HubError::new("RESTORE_WORKER_FAILED", "恢复预检线程中断。"))?;
        let (source, verification) = match checked {
            Ok(checked) => checked,
            Err(error) => {
                self.record_restore_source_failure(&record.id, &error)
                    .await?;
                return Err(error);
            }
        };
        let parent =
            Dir::open_ambient_dir(&parent_text, cap_std::ambient_authority()).map_err(io_error)?;
        let bytes = verification
            .total_bytes
            .parse::<u64>()
            .map_err(|_| stale())?;
        if fs2::available_space(&parent_text).map_err(io_error)?
            < bytes.saturating_add(16 * 1024 * 1024)
        {
            return Err(HubError::new("DISK_FULL", "目标磁盘可用空间不足。"));
        }
        let mut plan = RestorePlan {
            id: plan_id,
            plan_digest: String::new(),
            backup_id: record.id,
            destination: paths::text(&destination)?,
            parent_dir: parent_text,
            entry_count: verification.entry_count,
            file_count: verification.file_count,
            total_bytes: verification.total_bytes,
            consistency: verification.consistency,
            archive_sha256: verification.archive_sha256,
            manifest_sha256: verification.manifest_sha256,
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            warnings: vec![
                "仅恢复到新建目录的 data/，不会切换当前存档。".into(),
                "文件校验不代表游戏可加载或跨版本兼容；原备份一致性等级保持不变。".into(),
                "失败或取消保留未完成目录，不自动清理或重新执行。".into(),
            ],
        };
        plan.plan_digest = sha(&serde_json::to_vec(&plan).map_err(|_| stale())?);
        let mut plans = self.restore_plans.lock().await;
        plans.retain(|_, prepared| prepared.plan.expires_at > now());
        if plans.len() >= 32 {
            return Err(HubError::new("PLAN_LIMIT", "待确认预览过多，请稍后重试。"));
        }
        plans.insert(
            plan.id.clone(),
            PreparedRestore {
                plan: plan.clone(),
                workspace_id: workspace.id,
                source,
                parent,
                name,
            },
        );
        Ok(plan)
    }
    pub async fn start_restore(
        self: &Arc<Self>,
        plan_id: &str,
        digest: &str,
        request_key: &str,
        confirmed: bool,
    ) -> Result<RestoreTask> {
        let _gate = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请确认新目录与恢复范围。",
            ));
        }
        if uuid::Uuid::parse_str(request_key).is_err() {
            return Err(HubError::new("REQUEST_KEY_INVALID", "请求标识无效。"));
        }
        let request_digest = sha(&serde_json::to_vec(&(plan_id, digest)).map_err(|_| stale())?);
        let existing: Option<(String, String)> =
            sqlx::query_as("SELECT id,request_digest FROM restore_task WHERE request_key=?")
                .bind(request_key)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?;
        if let Some((task, previous)) = existing {
            if previous != request_digest {
                return Err(HubError::new(
                    "REQUEST_CONFLICT",
                    "请求标识已用于不同的恢复操作。",
                ));
            }
            return self.restore_task(&task).await;
        }
        self.check_sessions("").await?;
        let prepared = self
            .restore_plans
            .lock()
            .await
            .remove(plan_id)
            .ok_or_else(stale)?;
        if prepared.plan.plan_digest != digest || prepared.plan.expires_at <= now() {
            return Err(stale());
        }
        same_parent(&prepared.parent, Path::new(&prepared.plan.parent_dir))?;
        let task_id = id();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO restore_task (id,request_key,request_digest,workspace_id,backup_id,destination,state,phase,created_at) VALUES (?,?,?,?,?,?,'running','preflight',?)")
            .bind(&task_id).bind(request_key).bind(request_digest).bind(&prepared.workspace_id).bind(&prepared.plan.backup_id).bind(&prepared.plan.destination).bind(now()).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&task_id)
            .bind("backup.restore_new")
            .bind("running")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        let flag = Arc::new(AtomicBool::new(false));
        self.scan_controls
            .lock()
            .await
            .insert(task_id.clone(), Arc::clone(&flag));
        let hub = Arc::clone(self);
        let worker_id = task_id.clone();
        tokio::spawn(async move {
            let phase = Arc::new(AtomicU8::new(0));
            let job_flag = Arc::clone(&flag);
            let job_phase = Arc::clone(&phase);
            let mut worker = tokio::task::spawn_blocking(move || {
                same_parent(&prepared.parent, Path::new(&prepared.plan.parent_dir))?;
                stage_restore(
                    &prepared.source,
                    RestoreDestination {
                        parent: &prepared.parent,
                        parent_path: Path::new(&prepared.plan.parent_dir),
                        name: &prepared.name,
                    },
                    Some(&prepared.plan.archive_sha256),
                    &job_flag,
                    &job_phase,
                )
            });
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut progress_failed = false;
            let result = loop {
                tokio::select! {
                    result = &mut worker => break result.unwrap_or_else(|_| Err(HubError::new("RESTORE_WORKER_FAILED", "恢复线程意外中断，请检查未完成目录。"))),
                    _ = ticker.tick() => {
                        if sqlx::query("UPDATE restore_task SET phase=?,revision=revision+1 WHERE id=? AND state IN ('running','cancelling')").bind(phase_name(phase.load(Ordering::Acquire))).bind(&worker_id).execute(&hub.pool).await.is_err() {
                            progress_failed = true;
                            flag.store(true, Ordering::Release);
                        }
                    }
                }
            };
            let result = if progress_failed {
                Err(HubError::new(
                    "TASK_PROGRESS_PERSIST_FAILED",
                    "恢复状态记录失败，请保留并检查未完成目录。",
                ))
            } else {
                result
            };
            if hub
                .finish_restore(
                    &worker_id,
                    result,
                    Arc::clone(&flag),
                    phase.load(Ordering::Acquire) == 1,
                )
                .await
                .is_err()
            {
                hub.mark_task_recovery(crate::task_recovery::TaskKind::Restore, &worker_id)
                    .await;
            }
            hub.scan_controls.lock().await.remove(&worker_id);
        });
        self.restore_task(&task_id).await
    }
    async fn finish_restore(
        &self,
        task: &str,
        result: Result<PendingRestore>,
        flag: Arc<AtomicBool>,
        verifying_source: bool,
    ) -> Result<()> {
        // Serialize cancellation with completion marker publication and terminal persistence.
        let _gate = self.mutation.lock().await;
        let result = match result {
            Ok(pending) => tokio::task::spawn_blocking(move || pending.complete(&flag))
                .await
                .unwrap_or_else(|_| {
                    Err(HubError::new(
                        "RESTORE_WORKER_FAILED",
                        "恢复完成状态不确定，请检查目标目录。",
                    ))
                }),
            Err(error) => Err(error),
        };
        let (state, error) = match result {
            Ok(_) => ("succeeded", None),
            Err(error) if error.code == "TASK_CANCELLED" => ("cancelled", Some(error.code)),
            Err(error) => ("failed", Some(error.code)),
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        if verifying_source && let Some(status) = error.and_then(source_failure_status) {
            sqlx::query("UPDATE backup_archive SET verification=? WHERE id=(SELECT backup_id FROM restore_task WHERE id=?)").bind(status).bind(task).execute(&mut *tx).await.map_err(HubError::db)?;
        }

        sqlx::query("UPDATE restore_task SET state=?,phase='finished',error_code=?,finished_at=?,revision=revision+1 WHERE id=?").bind(state).bind(error).bind(now()).bind(task).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task)
            .bind("backup.restore_new")
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    async fn record_restore_source_failure(&self, backup_id: &str, error: &HubError) -> Result<()> {
        let Some(status) = source_failure_status(error.code) else {
            return Ok(());
        };
        sqlx::query("UPDATE backup_archive SET verification=? WHERE id=?")
            .bind(status)
            .bind(backup_id)
            .execute(&self.pool)
            .await
            .map_err(HubError::db)?;
        Ok(())
    }
    pub async fn restore_task(&self, task_id: &str) -> Result<RestoreTask> {
        let mut task = sqlx::query_as(&format!("SELECT {COLUMNS} FROM restore_task WHERE id=?"))
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("TASK_NOT_FOUND", "恢复任务不存在。"))?;
        self.overlay_restore(&mut task).await;
        Ok(task)
    }
    pub async fn cancel_restore(&self, task_id: &str) -> Result<RestoreTask> {
        let _gate = self.mutation.lock().await;
        let task = self.restore_task(task_id).await?;
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return Ok(task);
        }
        let controls = self.scan_controls.lock().await;
        let flag = controls.get(task_id).ok_or(HubError::new(
            "TASK_STATE_UNKNOWN",
            "恢复任务状态未知，请保留目标目录并重启检查。",
        ))?;
        sqlx::query("UPDATE restore_task SET state='cancelling',revision=revision+1 WHERE id=? AND state='running'").bind(task_id).execute(&self.pool).await.map_err(HubError::db)?;
        flag.store(true, Ordering::Release);
        self.restore_task(task_id).await
    }
}
