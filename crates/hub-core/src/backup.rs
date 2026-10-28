use crate::{
    BackupEntry, BackupPlan, BackupRecord, BackupTask, Hub, HubError, Result,
    backup_format::{
        self as format, MAX_BYTES, MAX_MANIFEST, Manifest, Receipt, check_cancel, io_error,
        open_file, parent, sha,
    },
    database::{id, now},
    paths,
    world_scan::inspect_source,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, OpenOptions};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    },
};

const COLUMNS: &str = "id,request_key,workspace_id,backup_id,operation,state,phase,processed_entries,revision,error_code,created_at,finished_at,play_instance_id,source_root";
#[derive(Clone)]
pub(crate) struct PreparedPlan {
    pub plan: BackupPlan,
    workspace_id: String,
    instance: Option<crate::PlayInstance>,
    source_identity: Arc<same_file::Handle>,
    backup_identity: Arc<same_file::Handle>,
}
#[derive(Clone)]
enum Work {
    Create(PreparedPlan),
    Verify(BackupRecord),
}
struct Output {
    manifest: Manifest,
    verification: format::BackupVerification,
    partial: Option<String>,
    publish_root: Option<Dir>,
}
fn changed() -> HubError {
    HubError::new(
        "PLAN_STALE",
        "存档文件范围或大小在预览后发生变化，请重新预览。",
    )
}
fn phase_name(code: u8) -> &'static str {
    match code {
        1 => "copying",
        2 => "checking_source",
        3 => "archiving",
        4 => "verifying",
        _ => "preflight",
    }
}
fn source_entries(source: &Path, cancel: &AtomicBool) -> Result<Vec<BackupEntry>> {
    let report = inspect_source(source, cancel)?;
    if !report.enumeration_complete {
        return Err(HubError::new(
            "INCOMPLETE_SCOPE",
            "扫描范围不完整，无法创建完整备份。",
        ));
    }
    if !report.issues.is_empty() {
        return Err(HubError::new(
            "UNSUPPORTED_FILE_TYPE",
            "范围内存在链接、特殊文件或不可读条目，备份已中止，未忽略任何文件。",
        ));
    }
    let mut total = 0_u64;
    let mut result = Vec::with_capacity(report.entries.len());
    for entry in report.entries {
        format::validate_path(&entry.relative_path).map_err(|_| {
            HubError::new(
                "PATH_UNSUPPORTED",
                "源文件名不符合当前可移植归档的路径要求。",
            )
        })?;
        let size: u64 = entry.size.parse().map_err(|_| changed())?;
        total = total.checked_add(size).ok_or_else(changed)?;
        if total > MAX_BYTES {
            return Err(HubError::new(
                "LIMIT_EXCEEDED",
                "当前备份上限为 64 GiB，请缩小工作区范围。",
            ));
        }
        result.push(BackupEntry {
            relative_path: entry.relative_path,
            file_type: entry.file_type,
            size: entry.size,
            sha256: None,
            mtime: None,
        });
    }
    // Bound metadata before any work directory is created.
    if serde_json::to_vec(&result).map_err(|_| changed())?.len() as u64 > MAX_MANIFEST / 2 {
        return Err(HubError::new(
            "LIMIT_EXCEEDED",
            "文件清单超过当前元数据限制。",
        ));
    }
    Ok(result)
}
fn same_scope(a: &[BackupEntry], b: &[BackupEntry]) -> bool {
    a.iter()
        .map(|e| (&e.relative_path, &e.file_type, &e.size))
        .eq(b.iter().map(|e| (&e.relative_path, &e.file_type, &e.size)))
}
fn safe_roots(source: &str, backup: &str, data: &Path) -> Result<(PathBuf, PathBuf)> {
    let source_path = paths::canonical(source, true)?;
    let backup_path = paths::canonical(backup, true)?;
    if paths::text(&source_path)? != source || paths::text(&backup_path)? != backup {
        return Err(changed());
    }
    if crate::play_instances::overlaps(&source_path, &backup_path)?
        || crate::play_instances::overlaps(&source_path, data)?
        || crate::play_instances::overlaps(&backup_path, data)?
    {
        return Err(HubError::new(
            "PATH_OVERLAP",
            "源、备份与应用元数据目录必须分离。",
        ));
    }
    Ok((source_path, backup_path))
}
fn check_space(path: &Path, required: u64) -> Result<()> {
    if fs2::available_space(path).map_err(io_error)? < required {
        return Err(HubError::new(
            "DISK_FULL",
            "空间不足以保存冻结副本和未压缩最坏情况下的归档。",
        ));
    }
    Ok(())
}
fn freeze(
    source: &Dir,
    staging: &Dir,
    entries: &mut [BackupEntry],
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<()> {
    for (index, entry) in entries.iter_mut().enumerate() {
        check_cancel(cancel)?;
        let (target, name) = parent(staging, &entry.relative_path)?;
        if entry.file_type == "directory" {
            target.create_dir(&name).map_err(io_error)?;
        } else {
            let mut input = open_file(source, &entry.relative_path)?;
            let before = input.metadata().map_err(io_error)?;
            #[cfg(unix)]
            {
                use cap_std::fs::MetadataExt;
                if before.nlink() > 1 {
                    return Err(HubError::new(
                        "UNSUPPORTED_FILE_TYPE",
                        "当前完整备份不支持带硬链接语义的源文件。",
                    ));
                }
            }
            let expected: u64 = entry.size.parse().map_err(|_| changed())?;
            if before.len() != expected {
                return Err(changed());
            }
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            let mut output = target.open_with(name, &options).map_err(io_error)?;
            let mut count = 0_u64;
            let mut hash = Sha256::new();
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                check_cancel(cancel)?;
                let n = input.read(&mut buffer).map_err(io_error)?;
                if n == 0 {
                    break;
                }
                count = count.checked_add(n as u64).ok_or_else(changed)?;
                if count > expected {
                    return Err(changed());
                }
                output.write_all(&buffer[..n]).map_err(io_error)?;
                hash.update(&buffer[..n]);
            }
            output.sync_all().map_err(io_error)?;
            let after = input.metadata().map_err(io_error)?;
            if count != expected
                || before.len() != after.len()
                || before.modified().ok() != after.modified().ok()
            {
                return Err(HubError::new(
                    "SOURCE_CHANGED",
                    "复制期间源文件发生变化，未发布备份。",
                ));
            }
            let actual = format!("{:x}", hash.finalize());
            if entry
                .sha256
                .as_deref()
                .is_some_and(|expected| expected != actual)
            {
                return Err(HubError::new(
                    "CONTENT_CONFIG_CHANGED",
                    "配置原字节在预览后发生变化，未发布保护备份。",
                ));
            }
            entry.sha256 = Some(actual);
        }
        progress.store(index as u64 + 1, Ordering::Release);
    }
    Ok(())
}
fn validate_source(
    source: &Dir,
    entries: &[BackupEntry],
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<()> {
    for (index, entry) in entries.iter().enumerate() {
        check_cancel(cancel)?;
        if entry.file_type == "file" {
            let mut file = open_file(source, &entry.relative_path)?;
            let size: u64 = entry.size.parse().map_err(|_| changed())?;
            let (actual, hash) = format::hash_reader(&mut file, size, cancel)?;
            if actual != size || entry.sha256.as_deref() != Some(&hash) {
                return Err(HubError::new(
                    "SOURCE_CHANGED",
                    "源文件与冻结副本不一致，未发布备份。",
                ));
            }
        }
        progress.store(index as u64 + 1, Ordering::Release);
    }
    Ok(())
}
fn header(size: u64, directory: bool) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_size(size);
    header.set_mode(if directory { 0o755 } else { 0o600 });
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    header.set_entry_type(if directory {
        tar::EntryType::Directory
    } else {
        tar::EntryType::Regular
    });
    header.set_cksum();
    header
}
fn write_json(root: &Dir, name: &str, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = root.open_with(name, &options).map_err(io_error)?;
    file.write_all(bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}
struct CancellableReader<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}
impl<R: Read> Read for CancellableReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::other("task cancelled"));
        }
        self.inner.read(buffer)
    }
}
fn create_archive(
    work: &Dir,
    data: &Dir,
    manifest: &Manifest,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<Receipt> {
    let manifest_bytes = serde_json::to_vec(manifest).map_err(|_| changed())?;
    if manifest_bytes.len() as u64 > MAX_MANIFEST {
        return Err(HubError::new("LIMIT_EXCEEDED", "备份清单超过安全限制。"));
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let file = work
        .open_with("archive.tar.zst", &options)
        .map_err(io_error)?;
    let mut encoder = zstd::stream::write::Encoder::new(file, 3).map_err(io_error)?;
    encoder.include_checksum(true).map_err(io_error)?;
    let mut archive = tar::Builder::new(encoder);
    archive
        .append_data(
            &mut header(manifest_bytes.len() as u64, false),
            "manifest.json",
            manifest_bytes.as_slice(),
        )
        .map_err(io_error)?;
    for (index, entry) in manifest.entries.iter().enumerate() {
        check_cancel(cancel)?;
        let path = format!("data/{}", entry.relative_path);
        let directory = entry.file_type == "directory";
        let size: u64 = entry.size.parse().map_err(|_| changed())?;
        if directory {
            archive
                .append_data(&mut header(0, true), path, std::io::empty())
                .map_err(io_error)?;
        } else {
            archive
                .append_data(
                    &mut header(size, false),
                    path,
                    CancellableReader {
                        inner: open_file(data, &entry.relative_path)?,
                        cancel,
                    },
                )
                .map_err(io_error)?;
        }
        progress.store(index as u64 + 1, Ordering::Release);
    }
    let file = archive
        .into_inner()
        .map_err(io_error)?
        .finish()
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    drop(file);
    let mut archive = open_file(work, "archive.tar.zst")?;
    let (_, archive_sha256) =
        format::hash_reader(&mut archive, MAX_BYTES + 128 * 1024 * 1024, cancel)?;
    Ok(Receipt {
        schema_version: 1,
        backup_id: manifest.backup_id.clone(),
        archive_file: "archive.tar.zst".into(),
        archive_sha256,
        manifest_sha256: sha(&manifest_bytes),
        verification: "verified".into(),
        verified_at: now(),
        verifier_version: env!("CARGO_PKG_VERSION").into(),
    })
}
fn do_create(
    prepared: &PreparedPlan,
    backup_id: &str,
    data_dir: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU64,
    phase: &AtomicU8,
) -> Result<Output> {
    let (source_path, backup_path) = safe_roots(
        &prepared.plan.source_root,
        &prepared.plan.backup_dir,
        data_dir,
    )?;
    check_space(
        &backup_path,
        prepared
            .plan
            .required_free_bytes
            .parse()
            .map_err(|_| changed())?,
    )?;
    if !same_scope(
        &source_entries(&source_path, cancel)?,
        &prepared.plan.entries,
    ) {
        return Err(changed());
    }
    let backup =
        Dir::open_ambient_dir(&backup_path, cap_std::ambient_authority()).map_err(io_error)?;
    let source =
        Dir::open_ambient_dir(&source_path, cap_std::ambient_authority()).map_err(io_error)?;
    if same_file::Handle::from_file(source.try_clone().map_err(io_error)?.into_std_file())
        .map_err(io_error)?
        != *prepared.source_identity
        || same_file::Handle::from_file(backup.try_clone().map_err(io_error)?.into_std_file())
            .map_err(io_error)?
            != *prepared.backup_identity
    {
        return Err(changed());
    }
    let partial = format!(".partial-{backup_id}");
    backup.create_dir(&partial).map_err(io_error)?;
    let work = backup.open_dir_nofollow(&partial).map_err(io_error)?;
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;
        backup
            .set_permissions(&partial, cap_std::fs::Permissions::from_mode(0o700))
            .map_err(io_error)?;
    }
    write_json(
        &work,
        "owner.json",
        serde_json::to_vec(&serde_json::json!({"task_id":backup_id,"state":"unpublished"}))
            .map_err(|_| changed())?
            .as_slice(),
    )?;
    work.create_dir("data").map_err(io_error)?;
    let data = work.open_dir_nofollow("data").map_err(io_error)?;
    let mut entries = prepared.plan.entries.clone();
    phase.store(1, Ordering::Release);
    progress.store(0, Ordering::Release);
    freeze(&source, &data, &mut entries, cancel, progress)?;
    phase.store(2, Ordering::Release);
    progress.store(0, Ordering::Release);
    validate_source(&source, &entries, cancel, progress)?;
    if !same_scope(&source_entries(&source_path, cancel)?, &entries) {
        return Err(changed());
    }
    let manifest = Manifest {
        schema_version: if prepared.instance.is_some() { 2 } else { 1 },
        backup_id: backup_id.into(),
        created_at: now(),
        scope: if prepared.instance.is_some() {
            "instance_data_root"
        } else {
            "save_root"
        }
        .into(),
        world_id: None,
        world_alias: None,
        source_token: prepared
            .instance
            .as_ref()
            .map_or(prepared.workspace_id.clone(), |i| i.id.clone()),
        consistency: "best_effort".into(),
        game: None,
        profile_id: None,
        adapter_version: None,
        tool_version: env!("CARGO_PKG_VERSION").into(),
        compression: "zstd".into(),
        archive_format: "tar".into(),
        characters: vec![],
        entry_count: entries.len() as u32,
        file_count: entries.iter().filter(|e| e.file_type == "file").count() as u32,
        total_bytes: prepared.plan.total_bytes.clone(),
        entries,
        extensions: serde_json::json!({"platform":std::env::consts::OS,"arch":std::env::consts::ARCH,"boundary_verified":false,"instance_source":prepared.instance.as_ref().map(|i|serde_json::json!({"instance_id":i.id,"installation_id":i.installation_id,"installation_sha256":i.installation_sha256,"binding_verified":false}))}),
        compression_version: zstd::zstd_safe::version_string().into(),
        world_contexts: vec![],
    };
    phase.store(3, Ordering::Release);
    progress.store(0, Ordering::Release);
    let receipt = create_archive(&work, &data, &manifest, cancel, progress)?;
    write_json(
        &work,
        "receipt.json",
        &serde_json::to_vec(&receipt).map_err(|_| changed())?,
    )?;
    phase.store(4, Ordering::Release);
    progress.store(0, Ordering::Release);
    let (_, verification) = format::verify_dir(&work, Some(&receipt.archive_sha256), cancel)?;
    drop(data);
    work.remove_dir_all("data").map_err(io_error)?;
    // owner.json stays until the service publishes under its cancellation gate.
    Ok(Output {
        manifest,
        verification,
        partial: Some(partial),
        publish_root: Some(backup),
    })
}
impl Hub {
    pub async fn prepare_backup(&self) -> Result<BackupPlan> {
        let _guard = self.mutation.lock().await;
        self.prepare_backup_inner(None).await
    }
    pub async fn prepare_instance_backup(&self, instance_id: &str) -> Result<BackupPlan> {
        let _guard = self.mutation.lock().await;
        let instance = self.instance(instance_id).await?;
        if !format::valid_instance_provenance(
            &instance.id,
            &instance.installation_id,
            &instance.installation_sha256,
        ) {
            return Err(HubError::new(
                "INSTANCE_CONTEXT_INVALID",
                "实例来源记录无效，无法生成可验证备份；未创建临时目录。",
            ));
        }

        let installation = self.installation(&instance.installation_id).await?;
        self.instance_identity(
            &instance,
            &installation.working_dir,
            &installation.executable,
        )
        .await?;
        self.prepare_backup_inner(Some(instance)).await
    }
    async fn prepare_backup_inner(
        &self,
        instance: Option<crate::PlayInstance>,
    ) -> Result<BackupPlan> {
        if instance.is_none() {
            self.check_resource_claims().await?;
        }
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let source_root = instance
            .as_ref()
            .map_or(workspace.save_dir.clone(), |i| i.data_root.clone());
        let (source, backup) = safe_roots(&source_root, &workspace.backup_dir, &self.data_dir)?;
        if instance.is_some() {
            self.check_file_transfer_resources(&source_root, &workspace.backup_dir)
                .await?;
        }
        let source_identity = Arc::new(same_file::Handle::from_path(&source).map_err(io_error)?);
        let backup_identity = Arc::new(same_file::Handle::from_path(&backup).map_err(io_error)?);
        let entries =
            tokio::task::spawn_blocking(move || source_entries(&source, &AtomicBool::new(false)))
                .await
                .map_err(|_| changed())??;
        let total = entries
            .iter()
            .try_fold(0_u64, |sum, e| {
                e.size
                    .parse::<u64>()
                    .ok()
                    .and_then(|size| sum.checked_add(size))
            })
            .ok_or_else(changed)?;
        let required = total
            .checked_mul(2)
            .and_then(|n| n.checked_add(entries.len() as u64 * 2048 + 16 * 1024 * 1024))
            .ok_or_else(changed)?;
        check_space(&backup, required)?;
        let plan_id = id();
        let digest = sha(&serde_json::to_vec(&(
            &plan_id,
            &workspace.id,
            &source_root,
            &workspace.backup_dir,
            &instance,
            &entries,
        ))
        .map_err(|_| changed())?);
        let plan = BackupPlan {
            play_instance_id: instance.as_ref().map(|i| i.id.clone()),
            id: plan_id,
            plan_digest: digest,
            source_root,
            backup_dir: workspace.backup_dir,
            entry_count: entries.len() as u32,
            file_count: entries.iter().filter(|e| e.file_type == "file").count() as u32,
            total_bytes: total.to_string(),
            required_free_bytes: required.to_string(),
            consistency: "best_effort".into(),
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339(),
            entries,
            warnings: vec![
                if instance.is_some() {
                    "范围为整个实例数据根，包含存档、配置及其他普通文件；不含游戏安装与其缓存。"
                        .into()
                } else {
                    "范围为整个存档根目录，包含所有世界候选及共享/未知普通文件。".into()
                },
                "游戏格式与外部进程尚未验证；本次仅为尽力快照，不保证游戏语义一致。".into(),
                "仅保存普通文件字节及空目录，不保证 ACL、扩展属性或全部权限元数据。".into(),
            ],
        };
        let mut plans = self.backup_plans.lock().await;
        plans.retain(|_, p| p.plan.expires_at > now());
        if plans.len() >= 10 {
            return Err(HubError::new("PLAN_LIMIT", "备份计划过多，请稍后重试。"));
        }
        plans.insert(
            plan.id.clone(),
            PreparedPlan {
                plan: plan.clone(),
                workspace_id: workspace.id,
                instance,
                source_identity,
                backup_identity,
            },
        );
        Ok(plan)
    }
    pub(crate) async fn bind_config_backup(
        &self,
        plan_id: &str,
        files: &[crate::ContentConfigFilePreview],
    ) -> Result<BackupPlan> {
        let mut plans = self.backup_plans.lock().await;
        let prepared = plans.get_mut(plan_id).ok_or_else(changed)?;
        if files.iter().any(|file| {
            !prepared
                .plan
                .entries
                .iter()
                .any(|entry| entry.relative_path == file.path && entry.file_type == "file")
        }) {
            plans.remove(plan_id);
            return Err(changed());
        }
        for file in files {
            let entry = prepared
                .plan
                .entries
                .iter_mut()
                .find(|entry| entry.relative_path == file.path)
                .ok_or_else(changed)?;
            entry.sha256 = Some(file.before_sha256.clone());
        }
        prepared.plan.plan_digest = sha(&serde_json::to_vec(&(
            &prepared.plan.id,
            &prepared.workspace_id,
            &prepared.plan.source_root,
            &prepared.plan.backup_dir,
            &prepared.instance,
            &prepared.plan.entries,
        ))
        .map_err(|_| changed())?);
        Ok(prepared.plan.clone())
    }
    pub async fn create_backup(
        self: &Arc<Self>,
        plan_id: &str,
        digest: &str,
        request_key: &str,
        confirmed: bool,
    ) -> Result<BackupTask> {
        let _guard = self.mutation.lock().await;
        if !confirmed {
            return Err(HubError::new(
                "CONFIRMATION_REQUIRED",
                "请检查备份范围并确认尽力快照风险。",
            ));
        }
        let request_digest = sha(format!("create:{plan_id}:{digest}").as_bytes());
        if let Some(task) = self
            .existing_backup_task(request_key, &request_digest)
            .await?
        {
            return Ok(task);
        }
        let prepared = self
            .backup_plans
            .lock()
            .await
            .get(plan_id)
            .cloned()
            .ok_or_else(changed)?;
        if prepared.plan.plan_digest != digest || prepared.plan.expires_at <= now() {
            return Err(changed());
        }
        if prepared.instance.is_some() {
            self.check_file_transfer_resources(
                &prepared.plan.source_root,
                &prepared.plan.backup_dir,
            )
            .await?;
        } else {
            self.check_resource_claims().await?;
        }
        self.backup_plans.lock().await.remove(plan_id);
        let task_id = id();
        self.spawn_backup_task(
            &task_id,
            request_key,
            &request_digest,
            &prepared.workspace_id,
            &task_id,
            Work::Create(prepared.clone()),
        )
        .await
    }
    pub async fn verify_backup(
        self: &Arc<Self>,
        backup_id: &str,
        request_key: &str,
    ) -> Result<BackupTask> {
        let _guard = self.mutation.lock().await;
        let digest = sha(format!("verify:{backup_id}").as_bytes());
        if let Some(task) = self.existing_backup_task(request_key, &digest).await? {
            return Ok(task);
        }
        self.check_resource_claims().await?;
        let record: BackupRecord = sqlx::query_as("SELECT * FROM backup_archive WHERE id=?")
            .bind(backup_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("BACKUP_NOT_FOUND", "备份记录不存在。"))?;
        let task_id = id();
        self.spawn_backup_task(
            &task_id,
            request_key,
            &digest,
            &record.workspace_id,
            backup_id,
            Work::Verify(record.clone()),
        )
        .await
    }
    async fn existing_backup_task(&self, key: &str, digest: &str) -> Result<Option<BackupTask>> {
        if uuid::Uuid::parse_str(key).is_err() {
            return Err(HubError::new("REQUEST_KEY_INVALID", "请求标识无效。"));
        }
        let existing: Option<String> =
            sqlx::query_scalar("SELECT request_digest FROM backup_task WHERE request_key=?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?;
        match existing {
            Some(existing) if existing != digest => Err(HubError::new(
                "REQUEST_CONFLICT",
                "此请求标识已用于不同的操作。",
            )),
            Some(_) => {
                let query = format!("SELECT {COLUMNS} FROM backup_task WHERE request_key=?");
                Ok(Some(
                    sqlx::query_as(&query)
                        .bind(key)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(HubError::db)?,
                ))
            }
            None => Ok(None),
        }
    }
    async fn spawn_backup_task(
        self: &Arc<Self>,
        task_id: &str,
        key: &str,
        digest: &str,
        workspace_id: &str,
        backup_id: &str,
        work: Work,
    ) -> Result<BackupTask> {
        let workspace = self.dashboard().await?.workspace.ok_or_else(changed)?;
        if let Work::Create(prepared) = &work
            && (prepared.workspace_id != workspace.id
                || (prepared.instance.is_none() && prepared.plan.source_root != workspace.save_dir)
                || prepared.plan.backup_dir != workspace.backup_dir)
        {
            return Err(changed());
        }
        if let Work::Create(prepared) = &work
            && let Some(snapshot) = &prepared.instance
        {
            let current = self.instance(&snapshot.id).await?;
            if current.data_root != prepared.plan.source_root
                || current.installation_id != snapshot.installation_id
                || current.installation_sha256 != snapshot.installation_sha256
            {
                return Err(changed());
            }
            let installation = self.installation(&current.installation_id).await?;
            self.instance_identity(
                &current,
                &installation.working_dir,
                &installation.executable,
            )
            .await?;
        }
        let (play_instance_id, source_root) = match &work {
            Work::Create(p) => (
                p.plan.play_instance_id.clone(),
                Some(p.plan.source_root.clone()),
            ),
            Work::Verify(r) => (r.play_instance_id.clone(), r.source_root.clone()),
        };
        let backup_dir = workspace.backup_dir.clone();
        let data_dir = self.data_dir.clone();
        let operation = if matches!(work, Work::Create(_)) {
            "backup.create"
        } else {
            "backup.verify"
        };
        let created = now();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at,play_instance_id,source_root) VALUES (?,?,?,?,?,?,'running','preflight',?,?,?)")
            .bind(task_id).bind(key).bind(digest).bind(workspace_id).bind(backup_id).bind(operation).bind(&created).bind(&play_instance_id).bind(&source_root).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task_id)
            .bind(operation)
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
            .insert(task_id.into(), Arc::clone(&flag));
        let hub = Arc::clone(self);
        let worker_id = task_id.to_owned();
        let archive_id = backup_id.to_owned();
        tokio::spawn(async move {
            let progress = Arc::new(AtomicU64::new(0));
            let phase = Arc::new(AtomicU8::new(0));
            let (job_flag, job_progress, job_phase) =
                (Arc::clone(&flag), Arc::clone(&progress), Arc::clone(&phase));
            let job_work = work.clone();
            let job_id = archive_id.clone();
            let job_dir = backup_dir.clone();
            let mut worker = tokio::task::spawn_blocking(move || match job_work {
                Work::Create(prepared) => do_create(
                    &prepared,
                    &job_id,
                    &data_dir,
                    &job_flag,
                    &job_progress,
                    &job_phase,
                ),
                Work::Verify(record) => {
                    job_phase.store(4, Ordering::Release);
                    let base = Dir::open_ambient_dir(&job_dir, cap_std::ambient_authority())
                        .map_err(io_error)?;
                    if record.relative_dir != record.id
                        || uuid::Uuid::parse_str(&record.id).is_err()
                    {
                        return Err(changed());
                    }
                    let root = base
                        .open_dir_nofollow(&record.relative_dir)
                        .map_err(io_error)?;
                    let (manifest, verification) =
                        format::verify_dir(&root, Some(&record.archive_sha256), &job_flag)?;
                    let matching_source = match &record.play_instance_id {
                        Some(instance_id) => {
                            manifest.schema_version == 2
                                && manifest.scope == "instance_data_root"
                                && manifest.source_token == *instance_id
                        }
                        None => manifest.schema_version == 1 && manifest.scope == "save_root",
                    };
                    if !matching_source
                        || verification.backup_id != record.id
                        || verification.manifest_sha256 != record.manifest_sha256
                    {
                        return Err(format::invalid());
                    }
                    Ok(Output {
                        manifest,
                        verification,
                        partial: None,
                        publish_root: None,
                    })
                }
            });
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut persistence_failed = false;
            let result = loop {
                tokio::select! {
                    result=&mut worker=>break result.unwrap_or_else(|_|Err(HubError::new("BACKUP_WORKER_FAILED","备份工作线程意外中断。"))),
                    _=ticker.tick()=>{
                        if sqlx::query("UPDATE backup_task SET phase=?,processed_entries=?,revision=revision+1 WHERE id=? AND state='running'")
                            .bind(phase_name(phase.load(Ordering::Acquire))).bind(progress.load(Ordering::Acquire) as i64).bind(&worker_id).execute(&hub.pool).await.is_err(){persistence_failed=true;}
                    }
                }
            };
            let result = if persistence_failed {
                Err(HubError::new(
                    "TASK_PROGRESS_PERSIST_FAILED",
                    "任务记录失败，未发布备份。",
                ))
            } else {
                result
            };
            if hub
                .finish_backup(&worker_id, &backup_dir, &work, result, &flag)
                .await
                .is_err()
            {
                hub.mark_task_recovery(crate::task_recovery::TaskKind::Backup, &worker_id)
                    .await;
            }
            hub.scan_controls.lock().await.remove(&worker_id);
        });
        Ok(BackupTask {
            play_instance_id,
            source_root,
            id: task_id.into(),
            request_key: key.into(),
            workspace_id: workspace_id.into(),
            backup_id: backup_id.into(),
            operation: operation.into(),
            state: "running".into(),
            phase: "preflight".into(),
            processed_entries: 0,
            revision: 1,
            error_code: None,
            created_at: created,
            finished_at: None,
        })
    }
    async fn finish_backup(
        &self,
        task_id: &str,
        _backup_dir: &str,
        work: &Work,
        result: Result<Output>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let _guard = self.mutation.lock().await;
        let result = if cancel.load(Ordering::Acquire) {
            Err(HubError::new("TASK_CANCELLED", "任务已取消。"))
        } else {
            result
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let mut result = result;
        if let Ok(output) = &result
            && let Some(partial) = &output.partial
        {
            let published = (|| {
                let base = output.publish_root.as_ref().ok_or_else(changed)?;
                if base.symlink_metadata(&output.manifest.backup_id).is_ok() {
                    return Err(HubError::new(
                        "BACKUP_ALREADY_EXISTS",
                        "目标备份目录已存在，不会覆盖。",
                    ));
                }
                let staged = base.open_dir_nofollow(partial).map_err(io_error)?;
                super::backup_format::sync_directory(&staged)?;
                // Publication and cancellation are serialized under the service mutation gate.
                base.rename(partial, base, &output.manifest.backup_id)
                    .map_err(io_error)?;
                staged.remove_file("owner.json").map_err(|_| {
                    HubError::new(
                        "PUBLICATION_UNCERTAIN",
                        "归档已切换到正式目录，但发布记录未完成；请保留目录并独立校验。",
                    )
                })?;
                super::backup_format::sync_directory(&staged).map_err(|_| {
                    HubError::new(
                        "PUBLICATION_UNCERTAIN",
                        "归档目录持久化确认失败；请保留目录并独立校验。",
                    )
                })?;
                #[cfg(unix)]
                base.try_clone()
                    .map_err(io_error)?
                    .into_std_file()
                    .sync_all()
                    .map_err(|_| {
                        HubError::new(
                            "PUBLICATION_UNCERTAIN",
                            "归档已切换，但目录持久化确认失败；请保留目录并独立校验。",
                        )
                    })?;
                Ok(())
            })();
            if let Err(error) = published {
                result = Err(error);
            }
        }
        let (state, error) = match result {
            Ok(output) => {
                let v = &output.verification;
                match work {
                    Work::Create(prepared) => {
                        sqlx::query(
                            "INSERT INTO backup_archive (id,task_id,workspace_id,relative_dir,created_at,entry_count,file_count,total_bytes,consistency,verification,verified_at,archive_sha256,manifest_sha256,play_instance_id,source_root) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                        )
                        .bind(&v.backup_id)
                        .bind(task_id)
                        .bind(&prepared.workspace_id)
                        .bind(&v.backup_id)
                        .bind(&output.manifest.created_at)
                        .bind(i64::from(v.entry_count))
                        .bind(i64::from(v.file_count))
                        .bind(&v.total_bytes)
                        .bind(&v.consistency)
                        .bind("verified")
                        .bind(&v.verified_at)
                        .bind(&v.archive_sha256)
                        .bind(&v.manifest_sha256)
                        .bind(&prepared.plan.play_instance_id)
                        .bind(&prepared.plan.source_root)
                        .execute(&mut *tx)
                        .await
                        .map_err(HubError::db)?;
                    }
                    Work::Verify(record) => {
                        sqlx::query("UPDATE backup_archive SET verification='verified',verified_at=? WHERE id=?").bind(&v.verified_at).bind(&record.id).execute(&mut *tx).await.map_err(HubError::db)?;
                    }
                }
                ("succeeded", None)
            }
            Err(error) => {
                if let Work::Verify(record) = work
                    && error.code != "TASK_CANCELLED"
                {
                    let status = if ["HASH_MISMATCH", "ARCHIVE_INVALID"].contains(&error.code) {
                        "failed"
                    } else {
                        "unavailable"
                    };
                    sqlx::query("UPDATE backup_archive SET verification=? WHERE id=?")
                        .bind(status)
                        .bind(&record.id)
                        .execute(&mut *tx)
                        .await
                        .map_err(HubError::db)?;
                }
                (
                    if error.code == "TASK_CANCELLED" {
                        "cancelled"
                    } else if error.code == "PUBLICATION_UNCERTAIN" {
                        "recovery_required"
                    } else {
                        "failed"
                    },
                    Some(error.code),
                )
            }
        };
        sqlx::query("UPDATE backup_task SET state=?,phase='finished',error_code=?,finished_at=?,revision=revision+1 WHERE id=?")
            .bind(state).bind(error).bind(now()).bind(task_id).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task_id)
            .bind(if matches!(work, Work::Create(_)) {
                "backup.create"
            } else {
                "backup.verify"
            })
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    pub async fn cancel_backup(&self, task_id: &str) -> Result<BackupTask> {
        let _guard = self.mutation.lock().await;
        let task = self.backup_task(task_id).await?;
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return Ok(task);
        }
        let controls = self.scan_controls.lock().await;
        let flag = controls.get(task_id).ok_or(HubError::new(
            "TASK_STATE_UNKNOWN",
            "任务状态未知，请重启后检查。",
        ))?;
        sqlx::query("UPDATE backup_task SET state='cancelling',revision=revision+1 WHERE id=? AND state='running'").bind(task_id).execute(&self.pool).await.map_err(HubError::db)?;
        flag.store(true, Ordering::Release);
        self.backup_task(task_id).await
    }
    pub async fn backup_task(&self, task_id: &str) -> Result<BackupTask> {
        let query = format!("SELECT {COLUMNS} FROM backup_task WHERE id=?");
        let mut task = sqlx::query_as(&query)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("TASK_NOT_FOUND", "备份任务不存在。"))?;
        self.overlay_backup(&mut task).await;
        Ok(task)
    }
}
