//! Single-file publication. Uncertain outcomes retain claims until explicit read-only reconciliation.
use crate::{
    BackupRecord, ContentConfigInput, Hub, HubError, Result, backup_format as format,
    database::{id, now},
    paths,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions, Permissions};
use serde::Serialize;
use std::{
    io::{Read, Write},
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, TS)]
pub struct ConfigChangePlan {
    pub id: String,
    pub plan_digest: String,
    pub instance_id: String,
    pub backup_id: String,
    pub data_root: String,
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub proposed_json: String,
    pub expires_at: String,
}
#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct ConfigChange {
    pub id: String,
    pub instance_id: String,
    pub backup_id: String,
    pub data_root: String,
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub state: String,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ConfigChangeObservation {
    pub change: ConfigChange,
    pub current_sha256: String,
    pub matches: String,
}
const COLUMNS: &str = "id,instance_id,backup_id,data_root,path,before_sha256,after_sha256,state,error_code,created_at,finished_at";
pub(crate) struct PreparedConfigChange {
    plan: ConfigChangePlan,
    root: Dir,
    config: Dir,
    archive: Dir,
    base: Dir,
    backup_root: String,
    record: BackupRecord,
    file_identity: same_file::Handle,
    permissions: Permissions,
    working_dir: String,
    resources_sha256: String,
}
fn stale() -> HubError {
    HubError::new(
        "CONFIG_CHANGE_STALE",
        "配置、保护归档或目录已变化，请重新准备应用。",
    )
}
fn same(dir: &Dir, path: &Path) -> Result<()> {
    let held =
        same_file::Handle::from_file(dir.try_clone().map_err(format::io_error)?.into_std_file())
            .map_err(format::io_error)?;
    if std::fs::canonicalize(path).map_err(format::io_error)? != path
        || held != same_file::Handle::from_path(path).map_err(format::io_error)?
    {
        return Err(stale());
    }
    Ok(())
}
fn read(dir: &Dir, name: &str) -> Result<(Vec<u8>, same_file::Handle, Permissions)> {
    let mut file = format::open_file(dir, name)?;
    let metadata = file.metadata().map_err(format::io_error)?;
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(stale());
        }
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(format::io_error)?;
    if bytes.len() > 1024 * 1024 {
        return Err(stale());
    }
    Ok((
        bytes,
        same_file::Handle::from_file(file.into_std()).map_err(format::io_error)?,
        metadata.permissions(),
    ))
}
fn verify(p: &PreparedConfigChange) -> Result<()> {
    same(&p.root, Path::new(&p.plan.data_root))?;
    same(&p.config, &Path::new(&p.plan.data_root).join("config"))?;
    same(&p.base, Path::new(&p.backup_root))?;
    same(
        &p.archive,
        &Path::new(&p.backup_root).join(&p.record.relative_dir),
    )?;
    let (manifest, verified) = format::verify_dir(
        &p.archive,
        Some(&p.record.archive_sha256),
        &AtomicBool::new(false),
    )?;
    if verified.backup_id != p.record.id
        || verified.manifest_sha256 != p.record.manifest_sha256
        || manifest.scope != "instance_data_root"
        || manifest.source_token != p.plan.instance_id
        || !manifest.entries.iter().any(|e| {
            e.relative_path == p.plan.path
                && e.file_type == "file"
                && e.sha256.as_deref() == Some(&p.plan.before_sha256)
        })
    {
        return Err(stale());
    }
    Ok(())
}
fn resources_digest(discovery: &crate::ContentDiscovery) -> Result<String> {
    Ok(format::sha(
        &serde_json::to_vec(&(&discovery.scopes, &discovery.packages, discovery.truncated))
            .map_err(|_| stale())?,
    ))
}
fn publish(p: PreparedConfigChange) -> Result<()> {
    verify(&p)?;
    let discovery =
        crate::content_discovery::discover(Path::new(&p.plan.data_root), &p.working_dir);
    if resources_digest(&discovery)? != p.resources_sha256 {
        return Err(stale());
    }
    let name = p.plan.path.strip_prefix("config/").ok_or_else(stale)?;
    let (bytes, identity, _) = read(&p.config, name)?;
    if identity != p.file_identity || format::sha(&bytes) != p.plan.before_sha256 {
        return Err(stale());
    }
    let temp = format!(".hub-config-{}.tmp", p.plan.id);
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    let mut output = p.config.open_with(&temp, &opts).map_err(format::io_error)?;
    output
        .write_all(p.plan.proposed_json.as_bytes())
        .map_err(format::io_error)?;
    output
        .set_permissions(p.permissions)
        .map_err(format::io_error)?;
    output.sync_all().map_err(format::io_error)?;
    let staged = same_file::Handle::from_file(output.into_std()).map_err(format::io_error)?;
    let (staged_bytes, staged_identity, _) = read(&p.config, &temp)?;
    if staged_identity != staged || format::sha(&staged_bytes) != p.plan.after_sha256 {
        return Err(stale());
    }
    same(&p.root, Path::new(&p.plan.data_root))?;
    same(&p.config, &Path::new(&p.plan.data_root).join("config"))?;
    let (latest, identity, _) = read(&p.config, name)?;
    if identity != p.file_identity || format::sha(&latest) != p.plan.before_sha256 {
        return Err(stale());
    }
    p.config
        .rename(&temp, &p.config, name)
        .map_err(format::io_error)?;
    format::sync_directory(&p.config)?;
    same(&p.config, &Path::new(&p.plan.data_root).join("config"))?;
    let (actual, identity, _) = read(&p.config, name)?;
    if identity != staged || format::sha(&actual) != p.plan.after_sha256 {
        return Err(stale());
    }
    Ok(())
}
impl Hub {
    pub async fn prepare_config_change(
        &self,
        instance_id: &str,
        input: ContentConfigInput,
        backup_id: &str,
    ) -> Result<ConfigChangePlan> {
        let _gate = self.mutation.lock().await;
        self.check_resource_claims().await?;
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        self.instance_identity(
            &instance,
            &installation.working_dir,
            &installation.executable,
        )
        .await?;
        let configuration = self
            .preview_content_config_locked(instance_id, input)
            .await?;
        if configuration.files.len() != 1 {
            return Err(HubError::new(
                "CONFIG_CHANGE_SINGLE_FILE",
                "一次只能应用一个有差异的配置文件；请分别调整默认模组与声音设置。",
            ));
        }
        // Unknown candidates are not promoted to loadable content by file publication.
        if configuration.proposed_discovery.truncated
            || !configuration.proposed_discovery.findings.is_empty()
            || configuration
                .proposed_discovery
                .scopes
                .iter()
                .any(|s| !matches!(s.status.as_str(), "scanned" | "missing"))
        {
            return Err(HubError::new(
                "CONFIG_CHANGE_DIAGNOSTICS",
                "候选扫描或依赖诊断尚有问题，请先解决再准备配置应用。",
            ));
        }
        let workspace = self.dashboard().await?.workspace.ok_or_else(stale)?;
        let record: BackupRecord =
            sqlx::query_as("SELECT * FROM backup_archive WHERE id=? AND workspace_id=?")
                .bind(backup_id)
                .bind(&workspace.id)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
                .ok_or_else(stale)?;
        if record.play_instance_id.as_deref() != Some(instance_id)
            || record.source_root.as_deref() != Some(&instance.data_root)
            || record.relative_dir != record.id
            || uuid::Uuid::parse_str(&record.id).is_err()
        {
            return Err(stale());
        }
        let resources_sha256 = resources_digest(&configuration.report.discovery)?;
        let file = configuration.files.into_iter().next().ok_or_else(stale)?;
        let mut plan = ConfigChangePlan {
            id: id(),
            plan_digest: String::new(),
            instance_id: instance.id,
            backup_id: record.id.clone(),
            data_root: instance.data_root,
            path: file.path,
            before_sha256: file.before_sha256,
            after_sha256: file.proposed_sha256,
            proposed_json: file.proposed_json,
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339(),
        };
        plan.plan_digest =
            format::sha(&serde_json::to_vec(&(&plan, &resources_sha256)).map_err(|_| stale())?);
        let root = Dir::open_ambient_dir(&plan.data_root, cap_std::ambient_authority())
            .map_err(format::io_error)?;
        let config = root.open_dir_nofollow("config").map_err(format::io_error)?;
        let (bytes, file_identity, permissions) = read(
            &config,
            plan.path.strip_prefix("config/").ok_or_else(stale)?,
        )?;
        if format::sha(&bytes) != plan.before_sha256 {
            return Err(stale());
        }
        let base = Dir::open_ambient_dir(&workspace.backup_dir, cap_std::ambient_authority())
            .map_err(format::io_error)?;
        let archive = base
            .open_dir_nofollow(&record.relative_dir)
            .map_err(format::io_error)?;
        let prepared = PreparedConfigChange {
            plan: plan.clone(),
            root,
            config,
            base,
            archive,
            backup_root: workspace.backup_dir,
            record,
            file_identity,
            permissions,
            working_dir: installation.working_dir,
            resources_sha256,
        };
        let prepared = tokio::task::spawn_blocking(move || {
            verify(&prepared)?;
            Ok(prepared)
        })
        .await
        .map_err(|_| stale())??;
        let mut plans = self.config_change_plans.lock().await;
        plans.retain(|_, p| p.plan.expires_at > now());
        if plans.len() >= 10 {
            return Err(HubError::new(
                "PLAN_LIMIT",
                "配置应用预览过多，请稍后重试。",
            ));
        }
        plans.insert(plan.id.clone(), prepared);
        Ok(plan)
    }
    pub async fn config_changes(&self) -> Result<Vec<ConfigChange>> {
        sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM config_change ORDER BY created_at DESC,id DESC LIMIT 200"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(HubError::db)
    }
    async fn config_change(&self, change_id: &str) -> Result<ConfigChange> {
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM config_change WHERE id=?"))
            .bind(change_id)
            .fetch_one(&self.pool)
            .await
            .map_err(HubError::db)
    }
    pub async fn apply_config_change(
        self: &Arc<Self>,
        plan_id: &str,
        digest: &str,
        key: &str,
        confirmed: bool,
    ) -> Result<ConfigChange> {
        let _gate = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请明确确认单文件配置应用。",
            ));
        }
        if uuid::Uuid::parse_str(key).is_err() {
            return Err(stale());
        }
        let request_digest = format::sha(format!("{plan_id}:{digest}").as_bytes());
        let existing: Option<(String, String)> =
            sqlx::query_as("SELECT id,request_digest FROM config_change WHERE request_key=?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?;
        if let Some((id, existing)) = existing {
            if existing != request_digest {
                return Err(stale());
            }
            return self.config_change(&id).await;
        }
        self.check_resource_claims().await?;
        let mut plans = self.config_change_plans.lock().await;
        let p = plans.get(plan_id).ok_or_else(stale)?;
        if p.plan.plan_digest != digest || p.plan.expires_at <= now() {
            return Err(stale());
        }
        let workspace = self.dashboard().await?.workspace.ok_or_else(stale)?;
        if workspace.id != p.record.workspace_id || workspace.backup_dir != p.backup_root {
            return Err(stale());
        }
        let instance = self.instance(&p.plan.instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        self.instance_identity(
            &instance,
            &installation.working_dir,
            &installation.executable,
        )
        .await?;
        if instance.data_root != p.plan.data_root {
            return Err(stale());
        }
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let inserted = sqlx::query(
            "INSERT INTO config_change VALUES (?,?,?,?,?,?,?,?,?,'applying',NULL,?,NULL)",
        )
        .bind(&p.plan.id)
        .bind(key)
        .bind(request_digest)
        .bind(&p.plan.instance_id)
        .bind(&p.plan.backup_id)
        .bind(&p.plan.data_root)
        .bind(&p.plan.path)
        .bind(&p.plan.before_sha256)
        .bind(&p.plan.after_sha256)
        .bind(now())
        .execute(&mut *tx)
        .await
        .map_err(HubError::db)?;
        if inserted.rows_affected() != 1 {
            return Err(stale());
        }
        let inserted =
            sqlx::query("INSERT INTO operation_log VALUES (?,?, 'config.apply','applying',NULL,?)")
                .bind(id())
                .bind(&p.plan.id)
                .bind(now())
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        if inserted.rows_affected() != 1 {
            return Err(stale());
        }
        tx.commit().await.map_err(HubError::db)?;
        let prepared = plans.remove(plan_id).ok_or_else(stale)?;
        drop(plans);
        let task_id = plan_id.to_owned();
        self.config_change_running
            .lock()
            .await
            .insert(task_id.clone());
        let hub = self.clone();
        tokio::spawn(async move {
            let _gate = hub.mutation.lock().await;
            let result = tokio::task::spawn_blocking(move || publish(prepared))
                .await
                .unwrap_or_else(|_| Err(stale()));
            let (state, error) = match result {
                Ok(()) => ("applied", None),
                Err(e) => ("recovery_required", Some(e.code)),
            };
            // A failed terminal transaction deliberately leaves the durable applying claim.
            let _ = hub.finish_config_change(&task_id, state, error).await;
            hub.config_change_running.lock().await.remove(&task_id);
        });
        self.config_change(plan_id).await
    }
    async fn finish_config_change(
        &self,
        change_id: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let written =
            sqlx::query("UPDATE config_change SET state=?,error_code=?,finished_at=? WHERE id=?")
                .bind(state)
                .bind(error)
                .bind(now())
                .bind(change_id)
                .execute(&mut *tx)
                .await
                .map_err(HubError::db)?;
        if written.rows_affected() != 1 {
            return Err(stale());
        }
        let written = sqlx::query("INSERT INTO operation_log VALUES (?,?, 'config.apply',?,?,?)")
            .bind(id())
            .bind(change_id)
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        if written.rows_affected() != 1 {
            return Err(stale());
        }
        tx.commit().await.map_err(HubError::db)
    }
    pub async fn inspect_config_change(&self, change_id: &str) -> Result<ConfigChangeObservation> {
        let _gate = self.mutation.lock().await;
        self.inspect_config_change_locked(change_id).await
    }
    async fn inspect_config_change_locked(
        &self,
        change_id: &str,
    ) -> Result<ConfigChangeObservation> {
        let change = self.config_change(change_id).await?;
        let instance = self.instance(&change.instance_id).await?;
        if instance.data_root != change.data_root
            || paths::canonical(&change.data_root, true)? != Path::new(&change.data_root)
        {
            return Err(stale());
        }
        let root = Dir::open_ambient_dir(&change.data_root, cap_std::ambient_authority())
            .map_err(format::io_error)?;
        let config = root.open_dir_nofollow("config").map_err(format::io_error)?;
        let (bytes, _, _) = read(
            &config,
            change.path.strip_prefix("config/").ok_or_else(stale)?,
        )?;
        same(&root, Path::new(&change.data_root))?;
        same(&config, &Path::new(&change.data_root).join("config"))?;
        let hash = format::sha(&bytes);
        let matches = if hash == change.before_sha256 {
            "original"
        } else if hash == change.after_sha256 {
            "proposed"
        } else {
            "other"
        };
        Ok(ConfigChangeObservation {
            change,
            current_sha256: hash,
            matches: matches.into(),
        })
    }
    pub async fn resolve_config_change(
        &self,
        change_id: &str,
        expected_hash: &str,
        confirmed: bool,
    ) -> Result<ConfigChangeObservation> {
        let _gate = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请核对当前配置后确认解除中断保护。",
            ));
        }
        if self.config_change_running.lock().await.contains(change_id) {
            return Err(HubError::new(
                "CONFIG_CHANGE_ACTIVE",
                "配置应用尚未结束，请稍后检查。",
            ));
        }
        let result = self.inspect_config_change_locked(change_id).await?;
        if !matches!(
            result.change.state.as_str(),
            "applying" | "recovery_required"
        ) || result.current_sha256 != expected_hash
            || result.matches == "other"
        {
            return Err(stale());
        }
        let state = if result.matches == "original" {
            "resolved_original"
        } else {
            "resolved_proposed"
        };
        self.finish_config_change(change_id, state, None).await?;
        self.inspect_config_change_locked(change_id).await
    }
}
