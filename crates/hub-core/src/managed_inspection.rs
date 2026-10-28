use crate::{
    BackupRecord, Hub, HubError, InspectionTask, RestoreInspection, RestoreTask, Result,
    backup_format::{io_error, sha, source_failure_status},
    database::{id, now},
    paths,
    restore_inspection::{ArchiveIdentity, inspect_dirs},
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

#[derive(sqlx::FromRow)]
pub(crate) struct InspectionRow {
    #[sqlx(flatten)]
    task: InspectionTask,
    result_json: Option<String>,
}
impl InspectionRow {
    pub(crate) fn into_task(mut self) -> Result<InspectionTask> {
        self.task.result = self
            .result_json
            .map(|json| serde_json::from_str(&json))
            .transpose()
            .map_err(|_| {
                HubError::new(
                    "INSPECTION_RESULT_INVALID",
                    "检查结果记录无法读取，请保留元数据并检查。",
                )
            })?;
        if self.task.state == "succeeded" && self.task.result.is_none() {
            return Err(HubError::new(
                "INSPECTION_RESULT_INVALID",
                "检查完成记录缺少结果，请保留元数据并检查。",
            ));
        }
        Ok(self.task)
    }
}
fn phase_name(phase: u8) -> &'static str {
    match phase {
        1 => "verifying_archive",
        2 => "checking_restored",
        _ => "preflight",
    }
}
fn inspect_registered(
    record: &BackupRecord,
    restore: &RestoreTask,
    backup_root: &str,
    cancel: &AtomicBool,
    phase: &AtomicU8,
) -> Result<RestoreInspection> {
    phase.store(1, Ordering::Release);
    let base_path = paths::canonical(backup_root, true)?;
    if paths::text(&base_path)? != backup_root {
        return Err(HubError::new(
            "PATH_CHANGED",
            "备份路径已变化，请检查工作区。",
        ));
    }
    let base = Dir::open_ambient_dir(base_path, cap_std::ambient_authority()).map_err(io_error)?;
    let archive = base
        .open_dir_nofollow(&record.relative_dir)
        .map_err(io_error)?;
    // Failures while opening the destination must not invalidate the source archive's health.
    phase.store(2, Ordering::Release);
    let target = Path::new(&restore.destination);
    let parent = target
        .parent()
        .ok_or(HubError::new("PATH_CHANGED", "恢复目标路径无效。"))?;
    let canonical = std::fs::canonicalize(parent).map_err(io_error)?;
    if canonical != parent {
        return Err(HubError::new(
            "PATH_CHANGED",
            "恢复目标父路径已变化，请检查目录。",
        ));
    }
    let parent =
        Dir::open_ambient_dir(canonical, cap_std::ambient_authority()).map_err(io_error)?;
    let name = target
        .file_name()
        .ok_or(HubError::new("PATH_CHANGED", "恢复目录名称无效。"))?;
    let root = parent.open_dir_nofollow(name).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            io_error(error)
        } else {
            HubError::new(
                "RESTORED_DATA_MISMATCH",
                "恢复目录不存在、不可读或已成为链接。",
            )
        }
    })?;
    inspect_dirs(
        &archive,
        &root,
        Some(ArchiveIdentity {
            backup_id: &record.id,
            archive_sha256: &record.archive_sha256,
            manifest_sha256: &record.manifest_sha256,
        }),
        cancel,
        phase,
    )
}
impl Hub {
    pub async fn start_inspection(
        self: &Arc<Self>,
        restore_task_id: &str,
        request_key: &str,
    ) -> Result<InspectionTask> {
        let _gate = self.mutation.lock().await;
        if uuid::Uuid::parse_str(request_key).is_err() {
            return Err(HubError::new("REQUEST_KEY_INVALID", "请求标识无效。"));
        }
        let digest = sha(restore_task_id.as_bytes());
        let existing: Option<(String, String)> =
            sqlx::query_as("SELECT id,request_digest FROM inspection_task WHERE request_key=?")
                .bind(request_key)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?;
        if let Some((task, previous)) = existing {
            if previous != digest {
                return Err(HubError::new(
                    "REQUEST_CONFLICT",
                    "请求标识已用于其他目录检查。",
                ));
            }
            return self.inspection_task(&task).await;
        }
        self.check_resource_claims().await?;
        let restore = self.restore_task(restore_task_id).await?;
        if ["running", "cancelling"].contains(&restore.state.as_str()) {
            return Err(HubError::new(
                "TASK_CONFLICT",
                "请先等待恢复结束或取消恢复。",
            ));
        }
        let record: BackupRecord =
            sqlx::query_as("SELECT * FROM backup_archive WHERE id=? AND workspace_id=?")
                .bind(&restore.backup_id)
                .bind(&restore.workspace_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
                .ok_or(HubError::new("BACKUP_NOT_FOUND", "原备份记录不存在。"))?;
        if record.relative_dir != record.id || uuid::Uuid::parse_str(&record.id).is_err() {
            return Err(HubError::new("PATH_CHANGED", "备份目录记录无效。"));
        }
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        if workspace.id != restore.workspace_id {
            return Err(HubError::new("PATH_CHANGED", "恢复记录与工作区不匹配。"));
        }
        let target = Path::new(&restore.destination);
        if target
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("restored-"))
            .is_none_or(|value| uuid::Uuid::parse_str(value).is_err())
        {
            return Err(HubError::new(
                "PATH_CHANGED",
                "恢复目标不是已登记的新目录。",
            ));
        }
        for protected in [
            &workspace.save_dir,
            &workspace.backup_dir,
            &workspace.content_dir,
            &paths::text(&self.data_dir)?,
        ] {
            if paths::overlap(target, Path::new(protected)) {
                return Err(HubError::new(
                    "PATH_OVERLAP",
                    "恢复检查不能指向工作区原始数据目录。",
                ));
            }
        }
        let task_id = id();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO inspection_task (id,request_key,request_digest,restore_task_id,state,phase,created_at) VALUES (?,?,?,?,'running','preflight',?)").bind(&task_id).bind(request_key).bind(digest).bind(restore_task_id).bind(now()).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&task_id)
            .bind("backup.inspect_restored")
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
            let backup_id = record.id.clone();
            let mut worker = tokio::task::spawn_blocking(move || {
                inspect_registered(
                    &record,
                    &restore,
                    &workspace.backup_dir,
                    &job_flag,
                    &job_phase,
                )
            });
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut progress_failed = false;
            let result = loop {
                tokio::select! {
                    result=&mut worker=>break result.unwrap_or_else(|_| Err(HubError::new("INSPECTION_WORKER_FAILED", "目录检查线程中断，请保留原数据。"))),
                    _=ticker.tick()=> {
                        if sqlx::query("UPDATE inspection_task SET phase=?,revision=revision+1 WHERE id=? AND state IN ('running','cancelling')").bind(phase_name(phase.load(Ordering::Acquire))).bind(&worker_id).execute(&hub.pool).await.is_err() {
                            progress_failed = true;
                            flag.store(true, Ordering::Release);
                        }
                    }
                }
            };
            let result = if progress_failed {
                Err(HubError::new(
                    "TASK_PROGRESS_PERSIST_FAILED",
                    "目录检查状态记录失败，未保存新的校验结论。",
                ))
            } else {
                result
            };
            if hub
                .finish_inspection(
                    &worker_id,
                    &backup_id,
                    result,
                    &flag,
                    phase.load(Ordering::Acquire) == 1,
                )
                .await
                .is_err()
            {
                hub.mark_task_recovery(crate::task_recovery::TaskKind::Inspection, &worker_id)
                    .await;
            }
            hub.scan_controls.lock().await.remove(&worker_id);
        });
        self.inspection_task(&task_id).await
    }
    async fn finish_inspection(
        &self,
        task: &str,
        backup_id: &str,
        result: Result<RestoreInspection>,
        cancel: &AtomicBool,
        verifying_source: bool,
    ) -> Result<()> {
        let _gate = self.mutation.lock().await;
        let progress_failed = result
            .as_ref()
            .err()
            .is_some_and(|error| error.code == "TASK_PROGRESS_PERSIST_FAILED");
        let result = if cancel.load(Ordering::Acquire) && !progress_failed {
            Err(HubError::new(
                "TASK_CANCELLED",
                "检查已取消，未保存新的校验结论。",
            ))
        } else {
            result
        };
        let (state, error, json) = match result {
            Ok(result) => (
                "succeeded",
                None,
                Some(serde_json::to_string(&result).map_err(|_| {
                    HubError::new("INSPECTION_RESULT_INVALID", "检查结果无法保存。")
                })?),
            ),
            Err(error) if error.code == "TASK_CANCELLED" => ("cancelled", Some(error.code), None),
            Err(error) => ("failed", Some(error.code), None),
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        if verifying_source && let Some(status) = error.and_then(source_failure_status) {
            sqlx::query("UPDATE backup_archive SET verification=? WHERE id=?")
                .bind(status)
                .bind(backup_id)
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        }
        sqlx::query("UPDATE inspection_task SET state=?,phase='finished',error_code=?,finished_at=?,result_json=?,revision=revision+1 WHERE id=?").bind(state).bind(error).bind(now()).bind(json).bind(task).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task)
            .bind("backup.inspect_restored")
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    pub async fn inspection_task(&self, task_id: &str) -> Result<InspectionTask> {
        let row = sqlx::query_as::<_, InspectionRow>("SELECT * FROM inspection_task WHERE id=?")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("TASK_NOT_FOUND", "目录检查任务不存在。"))?;
        let mut task = row.into_task()?;
        self.overlay_inspection(&mut task).await;
        Ok(task)
    }
    pub async fn cancel_inspection(&self, task_id: &str) -> Result<InspectionTask> {
        let _gate = self.mutation.lock().await;
        let task = self.inspection_task(task_id).await?;
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return Ok(task);
        }
        let controls = self.scan_controls.lock().await;
        let flag = controls.get(task_id).ok_or(HubError::new(
            "TASK_STATE_UNKNOWN",
            "检查任务状态未知，请重启后检查记录。",
        ))?;
        sqlx::query("UPDATE inspection_task SET state='cancelling',revision=revision+1 WHERE id=? AND state='running'").bind(task_id).execute(&self.pool).await.map_err(HubError::db)?;
        flag.store(true, Ordering::Release);
        self.inspection_task(task_id).await
    }
}
