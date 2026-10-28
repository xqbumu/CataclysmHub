use crate::{
    Hub, HubError, Installation, Result,
    database::{id, now},
    installation_copy::{self as copy, CopyControl},
    paths,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct ManagedInstallationCopyPlan {
    pub id: String,
    pub plan_digest: String,
    pub source_installation_id: String,
    pub source_root: String,
    pub parent_dir: String,
    pub destination: String,
    pub executable_relative: String,
    pub executable_sha256: String,
    pub copy_id: String,
    pub payload_root: String,
    pub manifest_sha256: String,
    pub entry_count: u32,
    pub file_count: u32,
    pub link_count: u32,
    pub total_bytes: String,
    pub expires_at: String,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct InstallationCopyTask {
    pub id: String,
    pub request_key: String,
    pub source_installation_id: String,
    pub source_root: String,
    pub parent_dir: String,
    pub destination: String,
    pub executable_relative: String,
    pub executable_sha256: String,
    pub copy_id: String,
    pub payload_root: String,
    pub manifest_sha256: String,
    #[ts(type = "number")]
    pub entry_count: i64,
    #[ts(type = "number")]
    pub file_count: i64,
    #[ts(type = "number")]
    pub link_count: i64,
    pub total_bytes: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub processed_entries: i64,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub verified_at: Option<String>,
    pub registered_installation_id: Option<String>,
}
pub(crate) struct PreparedCopy {
    plan: ManagedInstallationCopyPlan,
    prepared: Arc<copy::Prepared>,
}
fn stale() -> HubError {
    HubError::new("PLAN_STALE", "复制计划已失效，请重新预览。")
}
fn phase(value: u8) -> &'static str {
    match value {
        1 => "copying",
        2 => "checking_source",
        3 => "verifying_copy",
        4 => "publishing",
        5 => "checking_artifact",
        _ => "preflight",
    }
}
fn bundle_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        .map(Path::to_path_buf)
}
fn inside(root: &Path, path: &Path) -> Result<bool> {
    let identity = same_file::Handle::from_path(root).map_err(|_| stale())?;
    for ancestor in path.ancestors() {
        if identity == same_file::Handle::from_path(ancestor).map_err(|_| stale())? {
            return Ok(true);
        }
    }
    Ok(false)
}
impl Hub {
    async fn validate_copy_scope(
        &self,
        source_id: &str,
        source: &Path,
        parent: &Path,
    ) -> Result<()> {
        let data = self.dashboard().await?;
        let workspace = data
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let mut protected = vec![
            self.data_dir.clone(),
            PathBuf::from(workspace.save_dir),
            PathBuf::from(workspace.backup_dir),
            PathBuf::from(workspace.content_dir),
        ];
        protected.extend(
            data.play_instances
                .iter()
                .map(|i| PathBuf::from(&i.data_root)),
        );
        for root in &protected {
            if crate::play_instances::overlaps(source, root)? || inside(root, parent)? {
                return Err(HubError::new(
                    "COPY_SCOPE_OVERLAP",
                    "复制范围须与工作区、实例及元数据目录分离。",
                ));
            }
        }
        let own = data
            .installations
            .iter()
            .find(|i| i.id == source_id)
            .ok_or_else(stale)?;
        let own_working = own.working_dir.clone();
        for installation in data.installations {
            let working = Path::new(&installation.working_dir);
            let protected = bundle_root(working).unwrap_or_else(|| working.to_owned());
            if inside(&protected, parent)? {
                return Err(HubError::new(
                    "COPY_SCOPE_OVERLAP",
                    "目标父目录不能位于现有游戏安装内。",
                ));
            }
            if installation.id != source_id
                && !same_file::is_same_file(working, Path::new(&own_working)).unwrap_or(false)
                && crate::play_instances::overlaps(source, working)?
            {
                return Err(HubError::new(
                    "COPY_SCOPE_OVERLAP",
                    "源范围包含其他安装，请选择明确的完整安装目录。",
                ));
            }
        }
        Ok(())
    }
    pub async fn prepare_managed_installation_copy(
        &self,
        source_id: &str,
        selected_root: &str,
        parent_dir: &str,
    ) -> Result<ManagedInstallationCopyPlan> {
        let _gate = self.mutation.lock().await;
        self.check_resource_claims().await?;
        let installation = self.installation(source_id).await?;
        let selected = paths::canonical(selected_root, true)?;
        let parent = paths::canonical(parent_dir, true)?;
        let source = if let Some(bundle) = bundle_root(Path::new(&installation.working_dir)) {
            if !bundle.starts_with(&selected) {
                return Err(HubError::new(
                    "COPY_SOURCE_ROOT_INCOMPLETE",
                    "请选择完整 .app 或包含它的目录，不只选择包内资源目录。",
                ));
            }
            bundle
        } else {
            selected
        };
        let relative = Path::new(&installation.executable)
            .strip_prefix(&source)
            .map_err(|_| {
                HubError::new("COPY_SOURCE_ROOT_INVALID", "所选源目录不包含已登记程序。")
            })?;
        let executable = paths::text(relative)?.replace('\\', "/");
        self.validate_copy_scope(source_id, &source, &parent)
            .await?;
        let plan_id = id();
        let destination = parent.join(format!("installation-copy-{plan_id}"));
        let (prepared, preview, identity) = tokio::task::spawn_blocking(move || {
            let prepared = copy::prepare(&source, &executable, &destination)?;
            let preview = prepared.preview()?;
            let identity = prepared.identity()?;
            Ok::<_, HubError>((prepared, preview, identity))
        })
        .await
        .map_err(|_| HubError::new("COPY_PLAN_FAILED", "复制预览未完成。"))??;
        if preview.executable_sha256 != installation.sha256 {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "已登记程序发生变化，不能生成复制计划。",
            ));
        }
        let mut plan = ManagedInstallationCopyPlan {
            id: plan_id,
            plan_digest: String::new(),
            source_installation_id: installation.id,
            source_root: preview.source_root,
            parent_dir: paths::text(&parent)?,
            destination: preview.destination,
            executable_relative: preview.executable_relative,
            executable_sha256: preview.executable_sha256,
            copy_id: identity.0,
            payload_root: identity.1,
            manifest_sha256: identity.2,
            entry_count: preview.entries.len() as u32,
            file_count: preview.entries.iter().filter(|e| e.kind == "file").count() as u32,
            link_count: preview
                .entries
                .iter()
                .filter(|e| e.kind == "symlink")
                .count() as u32,
            total_bytes: preview.total_bytes,
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339(),
            warnings: preview.warnings,
        };
        plan.warnings
            .push("复制与核验完成后仍须明确登记为新安装；不会自动绑定或启动游戏。".into());
        plan.plan_digest =
            crate::backup_format::sha(&serde_json::to_vec(&plan).map_err(|_| stale())?);
        let mut plans = self.copy_plans.lock().await;
        plans.retain(|_, p| p.plan.expires_at > now());
        if plans.len() >= 8 {
            return Err(HubError::new("PLAN_LIMIT", "复制计划过多，请稍后重试。"));
        }
        plans.insert(
            plan.id.clone(),
            PreparedCopy {
                plan: plan.clone(),
                prepared: Arc::new(prepared),
            },
        );
        Ok(plan)
    }
    pub async fn installation_copy_task(&self, task_id: &str) -> Result<InstallationCopyTask> {
        let mut task: InstallationCopyTask =
            sqlx::query_as("SELECT * FROM installation_copy_task WHERE id=?")
                .bind(task_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
                .ok_or(HubError::new("TASK_NOT_FOUND", "复制任务不存在。"))?;
        self.overlay_copy(&mut task).await;
        Ok(task)
    }
    pub(crate) async fn overlay_copy(&self, task: &mut InstallationCopyTask) {
        if let Some(failure) = self.runtime_recovery.lock().await.get(&task.id)
            && matches!(
                failure.kind,
                crate::task_recovery::TaskKind::InstallationCopy
            )
        {
            task.state = "recovery_required".into();
            task.phase = "recording_failed".into();
            task.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            task.finished_at = Some(failure.finished_at.clone());
            task.revision += 1;
        }
    }
    pub async fn start_installation_copy(
        self: &Arc<Self>,
        plan_id: &str,
        digest: &str,
        key: &str,
        confirmed: bool,
    ) -> Result<InstallationCopyTask> {
        let _gate = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请检查完整复制范围并明确确认。",
            ));
        }
        if uuid::Uuid::parse_str(key).is_err() {
            return Err(HubError::new("REQUEST_KEY_INVALID", "请求标识无效。"));
        }
        let request_digest = crate::backup_format::sha(
            &serde_json::to_vec(&(plan_id, digest)).map_err(|_| stale())?,
        );
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id,request_digest FROM installation_copy_task WHERE request_key=?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(HubError::db)?;
        if let Some((id, old)) = existing {
            if old != request_digest {
                return Err(HubError::new(
                    "REQUEST_CONFLICT",
                    "请求标识已用于其他复制操作。",
                ));
            }
            return self.installation_copy_task(&id).await;
        }
        self.check_resource_claims().await?;
        let prepared = self
            .copy_plans
            .lock()
            .await
            .remove(plan_id)
            .ok_or_else(stale)?;
        let plan = &prepared.plan;
        if plan.plan_digest != digest || plan.expires_at <= now() {
            return Err(stale());
        }
        self.validate_copy_scope(
            &plan.source_installation_id,
            Path::new(&plan.source_root),
            Path::new(&plan.parent_dir),
        )
        .await?;
        let installation = self.installation(&plan.source_installation_id).await?;
        if installation.sha256 != plan.executable_sha256
            || paths::digest(PathBuf::from(installation.executable)).await?
                != plan.executable_sha256
        {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "预览后程序发生变化，复制已拒绝。",
            ));
        }
        let task_id = id();
        let created = now();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO installation_copy_task (id,request_key,request_digest,source_installation_id,source_root,parent_dir,destination,executable_relative,executable_sha256,copy_id,payload_root,manifest_sha256,entry_count,file_count,link_count,total_bytes,state,phase,created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'running','preflight',?)")
            .bind(&task_id).bind(key).bind(request_digest).bind(&plan.source_installation_id).bind(&plan.source_root).bind(&plan.parent_dir).bind(&plan.destination).bind(&plan.executable_relative).bind(&plan.executable_sha256).bind(&plan.copy_id).bind(&plan.payload_root).bind(&plan.manifest_sha256).bind(i64::from(plan.entry_count)).bind(i64::from(plan.file_count)).bind(i64::from(plan.link_count)).bind(&plan.total_bytes).bind(&created).execute(&mut *tx).await.map_err(HubError::db)?;
        audit(&mut tx, &task_id, "installation.copy", "running", None).await?;
        tx.commit().await.map_err(HubError::db)?;
        let control = Arc::new(CopyControl::new());
        self.copy_controls
            .lock()
            .await
            .insert(task_id.clone(), Arc::clone(&control));
        let hub = Arc::clone(self);
        let worker_id = task_id.clone();
        let plan = prepared.plan.clone();
        tokio::spawn(async move {
            let worker_hub = Arc::clone(&hub);
            let job_control = Arc::clone(&control);
            let job_plan = plan.clone();
            let mut worker = tokio::task::spawn_blocking(move || {
                copy::execute_prepared(&prepared.prepared, &job_control, |_| {
                    let gate = worker_hub.mutation.blocking_lock();
                    crate::backup_format::check_cancel(&job_control.cancel)?;
                    Ok(gate)
                })?;
                job_control.phase.store(5, Ordering::Release);
                let verified = copy::verify_installation_copy(Path::new(&job_plan.destination))?;
                check_result(
                    &job_plan.copy_id,
                    &job_plan.payload_root,
                    &job_plan.manifest_sha256,
                    &job_plan.executable_sha256,
                    &verified,
                )?;
                Ok::<_, HubError>(verified)
            });
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(200));
            let mut persistence_failed = false;
            let result = loop {
                tokio::select! {
                    done=&mut worker=>break done.unwrap_or_else(|_|Err(HubError::new("COPY_WORKER_FAILED","复制工作线程中断，请保留目标。"))),
                    _=ticker.tick()=>{if sqlx::query("UPDATE installation_copy_task SET phase=?,processed_entries=?,revision=revision+1 WHERE id=? AND state IN ('running','cancelling')").bind(phase(control.phase.load(Ordering::Acquire))).bind(control.processed.load(Ordering::Acquire) as i64).bind(&worker_id).execute(&hub.pool).await.is_err(){persistence_failed=true;control.cancel.store(true,Ordering::Release);}}
                }
            };
            let _gate = hub.mutation.lock().await;
            let terminal = if persistence_failed {
                Err(HubError::new(
                    "TASK_RESULT_PERSIST_FAILED",
                    "复制已结束，但任务结果未能记录。",
                ))
            } else {
                hub.finish_copy(
                    &worker_id,
                    result,
                    control.phase.load(Ordering::Acquire),
                    control.processed.load(Ordering::Acquire),
                )
                .await
            };
            if terminal.is_err() {
                hub.mark_task_recovery(
                    crate::task_recovery::TaskKind::InstallationCopy,
                    &worker_id,
                )
                .await;
            }
            hub.copy_controls.lock().await.remove(&worker_id);
        });
        self.installation_copy_task(&task_id).await
    }
    async fn finish_copy(
        &self,
        task_id: &str,
        result: Result<crate::InstallationCopyVerification>,
        phase: u8,
        processed: u64,
    ) -> Result<()> {
        let (state, error, verified_at) = match result {
            Ok(value) => ("succeeded", None, Some(value.verified_at)),
            Err(error) => {
                let state = if error.code == "TASK_CANCELLED" {
                    "cancelled"
                } else if phase >= 4 || error.code == "COPY_WORKER_FAILED" {
                    "recovery_required"
                } else {
                    "failed"
                };
                (state, Some(error.code), None)
            }
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let updated = sqlx::query("UPDATE installation_copy_task SET state=?,phase='finished',error_code=?,finished_at=?,verified_at=?,processed_entries=?,revision=revision+1 WHERE id=?").bind(state).bind(error).bind(now()).bind(verified_at).bind(processed as i64).bind(task_id).execute(&mut *tx).await.map_err(HubError::db)?.rows_affected();
        if updated != 1 {
            return Err(HubError::new(
                "TASK_RESULT_PERSIST_FAILED",
                "复制任务结果记录已变化。",
            ));
        }
        audit(&mut tx, task_id, "installation.copy", state, error).await?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    pub async fn cancel_installation_copy(&self, task_id: &str) -> Result<InstallationCopyTask> {
        let _gate = self.mutation.lock().await;
        let task = self.installation_copy_task(task_id).await?;
        if !matches!(task.state.as_str(), "running" | "cancelling") {
            return Ok(task);
        }
        let control = self
            .copy_controls
            .lock()
            .await
            .get(task_id)
            .cloned()
            .ok_or(HubError::new(
                "TASK_STATE_UNKNOWN",
                "复制工作者状态未知，请重启后检查。",
            ))?;
        if control.published.load(Ordering::Acquire) {
            return Err(HubError::new(
                "COPY_COMMITTING",
                "副本已进入完成核验阶段，不能再取消。",
            ));
        }
        if task.state == "running" {
            let mut tx = self.pool.begin().await.map_err(HubError::db)?;
            sqlx::query("UPDATE installation_copy_task SET state='cancelling',revision=revision+1 WHERE id=? AND state='running'").bind(task_id).execute(&mut *tx).await.map_err(HubError::db)?;
            audit(
                &mut tx,
                task_id,
                "installation.copy_cancel",
                "cancelling",
                None,
            )
            .await?;
            tx.commit().await.map_err(HubError::db)?;
        }
        control.cancel.store(true, Ordering::Release);
        self.installation_copy_task(task_id).await
    }
    pub async fn register_copied_installation(
        &self,
        task_id: &str,
        name: &str,
    ) -> Result<Installation> {
        let _gate = self.mutation.lock().await;
        let task = self.installation_copy_task(task_id).await?;
        if let Some(id) = &task.registered_installation_id {
            let record = self.installation(id).await?;
            if record.name != name.trim() {
                return Err(HubError::new(
                    "COPY_ALREADY_REGISTERED",
                    "副本已登记；重试不能隐式改名。",
                ));
            }
            return Ok(record);
        }
        if !matches!(task.state.as_str(), "succeeded" | "recovery_required") {
            return Err(HubError::new(
                "COPY_NOT_COMPLETE",
                "任务未完成或已取消，不能登记。",
            ));
        }
        if self.runtime_recovery.lock().await.contains_key(task_id) {
            return Err(HubError::new(
                "TASK_RESULT_PERSIST_FAILED",
                "请先重试记录已结束任务的状态，再独立校验登记。",
            ));
        }
        self.check_resource_claims().await?;
        let task_for_verify = task.clone();
        let (verified, executable, identities) = tokio::task::spawn_blocking(move || {
            let container = paths::canonical(&task_for_verify.destination, true)?;
            if paths::text(&container)? != task_for_verify.destination {
                return Err(stale());
            }
            let identity = same_file::Handle::from_path(&container).map_err(|_| stale())?;
            crate::backup_format::validate_path(&task_for_verify.payload_root)?;
            let payload_path = container.join(&task_for_verify.payload_root);
            let payload_identity =
                same_file::Handle::from_path(&payload_path).map_err(|_| stale())?;
            let verified = copy::verify_installation_copy(&container)?;
            check_result(
                &task_for_verify.copy_id,
                &task_for_verify.payload_root,
                &task_for_verify.manifest_sha256,
                &task_for_verify.executable_sha256,
                &verified,
            )?;
            if same_file::Handle::from_path(&container).map_err(|_| stale())? != identity {
                return Err(stale());
            }
            if same_file::Handle::from_path(&payload_path).map_err(|_| stale())? != payload_identity
            {
                return Err(stale());
            }
            let executable = container
                .join(&verified.payload_root)
                .join(&verified.executable_relative);
            Ok::<_, HubError>((verified, executable, (identity, payload_identity)))
        })
        .await
        .map_err(|_| HubError::new("COPY_VERIFY_FAILED", "副本独立校验未完成。"))??;
        if paths::digest(executable.clone()).await? != task.executable_sha256 {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "副本程序已变化，登记已阻止。",
            ));
        }
        let installation = self
            .prepare_installation(&paths::text(&executable)?, name)
            .await?;
        if installation.sha256 != task.executable_sha256 {
            return Err(stale());
        }
        if installation.name != name.trim() {
            return Err(HubError::new(
                "COPY_ALREADY_REGISTERED",
                "程序路径已登记为其他名称，请使用现有安装名称。",
            ));
        }
        if same_file::Handle::from_path(&task.destination).map_err(|_| stale())? != identities.0
            || same_file::Handle::from_path(Path::new(&task.destination).join(&task.payload_root))
                .map_err(|_| stale())?
                != identities.1
        {
            return Err(stale());
        }
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        Self::write_installation(&mut tx, &installation).await?;
        let changed = sqlx::query("UPDATE installation_copy_task SET registered_installation_id=?,verified_at=?,revision=revision+1 WHERE id=? AND registered_installation_id IS NULL").bind(&installation.id).bind(&verified.verified_at).bind(task_id).execute(&mut *tx).await.map_err(HubError::db)?.rows_affected();
        if changed != 1 {
            return Err(HubError::new(
                "COPY_RECORD_CHANGED",
                "复制任务登记关联已变化，未提交新安装。",
            ));
        }
        audit(
            &mut tx,
            task_id,
            "installation.copy_register",
            "registered",
            None,
        )
        .await?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(installation)
    }
}
fn check_result(
    id: &str,
    payload: &str,
    manifest: &str,
    executable: &str,
    result: &crate::InstallationCopyVerification,
) -> Result<()> {
    if result.copy_id != id
        || result.payload_root != payload
        || result.manifest_sha256 != manifest
        || result.executable_sha256 != executable
        || result.binding_verified
        || result.isolation_verified
    {
        return Err(HubError::new(
            "COPY_CONTEXT_MISMATCH",
            "副本内容与已确认任务不一致，不能登记。",
        ));
    }
    Ok(())
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task: &str,
    action: &str,
    state: &str,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
        .bind(id())
        .bind(task)
        .bind(action)
        .bind(state)
        .bind(error)
        .bind(now())
        .execute(&mut **tx)
        .await
        .map_err(HubError::db)?;
    Ok(())
}
