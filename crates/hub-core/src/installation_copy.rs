//! Independent installation copies. No game execution, database import, or isolation promotion.
use crate::{
    HubError, Result, backup_format as format,
    database::{id, now},
    paths,
};
use cap_fs_ext::DirExt;
use cap_std::fs::{Dir, Metadata, OpenOptions};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
};

const MAX_ENTRIES: usize = 20_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_METADATA: usize = 8 * 1024 * 1024;
const MARKER: &str = "INSTALLATION_COPY_INCOMPLETE.json";
const MANIFEST: &str = "copy-manifest.json";
const RECEIPT: &str = "copy-verification.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationCopyEntry {
    pub relative_path: String,
    pub kind: String,
    pub size: String,
    pub sha256: Option<String>,
    pub link_target: Option<String>,
    pub link_is_directory: bool,
    pub unix_mode: Option<u32>,
    pub readonly: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_unix_nanos: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    copy_id: String,
    created_at: String,
    payload_root: String,
    executable_relative: String,
    executable_sha256: String,
    root_unix_mode: Option<u32>,
    root_readonly: bool,
    entries: Vec<InstallationCopyEntry>,
}
#[derive(Debug, Serialize)]
pub struct InstallationCopyPlan {
    pub source_root: String,
    pub destination: String,
    pub executable_relative: String,
    pub executable_sha256: String,
    pub total_bytes: String,
    pub entries: Vec<InstallationCopyEntry>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationCopyVerification {
    pub schema_version: u32,
    pub copy_id: String,
    pub payload_root: String,
    pub executable_relative: String,
    pub executable_sha256: String,
    pub manifest_sha256: String,
    pub file_count: u32,
    pub link_count: u32,
    pub total_bytes: String,
    pub verified_at: String,
    pub binding_verified: bool,
    pub isolation_verified: bool,
}
fn invalid() -> HubError {
    HubError::new(
        "INSTALLATION_COPY_INVALID",
        "安装副本清单、路径或内容不符合校验要求。",
    )
}
fn changed() -> HubError {
    HubError::new(
        "INSTALLATION_SOURCE_CHANGED",
        "安装源或目标目录在复制期间变化，未完成副本。",
    )
}
fn io(error: std::io::Error) -> HubError {
    match error.kind() {
        std::io::ErrorKind::AlreadyExists => {
            HubError::new("COPY_TARGET_EXISTS", "目标已存在，不会覆盖或合并。")
        }
        std::io::ErrorKind::StorageFull => HubError::new("DISK_FULL", "空间不足，未完成安装副本。"),
        _ => HubError::new(
            "INSTALLATION_COPY_IO_FAILED",
            "安装复制路径不可读写或操作未完成，请保留未完成目录。",
        ),
    }
}
fn modes(metadata: &Metadata) -> Result<(Option<u32>, bool)> {
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;
        let mode = metadata.permissions().mode();
        if mode & 0o7000 != 0 {
            return Err(HubError::new(
                "COPY_PERMISSION_UNSUPPORTED",
                "不复制 setuid、setgid 或 sticky 权限位。",
            ));
        }
        Ok((Some(mode & 0o777), metadata.permissions().readonly()))
    }
    #[cfg(not(unix))]
    {
        Ok((None, metadata.permissions().readonly()))
    }
}
fn set_modes(dir: &Dir, name: &str, mode: Option<u32>, _readonly: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;
        dir.set_permissions(
            name,
            cap_std::fs::Permissions::from_mode(mode.ok_or_else(invalid)?),
        )
        .map_err(io)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        let mut permissions = dir.symlink_metadata(name).map_err(io)?.permissions();
        permissions.set_readonly(_readonly);
        dir.set_permissions(name, permissions).map_err(io)
    }
}
fn timestamp(time: std::time::SystemTime) -> String {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos().to_string(),
        Err(error) => format!("-{}", error.duration().as_nanos()),
    }
}
fn parse_timestamp(text: &str) -> Result<std::time::SystemTime> {
    let nanos: i128 = text.parse().map_err(|_| invalid())?;
    if nanos.to_string() != text {
        return Err(invalid());
    }
    let magnitude = nanos.unsigned_abs();
    let seconds = u64::try_from(magnitude / 1_000_000_000).map_err(|_| invalid())?;
    let duration = std::time::Duration::new(seconds, (magnitude % 1_000_000_000) as u32);
    if nanos < 0 {
        std::time::UNIX_EPOCH.checked_sub(duration)
    } else {
        std::time::UNIX_EPOCH.checked_add(duration)
    }
    .ok_or_else(invalid)
}
fn timestamp_error() -> HubError {
    HubError::new(
        "COPY_TIMESTAMP_UNSUPPORTED",
        "无法精确保留普通文件修改时间，副本未完成；请保留未完成目录。",
    )
}
fn hash(dir: &Dir, path: &str, limit: u64, cancel: &AtomicBool) -> Result<(u64, String)> {
    format::hash_reader(&mut format::open_file(dir, path)?, limit, cancel)
}
fn safe_link(dir: &Dir, path: &str) -> Result<(String, bool)> {
    let (parent, name) = format::parent(dir, path)?;
    let target = parent.read_link_contents(name).map_err(io)?;
    if target.is_absolute()
        || target.as_os_str().is_empty()
        || target
            .components()
            .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
    {
        return Err(invalid());
    }
    let text = paths::text(&target)?;
    if text.contains(['\\', '\0', ':']) || text.chars().any(char::is_control) {
        return Err(invalid());
    }
    // Resolve using the root capability; external, dangling and cyclic links fail.
    let resolved = dir.canonicalize(path).map_err(|_| invalid())?;
    let metadata = dir.metadata(resolved).map_err(io)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(invalid());
    }
    Ok((text, metadata.is_dir()))
}
fn inventory(
    root: &Dir,
    independent: bool,
    cancel: &AtomicBool,
    preserve_timestamps: bool,
) -> Result<Vec<InstallationCopyEntry>> {
    fn walk(
        root: &Dir,
        dir: &Dir,
        prefix: &str,
        depth: usize,
        entries: &mut Vec<InstallationCopyEntry>,
        bytes: &mut u64,
        controls: (bool, &AtomicBool, bool),
    ) -> Result<()> {
        let (independent, cancel, preserve_timestamps) = controls;
        format::check_cancel(cancel)?;
        if depth > 32 {
            return Err(HubError::new(
                "COPY_LIMIT_EXCEEDED",
                "安装目录深度超过 32 层。",
            ));
        }
        for item in dir.entries().map_err(io)? {
            format::check_cancel(cancel)?;
            let item = item.map_err(io)?;
            let name = paths::text(Path::new(&item.file_name()))?;
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            format::validate_path(&path)?;
            if entries.len() >= MAX_ENTRIES {
                return Err(HubError::new(
                    "COPY_LIMIT_EXCEEDED",
                    "安装范围超过 20000 项。",
                ));
            }
            let metadata = dir.symlink_metadata(&name).map_err(io)?;
            let mut entry = InstallationCopyEntry {
                relative_path: path.clone(),
                kind: String::new(),
                size: "0".into(),
                sha256: None,
                link_target: None,
                link_is_directory: false,
                unix_mode: None,
                readonly: false,
                modified_unix_nanos: None,
            };
            if metadata.file_type().is_symlink() {
                let (target, is_dir) = safe_link(root, &path)?;
                entry.kind = "symlink".into();
                entry.link_target = Some(target);
                entry.link_is_directory = is_dir;
                entries.push(entry);
            } else if metadata.is_dir() {
                (entry.unix_mode, entry.readonly) = modes(&metadata)?;
                entry.kind = "directory".into();
                entries.push(entry);
                let child = dir.open_dir_nofollow(&name).map_err(io)?;
                walk(root, &child, &path, depth + 1, entries, bytes, controls)?;
            } else if metadata.is_file() {
                #[cfg(unix)]
                {
                    use cap_std::fs::MetadataExt;
                    if independent && metadata.nlink() != 1 {
                        return Err(invalid());
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = independent;
                }
                let remaining = MAX_BYTES.checked_sub(*bytes).ok_or_else(invalid)?;
                if metadata.len() > remaining {
                    return Err(HubError::new(
                        "COPY_LIMIT_EXCEEDED",
                        "安装复制总量超过 64 GiB。",
                    ));
                }
                let (size, digest) = hash(root, &path, remaining, cancel)?;
                if size != metadata.len() {
                    return Err(changed());
                }
                *bytes = bytes.checked_add(size).ok_or_else(invalid)?;
                if *bytes > MAX_BYTES {
                    return Err(HubError::new(
                        "COPY_LIMIT_EXCEEDED",
                        "安装复制总量超过 64 GiB。",
                    ));
                }
                (entry.unix_mode, entry.readonly) = modes(&metadata)?;
                entry.kind = "file".into();
                if preserve_timestamps {
                    entry.modified_unix_nanos = Some(timestamp(
                        metadata
                            .modified()
                            .map_err(|_| timestamp_error())?
                            .into_std(),
                    ));
                }
                entry.size = size.to_string();
                entry.sha256 = Some(digest);
                entries.push(entry);
            } else {
                return Err(HubError::new(
                    "COPY_FILE_TYPE_UNSUPPORTED",
                    "安装范围含特殊文件，不会忽略或跟随复制。",
                ));
            }
        }
        Ok(())
    }
    let mut entries = Vec::new();
    walk(
        root,
        root,
        "",
        0,
        &mut entries,
        &mut 0,
        (independent, cancel, preserve_timestamps),
    )?;
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    if serde_json::to_vec(&entries).map_err(|_| invalid())?.len() > MAX_METADATA / 2 {
        return Err(invalid());
    }
    Ok(entries)
}
pub(crate) struct Prepared {
    root: Dir,
    parent: Dir,
    source: PathBuf,
    parent_path: PathBuf,
    leaf: String,
    manifest: Manifest,
}
pub(crate) fn prepare(source: &Path, executable: &str, destination: &Path) -> Result<Prepared> {
    format::validate_path(executable)?;
    let source = std::fs::canonicalize(source).map_err(io)?;
    let root = Dir::open_ambient_dir(&source, cap_std::ambient_authority()).map_err(io)?;
    let parent_path =
        std::fs::canonicalize(destination.parent().ok_or_else(invalid)?).map_err(io)?;
    let leaf = paths::text(Path::new(destination.file_name().ok_or_else(invalid)?))?;
    format::validate_path(&leaf)?;
    if leaf.contains('/') {
        return Err(invalid());
    }
    let parent = Dir::open_ambient_dir(&parent_path, cap_std::ambient_authority()).map_err(io)?;
    match parent.symlink_metadata(&leaf) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Ok(_) => {
            return Err(HubError::new(
                "COPY_TARGET_EXISTS",
                "目标已存在，不会覆盖或合并。",
            ));
        }
        Err(e) => return Err(io(e)),
    }
    let source_identity =
        same_file::Handle::from_file(root.try_clone().map_err(io)?.into_std_file()).map_err(io)?;
    for ancestor in parent_path.ancestors() {
        if source_identity == same_file::Handle::from_path(ancestor).map_err(io)? {
            return Err(HubError::new(
                "COPY_SCOPE_OVERLAP",
                "安装副本目标须在源目录以外。",
            ));
        }
    }
    let name = paths::text(Path::new(source.file_name().ok_or_else(invalid)?))?;
    format::validate_path(&name)?;
    let (root_unix_mode, root_readonly) = modes(&root.dir_metadata().map_err(io)?)?;
    let entries = inventory(&root, false, &AtomicBool::new(false), true)?;
    let executable_entry = entries
        .iter()
        .find(|e| e.relative_path == executable && e.kind == "file")
        .ok_or_else(invalid)?;
    paths::executable(&source.join(executable))?;
    let executable_sha256 = executable_entry.sha256.clone().ok_or_else(invalid)?;
    let manifest = Manifest {
        schema_version: 2,
        copy_id: id(),
        created_at: now(),
        payload_root: format!("payload/{name}"),
        executable_relative: executable.into(),
        executable_sha256,
        root_unix_mode,
        root_readonly,
        entries,
    };
    Ok(Prepared {
        root,
        parent,
        source,
        parent_path,
        leaf,
        manifest,
    })
}
fn total(entries: &[InstallationCopyEntry]) -> Result<u64> {
    entries.iter().try_fold(0_u64, |sum, e| {
        sum.checked_add(e.size.parse().map_err(|_| invalid())?)
            .ok_or_else(invalid)
    })
}
pub fn plan_installation_copy(
    source: &Path,
    executable: &str,
    destination: &Path,
) -> Result<InstallationCopyPlan> {
    let p = prepare(source, executable, destination)?;
    p.preview()
}
impl Prepared {
    pub(crate) fn identity(&self) -> Result<(String, String, String)> {
        Ok((
            self.manifest.copy_id.clone(),
            self.manifest.payload_root.clone(),
            format::sha(&serde_json::to_vec_pretty(&self.manifest).map_err(|_| invalid())?),
        ))
    }
    pub(crate) fn preview(&self) -> Result<InstallationCopyPlan> {
        let p = self;
        Ok(InstallationCopyPlan {
            source_root: paths::text(&p.source)?,
            destination: paths::text(&p.parent_path.join(&p.leaf))?,
            executable_relative: p.manifest.executable_relative.clone(),
            executable_sha256: p.manifest.executable_sha256.clone(),
            total_bytes: total(&p.manifest.entries)?.to_string(),
            entries: p.manifest.entries.clone(),
            warnings: vec![
                "仅生成只读范围预览；确认执行时会重新检查源内容。".into(),
                "副本不证明完整写入隔离、签名有效、游戏兼容性或可安全并发。".into(),
                "不复制 ACL、扩展属性或所有者；不会执行、重新签名或自动登记游戏。".into(),
                "普通文件修改时间将精确保留并校验；这不证明已有缓存兼容，不保留目录时间、创建时间或扩展属性。".into(),
                "不重写配置中的绝对路径；仍须建立独立玩家目录与启动环境。".into(),
            ],
        })
    }
}

fn write_json(root: &Dir, name: &str, value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid())?;
    if bytes.len() > MAX_METADATA {
        return Err(invalid());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = root.open_with(name, &options).map_err(io)?;
    file.write_all(&bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    Ok(bytes)
}
fn sync_dir(dir: &Dir) -> Result<()> {
    #[cfg(unix)]
    {
        dir.try_clone()
            .map_err(io)?
            .into_std_file()
            .sync_all()
            .map_err(io)?;
    }
    Ok(())
}
fn same_dir(dir: &Dir, path: &Path) -> Result<bool> {
    Ok(
        same_file::Handle::from_file(dir.try_clone().map_err(io)?.into_std_file()).map_err(io)?
            == same_file::Handle::from_path(path).map_err(io)?,
    )
}
fn result(manifest: &Manifest, digest: String) -> Result<InstallationCopyVerification> {
    Ok(InstallationCopyVerification {
        schema_version: manifest.schema_version,
        copy_id: manifest.copy_id.clone(),
        payload_root: manifest.payload_root.clone(),
        executable_relative: manifest.executable_relative.clone(),
        executable_sha256: manifest.executable_sha256.clone(),
        manifest_sha256: digest,
        file_count: manifest.entries.iter().filter(|e| e.kind == "file").count() as u32,
        link_count: manifest
            .entries
            .iter()
            .filter(|e| e.kind == "symlink")
            .count() as u32,
        total_bytes: total(&manifest.entries)?.to_string(),
        verified_at: now(),
        binding_verified: false,
        isolation_verified: false,
    })
}
pub fn copy_installation_to_new_directory(
    source: &Path,
    executable: &str,
    destination: &Path,
) -> Result<InstallationCopyVerification> {
    copy_with_hook(source, executable, destination, |_| Ok(()))
}
pub(crate) struct CopyControl {
    pub cancel: AtomicBool,
    pub processed: AtomicU64,
    pub phase: AtomicU8,
    pub published: AtomicBool,
}
impl CopyControl {
    pub(crate) fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            processed: AtomicU64::new(0),
            phase: AtomicU8::new(0),
            published: AtomicBool::new(false),
        }
    }
}
fn copy_with_hook(
    source: &Path,
    executable: &str,
    destination: &Path,
    before_commit: impl FnOnce(&Dir) -> Result<()>,
) -> Result<InstallationCopyVerification> {
    let p = prepare(source, executable, destination)?;
    execute_prepared(&p, &CopyControl::new(), before_commit)
}
pub(crate) fn execute_prepared<T>(
    p: &Prepared,
    control: &CopyControl,
    before_commit: impl FnOnce(&Dir) -> Result<T>,
) -> Result<InstallationCopyVerification> {
    format::check_cancel(&control.cancel)?;
    control.phase.store(1, Ordering::Release);
    let required = total(&p.manifest.entries)?
        .checked_add(16 * 1024 * 1024)
        .ok_or_else(invalid)?;
    if fs2::available_space(&p.parent_path).map_err(io)? < required {
        return Err(HubError::new("DISK_FULL", "空间不足以保存完整安装副本。"));
    }
    if !same_dir(&p.parent, &p.parent_path)? || !same_dir(&p.root, &p.source)? {
        return Err(changed());
    }
    p.parent.create_dir(&p.leaf).map_err(io)?;
    let container = p.parent.open_dir_nofollow(&p.leaf).map_err(io)?;
    #[cfg(unix)]
    {
        use cap_std::fs::PermissionsExt;
        p.parent
            .set_permissions(&p.leaf, cap_std::fs::Permissions::from_mode(0o700))
            .map_err(io)?;
    }
    write_json(
        &container,
        MARKER,
        &serde_json::json!({"schema_version":p.manifest.schema_version,"copy_id":p.manifest.copy_id,"state":"incomplete"}),
    )?;
    sync_dir(&container)?;
    sync_dir(&p.parent)?;
    container.create_dir("payload").map_err(io)?;
    let (payload_parent, name) = format::parent(&container, &p.manifest.payload_root)?;
    payload_parent.create_dir(&name).map_err(io)?;
    let target = payload_parent.open_dir_nofollow(&name).map_err(io)?;
    for entry in &p.manifest.entries {
        format::check_cancel(&control.cancel)?;
        let (parent, name) = format::parent(&target, &entry.relative_path)?;
        match entry.kind.as_str() {
            "directory" => parent.create_dir(&name).map_err(io)?,
            "file" => {
                let mut source = format::open_file(&p.root, &entry.relative_path)?;
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                let mut dest = parent.open_with(&name, &options).map_err(io)?;
                let (size, digest) = format::transfer_hashed(
                    &mut source,
                    &mut dest,
                    entry.size.parse().map_err(|_| invalid())?,
                    &control.cancel,
                )?;
                if size.to_string() != entry.size || Some(digest) != entry.sha256 {
                    return Err(changed());
                }
                let a = same_file::Handle::from_file(source.into_std()).map_err(io)?;
                let b = same_file::Handle::from_file(dest.try_clone().map_err(io)?.into_std())
                    .map_err(io)?;
                if a == b {
                    return Err(invalid());
                }
                let dest = dest.into_std();
                let modified =
                    parse_timestamp(entry.modified_unix_nanos.as_deref().ok_or_else(invalid)?)?;
                dest.set_times(std::fs::FileTimes::new().set_modified(modified))
                    .map_err(|_| timestamp_error())?;
                if dest
                    .metadata()
                    .map_err(io)?
                    .modified()
                    .map_err(|_| timestamp_error())?
                    != modified
                {
                    return Err(timestamp_error());
                }
                set_modes(&parent, &name, entry.unix_mode, entry.readonly)?;
                dest.sync_all().map_err(io)?;
            }
            // Create links only after all ordinary directories and files exist.
            "symlink" => (),
            _ => return Err(invalid()),
        }
        if entry.kind != "symlink" {
            control.processed.fetch_add(1, Ordering::Release);
        }
    }
    for entry in p.manifest.entries.iter().filter(|e| e.kind == "symlink") {
        let (parent, name) = format::parent(&target, &entry.relative_path)?;
        format::check_cancel(&control.cancel)?;
        let link = entry.link_target.as_ref().ok_or_else(invalid)?;
        #[cfg(not(windows))]
        parent.symlink(link, &name).map_err(io)?;
        #[cfg(windows)]
        {
            if entry.link_is_directory {
                parent.symlink_dir(link, &name)
            } else {
                parent.symlink_file(link, &name)
            }
            .map_err(io)?;
        }
        control.processed.fetch_add(1, Ordering::Release);
    }
    for entry in p
        .manifest
        .entries
        .iter()
        .rev()
        .filter(|e| e.kind == "directory")
    {
        format::check_cancel(&control.cancel)?;
        let (parent, name) = format::parent(&target, &entry.relative_path)?;
        set_modes(&parent, &name, entry.unix_mode, entry.readonly)?;
    }
    set_modes(
        &payload_parent,
        &name,
        p.manifest.root_unix_mode,
        p.manifest.root_readonly,
    )?;
    control.phase.store(2, Ordering::Release);
    let source_entries = inventory(&p.root, false, &control.cancel, true)?;
    control.phase.store(3, Ordering::Release);
    let target_entries = inventory(&target, true, &control.cancel, true)?;
    if modes(&p.root.dir_metadata().map_err(io)?)?
        != (p.manifest.root_unix_mode, p.manifest.root_readonly)
        || modes(&target.dir_metadata().map_err(io)?)?
            != (p.manifest.root_unix_mode, p.manifest.root_readonly)
        || !same_dir(
            &target,
            &p.parent_path.join(&p.leaf).join(&p.manifest.payload_root),
        )?
        || source_entries != p.manifest.entries
        || target_entries != p.manifest.entries
        || !same_dir(&p.parent, &p.parent_path)?
        || !same_dir(&p.root, &p.source)?
        || !same_dir(&container, &p.parent_path.join(&p.leaf))?
    {
        return Err(changed());
    }
    control.phase.store(4, Ordering::Release);
    let _commit_guard = before_commit(&container)?;
    format::check_cancel(&control.cancel)?;
    let manifest_bytes = write_json(&container, MANIFEST, &p.manifest)?;
    let verified = result(&p.manifest, format::sha(&manifest_bytes))?;
    write_json(&container, RECEIPT, &verified)?;
    sync_tree(&target, &p.manifest.entries)?;
    sync_dir(&payload_parent)?;
    sync_dir(&container)?;
    container.remove_file(MARKER).map_err(io)?;
    sync_dir(&container)?;
    sync_dir(&p.parent)?;
    control.published.store(true, Ordering::Release);
    Ok(verified)
}
fn sync_tree(root: &Dir, entries: &[InstallationCopyEntry]) -> Result<()> {
    for entry in entries.iter().rev().filter(|e| e.kind == "directory") {
        let (parent, name) = format::parent(root, &entry.relative_path)?;
        sync_dir(&parent.open_dir_nofollow(name).map_err(io)?)?;
    }
    sync_dir(root)
}
fn read_json<T: serde::de::DeserializeOwned>(root: &Dir, path: &str) -> Result<(T, Vec<u8>)> {
    use std::io::Read;
    let mut bytes = Vec::new();
    format::open_file(root, path)?
        .take(MAX_METADATA as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > MAX_METADATA {
        return Err(invalid());
    }
    Ok((
        serde_json::from_slice(&bytes).map_err(|_| invalid())?,
        bytes,
    ))
}
// The control directories have fixed cardinality. Do not enumerate an untrusted
// directory to completion when the first extra entry already proves it invalid.
fn bounded_names(
    entries: impl Iterator<Item = Result<std::ffi::OsString>>,
    limit: usize,
) -> Result<Vec<std::ffi::OsString>> {
    let mut names = Vec::with_capacity(limit);
    for entry in entries {
        if names.len() == limit {
            return Err(invalid());
        }
        names.push(entry?);
    }
    Ok(names)
}
pub fn verify_installation_copy(container: &Path) -> Result<InstallationCopyVerification> {
    let root = Dir::open_ambient_dir(container, cap_std::ambient_authority()).map_err(io)?;
    match root.symlink_metadata(MARKER) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        _ => {
            return Err(HubError::new(
                "INSTALLATION_COPY_INCOMPLETE",
                "副本保留未完成标记，不能作为已完成副本。",
            ));
        }
    }
    let (manifest, bytes): (Manifest, _) = read_json(&root, MANIFEST)?;
    if ![1, 2].contains(&manifest.schema_version)
        || uuid::Uuid::parse_str(&manifest.copy_id).is_err()
        || manifest.entries.len() > MAX_ENTRIES
        || !format::valid_sha(&manifest.executable_sha256)
        || !manifest.payload_root.starts_with("payload/")
        || manifest.payload_root.split('/').count() != 2
    {
        return Err(invalid());
    }
    format::validate_path(&manifest.executable_relative)?;
    let mut controls = bounded_names(
        root.entries()
            .map_err(io)?
            .map(|e| e.map(|e| e.file_name()).map_err(io)),
        3,
    )?;
    controls.sort();
    let mut expected_controls = [MANIFEST, RECEIPT, "payload"]
        .map(std::ffi::OsString::from)
        .to_vec();
    expected_controls.sort();
    if controls != expected_controls {
        return Err(invalid());
    }
    let (parent, name) = format::parent(&root, &manifest.payload_root)?;
    let children = bounded_names(
        parent
            .entries()
            .map_err(io)?
            .map(|e| e.map(|e| e.file_name()).map_err(io)),
        1,
    )?;
    if children.len() != 1 || children[0] != std::ffi::OsStr::new(&name) {
        return Err(invalid());
    }
    let payload = parent.open_dir_nofollow(name).map_err(io)?;
    if modes(&payload.dir_metadata().map_err(io)?)?
        != (manifest.root_unix_mode, manifest.root_readonly)
        || inventory(
            &payload,
            true,
            &AtomicBool::new(false),
            manifest.schema_version == 2,
        )? != manifest.entries
    {
        return Err(invalid());
    }
    let executable = manifest
        .entries
        .iter()
        .find(|e| e.relative_path == manifest.executable_relative && e.kind == "file")
        .ok_or_else(invalid)?;
    if executable.sha256.as_deref() != Some(&manifest.executable_sha256) {
        return Err(invalid());
    }
    let (receipt, _): (InstallationCopyVerification, _) = read_json(&root, RECEIPT)?;
    let expected = result(&manifest, format::sha(&bytes))?;
    if receipt.schema_version != manifest.schema_version
        || receipt.copy_id != expected.copy_id
        || receipt.payload_root != expected.payload_root
        || receipt.executable_relative != expected.executable_relative
        || receipt.executable_sha256 != expected.executable_sha256
        || receipt.manifest_sha256 != expected.manifest_sha256
        || receipt.file_count != expected.file_count
        || receipt.link_count != expected.link_count
        || receipt.total_bytes != expected.total_bytes
        || receipt.binding_verified
        || receipt.isolation_verified
    {
        return Err(invalid());
    }
    Ok(expected)
}

#[cfg(test)]
mod interruption_tests {
    use super::*;
    const CHILD: &str = "installation_copy::interruption_tests::copy_child_at_completion_boundary";
    #[test]
    fn copy_child_at_completion_boundary() {
        let Some(root) = std::env::var_os("HUB_COPY_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        copy_with_hook(
            &root.join("source"),
            "game.exe",
            &root.join("target"),
            |_| {
                std::fs::write(root.join("child-ready"), b"before completion").unwrap();
                loop {
                    std::thread::park();
                }
            },
        )
        .unwrap();
    }
    #[test]
    fn killing_before_completion_retains_incomplete_payload_and_never_claims_success() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("source")).unwrap();
        std::fs::write(temp.path().join("source/game.exe"), b"synthetic executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                temp.path().join("source/game.exe"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(CHILD)
            .arg("--nocapture")
            .env("HUB_COPY_TEST_ROOT", temp.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !temp.path().join("child-ready").exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("child exited before completion boundary: {status}");
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("copy boundary not reached");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let target = temp.path().join("target");
        assert!(target.join(MARKER).exists());
        assert!(!target.join(RECEIPT).exists());
        assert_eq!(
            verify_installation_copy(&target).unwrap_err().code,
            "INSTALLATION_COPY_INCOMPLETE"
        );
        assert_eq!(
            copy_installation_to_new_directory(&temp.path().join("source"), "game.exe", &target)
                .unwrap_err()
                .code,
            "COPY_TARGET_EXISTS"
        );
        assert_eq!(
            std::fs::read(temp.path().join("source/game.exe")).unwrap(),
            b"synthetic executable"
        );
    }
    #[test]
    fn operation_failure_at_completion_boundary_leaves_no_verified_copy() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("game.exe"), b"synthetic executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                source.join("game.exe"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        // Inject an operation failure at the publication boundary; no production fault switch.
        assert!(copy_with_hook(&source, "game.exe", &target, |_| Err(changed())).is_err());
        assert!(target.join(MARKER).exists());
        assert!(!target.join(RECEIPT).exists());
        assert!(verify_installation_copy(&target).is_err());
    }
}

#[cfg(test)]
mod control_directory_tests {
    use super::*;
    #[test]
    fn reject_controls_after_first_excess_entry_without_reading_the_rest() {
        for limit in [3, 1] {
            let entries = (0..=limit)
                .map(|n| Ok(std::ffi::OsString::from(n.to_string())))
                .chain(std::iter::once_with(|| {
                    panic!("must not enumerate the remaining directory")
                }));
            assert_eq!(
                bounded_names(entries, limit).unwrap_err().code,
                "INSTALLATION_COPY_INVALID"
            );
        }
    }
}
