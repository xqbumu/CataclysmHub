//! Verified local content copies; no writes to game installations or world settings.
use crate::{
    Hub, HubError, Result, backup_format as format,
    database::{id, now},
    paths,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct ContentFile {
    pub path: String,
    pub kind: String,
    pub size: String,
    pub sha256: Option<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ContentInstallPlan {
    pub id: String,
    pub plan_digest: String,
    pub entry_id: String,
    pub source_root: String,
    pub parent_dir: String,
    pub destination: String,
    pub payload_root: String,
    pub content_kind: String,
    pub manifest_sha256: String,
    pub files: Vec<ContentFile>,
    pub total_bytes: String,
    pub expires_at: String,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct InstanceContentInstallPlan {
    pub instance_id: String,
    pub instance_name: String,
    pub installation_id: String,
    pub plan: ContentInstallPlan,
}
struct Deployment {
    instance: crate::PlayInstance,
    installation: crate::Installation,
    definitions: Vec<crate::ContentDefinition>,
    metadata_sha256: String,
    kind: String,
}
fn role(kind: &str) -> &'static str {
    if kind == "mod" { "mods" } else { "sound" }
}
fn check_deployment(d: &Deployment) -> Result<()> {
    let root = Path::new(&d.instance.data_root);
    let path = root.join(role(&d.kind));
    match std::fs::symlink_metadata(&path) {
        Ok(m) if !m.is_dir() || m.file_type().is_symlink() => return Err(changed()),
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(format::io_error(e)),
    }
    let discovery = crate::content_discovery::discover(root, &d.installation.working_dir);
    if discovery.truncated
        || discovery
            .scopes
            .iter()
            .any(|s| !matches!(s.status.as_str(), "scanned" | "missing"))
    {
        return Err(HubError::new(
            "CONTENT_DISCOVERY_INCOMPLETE",
            "目标资源扫描不完整，不能排除重复 ID。",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for definition in &d.definitions {
        if !ids.insert(&definition.content_id)
            || discovery
                .packages
                .iter()
                .filter(|p| p.kind == d.kind)
                .flat_map(|p| &p.definitions)
                .any(|item| item.content_id == definition.content_id)
        {
            return Err(HubError::new(
                "CONTENT_ID_CONFLICT",
                "目标实例或关联安装已有相同资源 ID；请先排查，不能叠加覆盖。",
            ));
        }
    }
    Ok(())
}
fn publish_deployment(p: &PreparedContentInstall, container: &Dir, d: &Deployment) -> Result<()> {
    check_deployment(d)?;
    same(&p.parent, Path::new(&p.plan.parent_dir))?;
    let name = role(&d.kind);
    match p.parent.create_dir(name) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(format::io_error(e)),
    }
    let target = p.parent.open_dir_nofollow(name).map_err(format::io_error)?;
    same(&target, &Path::new(&p.plan.parent_dir).join(name))?;
    let leaf = Path::new(&p.plan.payload_root)
        .file_name()
        .ok_or_else(changed)?;
    rustix::fs::renameat_with(
        container,
        "data",
        &target,
        leaf,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|e| format::io_error(e.into()))?;
    sync(&target)?;
    Ok(())
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn publish_deployment(
    _p: &PreparedContentInstall,
    _container: &Dir,
    _d: &Deployment,
) -> Result<()> {
    Err(HubError::new(
        "CONTENT_DEPLOY_PLATFORM_UNSUPPORTED",
        "此平台的资源发布尚未验证。",
    ))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct ContentInstallTask {
    pub id: String,
    pub entry_id: String,
    pub source_root: String,
    pub destination: String,
    pub payload_root: String,
    pub manifest_sha256: String,
    pub state: String,
    pub phase: String,
    #[ts(type = "number")]
    pub revision: i64,
    pub error_code: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}
const COLUMNS: &str = "id,entry_id,source_root,destination,payload_root,manifest_sha256,state,phase,revision,error_code,created_at,finished_at";
pub(crate) struct PreparedContentInstall {
    plan: ContentInstallPlan,
    source: Dir,
    parent: Dir,
    deployment: Option<Deployment>,
}
fn changed() -> HubError {
    HubError::new(
        "CONTENT_SOURCE_CHANGED",
        "资源包或目标路径发生变化，请刷新资源并重新预览。",
    )
}
fn scan(root: &Dir, cancel: &AtomicBool) -> Result<Vec<ContentFile>> {
    crate::installation_copy::inventory(root, true, cancel, false)?
        .into_iter()
        .map(|e| {
            if !matches!(e.kind.as_str(), "file" | "directory") {
                return Err(HubError::new(
                    "CONTENT_LINK_UNSUPPORTED",
                    "资源安装不支持链接或特殊文件。",
                ));
            }
            Ok(ContentFile {
                path: e.relative_path,
                kind: e.kind,
                size: e.size,
                sha256: e.sha256,
            })
        })
        .collect()
}
fn same(root: &Dir, path: &Path) -> Result<()> {
    let held =
        same_file::Handle::from_file(root.try_clone().map_err(format::io_error)?.into_std_file())
            .map_err(format::io_error)?;
    if std::fs::canonicalize(path).map_err(format::io_error)? != path
        || held != same_file::Handle::from_path(path).map_err(format::io_error)?
    {
        return Err(changed());
    }
    Ok(())
}
fn write_new(root: &Dir, name: &str, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = root.open_with(name, &options).map_err(format::io_error)?;
    file.write_all(bytes).map_err(format::io_error)?;
    file.sync_all().map_err(format::io_error)
}
fn sync(root: &Dir) -> Result<()> {
    #[cfg(unix)]
    root.try_clone()
        .map_err(format::io_error)?
        .into_std_file()
        .sync_all()
        .map_err(format::io_error)?;
    Ok(())
}
fn execute<T>(
    p: &PreparedContentInstall,
    cancel: &AtomicBool,
    before_publish: impl FnOnce() -> Result<T>,
) -> Result<()> {
    format::check_cancel(cancel)?;
    same(&p.source, Path::new(&p.plan.source_root))?;
    same(&p.parent, Path::new(&p.plan.parent_dir))?;
    if scan(&p.source, cancel)? != p.plan.files {
        return Err(changed());
    }
    let required = p
        .plan
        .total_bytes
        .parse::<u64>()
        .map_err(|_| changed())?
        .checked_add(16 * 1024 * 1024)
        .ok_or_else(changed)?;
    if fs2::available_space(&p.plan.parent_dir).map_err(format::io_error)? < required {
        return Err(HubError::new("DISK_FULL", "资源安装可用空间不足。"));
    }
    let leaf = Path::new(&p.plan.destination)
        .file_name()
        .ok_or_else(changed)?;
    p.parent.create_dir(leaf).map_err(format::io_error)?;
    let container = p.parent.open_dir_nofollow(leaf).map_err(format::io_error)?;
    write_new(
        &container,
        "CONTENT_INSTALL_INCOMPLETE",
        p.plan.id.as_bytes(),
    )?;
    sync(&container)?;
    sync(&p.parent)?;
    container.create_dir("data").map_err(format::io_error)?;
    let target = container
        .open_dir_nofollow("data")
        .map_err(format::io_error)?;
    for entry in &p.plan.files {
        format::check_cancel(cancel)?;
        let (parent, name) = format::parent(&target, &entry.path)?;
        if entry.kind == "directory" {
            parent.create_dir(name).map_err(format::io_error)?;
        } else {
            let mut input = format::open_file(&p.source, &entry.path)?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            let mut output = parent.open_with(name, &options).map_err(format::io_error)?;
            let (size, digest) = format::transfer_hashed(
                &mut input,
                &mut output,
                entry.size.parse().map_err(|_| changed())?,
                cancel,
            )?;
            if size.to_string() != entry.size || Some(digest) != entry.sha256 {
                return Err(changed());
            }
            output.sync_all().map_err(format::io_error)?;
        }
    }
    if scan(&p.source, cancel)? != p.plan.files || scan(&target, cancel)? != p.plan.files {
        return Err(changed());
    }
    same(&p.source, Path::new(&p.plan.source_root))?;
    same(&p.parent, Path::new(&p.plan.parent_dir))?;
    same(&container, Path::new(&p.plan.destination))?;
    same(
        &target,
        if p.deployment.is_some() {
            Path::new(&p.plan.destination).join("data")
        } else {
            PathBuf::from(&p.plan.payload_root)
        }
        .as_path(),
    )?;
    // Serialize cancellation with the durable publication boundary.
    let _gate = before_publish()?;
    format::check_cancel(cancel)?;
    let bytes = serde_json::to_vec(&p.plan.files).map_err(|_| changed())?;
    write_new(&container, "content-manifest.json", &bytes)?;
    write_new(&container, "content-verification.json", &serde_json::to_vec(&serde_json::json!({"schema_version":1,"plan_id":p.plan.id,"manifest_sha256":p.plan.manifest_sha256,"payload_root":if p.deployment.is_some() { p.plan.payload_root.as_str() } else { "data" },"target_instance_id":p.deployment.as_ref().map(|d| &d.instance.id),"verified_at":now(),"game_compatibility":"unverified"})).map_err(|_| changed())?)?;
    for entry in p.plan.files.iter().rev().filter(|e| e.kind == "directory") {
        let (parent, name) = format::parent(&target, &entry.path)?;
        sync(&parent.open_dir_nofollow(name).map_err(format::io_error)?)?;
    }
    sync(&target)?;
    if let Some(deployment) = &p.deployment {
        publish_deployment(p, &container, deployment)?;
    }
    sync(&container)?;
    container
        .remove_file("CONTENT_INSTALL_INCOMPLETE")
        .map_err(format::io_error)?;
    sync(&container)?;
    sync(&p.parent)?;
    Ok(())
}
impl Hub {
    async fn content_install_scope(
        &self,
        source: &Path,
        parent: &Path,
        deployment: Option<&Deployment>,
    ) -> Result<()> {
        self.check_resource_claims().await?;
        if crate::play_instances::overlaps(source, parent)? {
            return Err(HubError::new(
                "CONTENT_SCOPE_OVERLAP",
                "源包与目标父目录必须独立。",
            ));
        }
        // A library may contain other copies, but cannot itself be a package.
        for entry in self.content_entries().await? {
            if let Ok(identity) = same_file::Handle::from_path(&entry.root) {
                for ancestor in parent.ancestors() {
                    if same_file::Handle::from_path(ancestor).is_ok_and(|held| held == identity) {
                        return Err(HubError::new(
                            "CONTENT_SCOPE_OVERLAP",
                            "目标父目录不能位于已登记资源包内。",
                        ));
                    }
                }
            }
        }
        let data = self.dashboard().await?;
        let mut protected = vec![self.data_dir.clone()];
        if let Some(workspace) = data.workspace {
            protected.extend([
                PathBuf::from(workspace.save_dir),
                PathBuf::from(workspace.backup_dir),
            ]);
        }
        protected.extend(
            data.play_instances
                .into_iter()
                .filter(|i| !deployment.is_some_and(|d| d.instance.id == i.id))
                .map(|i| PathBuf::from(i.data_root)),
        );
        protected.extend(
            data.installations
                .into_iter()
                .map(|i| paths::installation_scope(&i.working_dir)),
        );
        for root in protected {
            if crate::play_instances::overlaps(parent, &root)? {
                return Err(HubError::new(
                    "CONTENT_SCOPE_OVERLAP",
                    "目标父目录必须独立于安装、实例、存档、备份与元数据目录。",
                ));
            }
        }
        Ok(())
    }
    pub async fn prepare_content_install(
        &self,
        entry_id: &str,
        parent_dir: &str,
    ) -> Result<ContentInstallPlan> {
        self.prepare_content_install_target(entry_id, parent_dir, None)
            .await
    }
    pub async fn prepare_instance_content_install(
        &self,
        entry_id: &str,
        instance_id: &str,
    ) -> Result<InstanceContentInstallPlan> {
        let instance = self.instance(instance_id).await?;
        let plan = self
            .prepare_content_install_target(entry_id, &instance.data_root, Some(instance_id))
            .await?;
        Ok(InstanceContentInstallPlan {
            instance_id: instance.id,
            instance_name: instance.name,
            installation_id: instance.installation_id,
            plan,
        })
    }
    async fn deployment_context(
        &self,
        instance_id: &str,
        entry: &crate::ContentEntry,
    ) -> Result<Deployment> {
        if !cfg!(any(target_os = "macos", target_os = "linux")) {
            return Err(HubError::new(
                "CONTENT_DEPLOY_PLATFORM_UNSUPPORTED",
                "此平台的资源发布尚未验证。",
            ));
        }
        let instance = self.instance(instance_id).await?;
        let installation = self
            .dashboard()
            .await?
            .installations
            .into_iter()
            .find(|i| i.id == instance.installation_id)
            .ok_or_else(changed)?;
        if installation.sha256 != instance.installation_sha256
            || paths::digest(PathBuf::from(&installation.executable)).await? != installation.sha256
        {
            return Err(changed());
        }
        let context = Deployment {
            instance,
            installation,
            definitions: entry.definitions.clone(),
            metadata_sha256: entry.metadata_sha256.clone(),
            kind: entry.kind.clone(),
        };
        tokio::task::spawn_blocking(move || {
            check_deployment(&context)?;
            Ok(context)
        })
        .await
        .map_err(|_| changed())?
    }
    async fn prepare_content_install_target(
        &self,
        entry_id: &str,
        parent_dir: &str,
        instance_id: Option<&str>,
    ) -> Result<ContentInstallPlan> {
        let _gate = self.mutation.lock().await;
        let entry = self
            .content_entries()
            .await?
            .into_iter()
            .find(|e| e.id == entry_id && e.last_error.is_none())
            .ok_or_else(changed)?;
        let source_path = paths::canonical(&entry.root, true)?;
        if paths::text(&source_path)? != entry.root {
            return Err(changed());
        }
        let parent_path = paths::canonical(parent_dir, true)?;
        let deployment = if let Some(id) = instance_id {
            let context = self.deployment_context(id, &entry).await?;
            if Path::new(&context.instance.data_root) != parent_path {
                return Err(changed());
            }
            Some(context)
        } else {
            None
        };
        self.content_install_scope(&source_path, &parent_path, deployment.as_ref())
            .await?;
        let plan_id = id();
        let destination = parent_path.join(if deployment.is_some() {
            format!(".hub-content-{plan_id}")
        } else {
            format!("content-{plan_id}")
        });
        let prepared = tokio::task::spawn_blocking(move || {
            let source = Dir::open_ambient_dir(&source_path, cap_std::ambient_authority()).map_err(format::io_error)?;
            let parent = Dir::open_ambient_dir(&parent_path, cap_std::ambient_authority()).map_err(format::io_error)?;
            let files = scan(&source, &AtomicBool::new(false))?;
            let metadata = if entry.kind == "mod" { "modinfo.json" } else { "soundpack.txt" };
            if deployment.is_some() && files.iter().any(|file| file.path.contains('/') && matches!(Path::new(&file.path).file_name().and_then(|n| n.to_str()), Some("modinfo.json" | "soundpack.txt"))) {
                return Err(HubError::new("CONTENT_NESTED_PACKAGE_UNSUPPORTED", "一次只部署单个资源根；请先分别登记嵌套包。"));
            }
            if !files.iter().any(|f| f.path == metadata && f.sha256.as_deref() == Some(&entry.metadata_sha256)) { return Err(changed()); }
            let total = files.iter().try_fold(0u64, |sum, f| sum.checked_add(f.size.parse().ok()?)).ok_or_else(changed)?;
            let manifest_sha256 = format::sha(&serde_json::to_vec(&files).map_err(|_| changed())?);
            let mut plan = ContentInstallPlan { id: plan_id, plan_digest: String::new(), entry_id: entry.id, source_root: entry.root, parent_dir: paths::text(&parent_path)?, destination: paths::text(&destination)?, payload_root: paths::text(&destination.join("data"))?, content_kind: entry.kind, manifest_sha256, files, total_bytes: total.to_string(), expires_at: (chrono::Utc::now()+chrono::Duration::minutes(10)).to_rfc3339(), warnings: vec!["复制到独立资源库目录，不写入游戏安装或世界，不自动启用。".into(), "包含整个包的普通文件和目录；链接、特殊文件和硬链接不支持。".into(), "完整文件清单 SHA-256 不证明来源真实性、版本兼容或声音可播放；不保留权限、时间戳及扩展属性。".into(), "同 ID 资源可并存；启用前仍需检查目标游戏的依赖、冲突与存档影响。".into(), "中断、失败或取消保留未完成目录，不自动删除或重放。".into()] };
            if let Some(target) = &deployment {
                plan.payload_root = paths::text(&parent_path.join(role(&plan.content_kind)).join(format!("hub-{}", plan.id)))?;
                plan.warnings = vec!["仅部署到选定实例的资源目录，不修改安装或已有世界配置，不自动启用。".into(), "先在实例内暂存并校验，再发布到新目录；重复 ID 拒绝，不覆盖已有资源。".into(), "失败或中断保留暂存现场；发布后状态记录失败时需核对现场，不自动重复复制。".into(), "不改配置；若已有配置引用这些 ID，下次启动可能加载。游戏实际加载、模组兼容及声音播放仍需验收。".into()];
                if target.instance.data_root != plan.parent_dir { return Err(changed()); }
            }
            plan.plan_digest = format::sha(&serde_json::to_vec(&plan).map_err(|_| changed())?);
            Ok::<_, HubError>(PreparedContentInstall { plan, source, parent, deployment })
        }).await.map_err(|_| changed())??;
        let plan = prepared.plan.clone();
        let mut plans = self.content_install_plans.lock().await;
        plans.retain(|_, p| p.plan.expires_at > now());
        if plans.len() >= 8 {
            return Err(HubError::new(
                "PLAN_LIMIT",
                "资源安装计划过多，请稍后重试。",
            ));
        }
        plans.insert(plan.id.clone(), Arc::new(prepared));
        Ok(plan)
    }
    pub async fn content_install_tasks(&self) -> Result<Vec<ContentInstallTask>> {
        let mut rows: Vec<ContentInstallTask> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM content_install_task ORDER BY created_at DESC,id LIMIT 50"
        ))
        .fetch_all(&self.pool)
        .await
        .map_err(HubError::db)?;
        let failures = self.runtime_recovery.lock().await;
        for row in &mut rows {
            if failures
                .get(&row.id)
                .is_some_and(|f| matches!(f.kind, crate::task_recovery::TaskKind::ContentInstall))
            {
                row.state = "recovery_required".into();
                row.phase = "recording_failed".into();
                row.error_code = Some("TASK_RESULT_PERSIST_FAILED".into());
            }
        }
        Ok(rows)
    }
    pub async fn start_content_install(
        self: &Arc<Self>,
        plan_id: &str,
        digest: &str,
        key: &str,
        confirmed: bool,
    ) -> Result<String> {
        let _gate = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请确认独立安装目录与资源范围。",
            ));
        }
        if uuid::Uuid::parse_str(key).is_err() {
            return Err(HubError::new("REQUEST_KEY_INVALID", "请求标识无效。"));
        }
        let request_digest =
            format::sha(&serde_json::to_vec(&(plan_id, digest)).map_err(|_| changed())?);
        if let Some((existing, previous)) = sqlx::query_as::<_, (String, String)>(
            "SELECT id,request_digest FROM content_install_task WHERE request_key=?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(HubError::db)?
        {
            if previous != request_digest {
                return Err(HubError::new(
                    "REQUEST_CONFLICT",
                    "请求标识已用于其他操作。",
                ));
            }
            return Ok(existing);
        }
        let prepared = self
            .content_install_plans
            .lock()
            .await
            .get(plan_id)
            .cloned()
            .ok_or_else(changed)?;
        let plan = &prepared.plan;
        if plan.plan_digest != digest || plan.expires_at <= now() {
            return Err(changed());
        }
        if let Some(target) = &prepared.deployment {
            let entry = self
                .content_entries()
                .await?
                .into_iter()
                .find(|e| e.id == plan.entry_id && e.last_error.is_none())
                .ok_or_else(changed)?;
            let current = self.deployment_context(&target.instance.id, &entry).await?;
            if current.instance.data_root != target.instance.data_root
                || current.instance.installation_sha256 != target.instance.installation_sha256
                || current.installation.executable != target.installation.executable
                || current.metadata_sha256 != target.metadata_sha256
            {
                return Err(changed());
            }
        }
        self.content_install_scope(
            Path::new(&plan.source_root),
            Path::new(&plan.parent_dir),
            prepared.deployment.as_ref(),
        )
        .await?;
        same(&prepared.source, Path::new(&plan.source_root))?;
        same(&prepared.parent, Path::new(&plan.parent_dir))?;
        let task_id = id();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO content_install_task(id,request_key,request_digest,entry_id,source_root,destination,payload_root,manifest_sha256,state,phase,created_at) VALUES(?,?,?,?,?,?,?,?,'running','copying',?)").bind(&task_id).bind(key).bind(request_digest).bind(&plan.entry_id).bind(&plan.source_root).bind(&plan.destination).bind(&plan.payload_root).bind(&plan.manifest_sha256).bind(now()).execute(&mut *tx).await.map_err(HubError::db)?;
        audit(&mut tx, &task_id, "running", None).await?;
        tx.commit().await.map_err(HubError::db)?;
        self.content_install_plans.lock().await.remove(plan_id);
        let flag = Arc::new(AtomicBool::new(false));
        self.scan_controls
            .lock()
            .await
            .insert(task_id.clone(), flag.clone());
        let hub = self.clone();
        let worker_id = task_id.clone();
        tokio::spawn(async move {
            let job_hub = hub.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                execute(&prepared, &flag, || Ok(job_hub.mutation.blocking_lock()))
            })
            .await
            .unwrap_or_else(|_| Err(HubError::new("CONTENT_WORKER_FAILED", "资源安装线程中断。")));
            let _gate = hub.mutation.lock().await;
            let (state, error) = match outcome {
                Ok(()) => ("succeeded", None),
                Err(e) if e.code == "TASK_CANCELLED" => ("cancelled", Some(e.code)),
                Err(e) => ("failed", Some(e.code)),
            };
            if hub
                .finish_content_install(&worker_id, state, error)
                .await
                .is_err()
            {
                hub.mark_task_recovery(crate::task_recovery::TaskKind::ContentInstall, &worker_id)
                    .await;
            }
            hub.scan_controls.lock().await.remove(&worker_id);
        });
        Ok(task_id)
    }
    async fn finish_content_install(
        &self,
        id: &str,
        state: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("UPDATE content_install_task SET state=?,phase='finished',error_code=?,finished_at=?,revision=revision+1 WHERE id=?").bind(state).bind(error).bind(now()).bind(id).execute(&mut *tx).await.map_err(HubError::db)?;
        audit(&mut tx, id, state, error).await?;
        tx.commit().await.map_err(HubError::db)
    }
    pub async fn cancel_content_install(&self, task_id: &str) -> Result<()> {
        let _gate = self.mutation.lock().await;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM content_install_task WHERE id=? AND state IN ('running','cancelling'))").bind(task_id).fetch_one(&self.pool).await.map_err(HubError::db)?;
        if exists {
            let mut tx = self.pool.begin().await.map_err(HubError::db)?;
            sqlx::query(
                "UPDATE content_install_task SET state='cancelling',revision=revision+1 WHERE id=?",
            )
            .bind(task_id)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
            audit(&mut tx, task_id, "cancelling", None).await?;
            tx.commit().await.map_err(HubError::db)?;
            if let Some(flag) = self.scan_controls.lock().await.get(task_id) {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        }
        Ok(())
    }
}
async fn audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task: &str,
    state: &str,
    error: Option<&str>,
) -> Result<()> {
    sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
        .bind(id())
        .bind(task)
        .bind("content.install")
        .bind(state)
        .bind(error)
        .bind(now())
        .execute(&mut **tx)
        .await
        .map_err(HubError::db)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_at_publication_keeps_incomplete_marker_and_no_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let source = temp.path().join("source");
        let parent = temp.path().join("library");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(source.join("soundpack.txt"), "NAME: test\nVIEW: Test\n").unwrap();
        let entry = hub
            .register_content(source.to_str().unwrap(), "soundpack")
            .await
            .unwrap();
        let plan = hub
            .prepare_content_install(&entry.id, parent.to_str().unwrap())
            .await
            .unwrap();
        let prepared = hub
            .content_install_plans
            .lock()
            .await
            .remove(&plan.id)
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let error = execute(&prepared, &cancelled, || {
            cancelled.store(true, std::sync::atomic::Ordering::Release);
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.code, "TASK_CANCELLED");
        assert!(
            Path::new(&plan.destination)
                .join("CONTENT_INSTALL_INCOMPLETE")
                .exists()
        );
        assert!(
            !Path::new(&plan.destination)
                .join("content-verification.json")
                .exists()
        );
        assert!(Path::new(&plan.payload_root).join("soundpack.txt").exists());
    }
    #[tokio::test]
    async fn cancellation_is_audited_and_keeps_protection_until_worker_ends() {
        let temp = tempfile::tempdir().unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        sqlx::query("INSERT INTO content_install_task(id,request_key,request_digest,entry_id,source_root,destination,payload_root,manifest_sha256,state,phase,created_at) VALUES('t','key','digest','e','/source','/target','/target/data','hash','running','copying','now')").execute(&hub.pool).await.unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        hub.scan_controls
            .lock()
            .await
            .insert("t".into(), flag.clone());
        let claims = hub.dashboard().await.unwrap().resource_claims;
        assert_eq!(claims.len(), 3);
        assert!(claims.iter().any(|c| c.scope == "directory"
            && c.path.as_deref() == Some("/source")
            && c.access == "read"));
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "TASK_CONFLICT"
        );
        sqlx::query("CREATE TRIGGER reject_cancel BEFORE INSERT ON operation_log WHEN NEW.status='cancelling' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&hub.pool).await.unwrap();
        assert!(hub.cancel_content_install("t").await.is_err());
        assert!(!flag.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(
            hub.content_install_tasks().await.unwrap()[0].state,
            "running"
        );
        sqlx::query("DROP TRIGGER reject_cancel")
            .execute(&hub.pool)
            .await
            .unwrap();
        hub.cancel_content_install("t").await.unwrap();
        assert!(flag.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(
            hub.content_install_tasks().await.unwrap()[0].state,
            "cancelling"
        );
        assert_eq!(
            hub.check_resource_claims().await.unwrap_err().code,
            "TASK_CONFLICT"
        );
        hub.finish_content_install("t", "cancelled", Some("TASK_CANCELLED"))
            .await
            .unwrap();
        assert!(hub.dashboard().await.unwrap().resource_claims.is_empty());
    }
}
