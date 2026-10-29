use crate::{BackupEntry, HubError, Result, database::now};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, File, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub(crate) const MAX_MANIFEST: u64 = 8 * 1024 * 1024;
pub(crate) const MAX_ENTRIES: usize = 20_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub schema_version: u32,
    pub backup_id: String,
    pub created_at: String,
    pub scope: String,
    pub world_id: Option<String>,
    pub world_alias: Option<String>,
    pub source_token: String,
    pub consistency: String,
    pub game: Option<serde_json::Value>,
    pub profile_id: Option<String>,
    pub adapter_version: Option<String>,
    pub tool_version: String,
    pub compression: String,
    pub archive_format: String,
    pub characters: Vec<serde_json::Value>,
    pub entry_count: u32,
    pub file_count: u32,
    pub total_bytes: String,
    pub entries: Vec<BackupEntry>,
    pub extensions: serde_json::Value,
    pub compression_version: String,
    pub world_contexts: Vec<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    pub schema_version: u32,
    pub backup_id: String,
    pub archive_file: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub verification: String,
    pub verified_at: String,
    pub verifier_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct BackupVerification {
    pub backup_id: String,
    pub file_count: u32,
    pub entry_count: u32,
    pub total_bytes: String,
    pub consistency: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub verified_at: String,
    pub receipt_present: bool,
}
pub(crate) fn io_error(error: std::io::Error) -> HubError {
    match error.kind() {
        std::io::ErrorKind::StorageFull => {
            HubError::new("DISK_FULL", "磁盘空间不足，未发布有效备份。")
        }
        std::io::ErrorKind::PermissionDenied => {
            HubError::new("PERMISSION_DENIED", "文件权限不足，操作已停止。")
        }
        _ => HubError::new("BACKUP_IO_FAILED", "备份文件不可读写、已变化或磁盘不可用。"),
    }
}
pub(crate) fn invalid() -> HubError {
    HubError::new(
        "ARCHIVE_INVALID",
        "归档或清单无效，不能作为完整性已验证的备份。",
    )
}
pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        return Err(HubError::new(
            "TASK_CANCELLED",
            "任务已取消；临时目录不会显示为有效备份。",
        ));
    }
    Ok(())
}
pub(crate) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.contains(['\\', '\0', ':'])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid());
    }
    Ok(())
}
pub(crate) fn parent(root: &Dir, path: &str) -> Result<(Dir, String)> {
    validate_path(path)?;
    let mut parts = path.split('/').collect::<Vec<_>>();
    let name = parts.pop().ok_or_else(invalid)?.to_owned();
    let mut directory = root.try_clone().map_err(io_error)?;
    for part in parts {
        directory = directory.open_dir_nofollow(part).map_err(io_error)?;
    }
    Ok((directory, name))
}
pub(crate) fn open_file(root: &Dir, path: &str) -> Result<File> {
    let (directory, name) = parent(root, path)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No).nonblock(true);
    let file = directory.open_with(name, &options).map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(invalid());
    }
    Ok(file)
}
pub(crate) fn hash_reader(
    reader: &mut impl Read,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<(u64, String)> {
    transfer_hashed(reader, &mut std::io::sink(), limit, cancel)
}
pub(crate) fn transfer_hashed(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<(u64, String)> {
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = reader.read(&mut buffer).map_err(io_error)?;
        if n == 0 {
            break;
        }
        count = count.checked_add(n as u64).ok_or_else(invalid)?;
        if count > limit {
            return Err(HubError::new(
                "LIMIT_EXCEEDED",
                "读取数据超过安全限制，已停止。",
            ));
        }
        writer.write_all(&buffer[..n]).map_err(io_error)?;
        hash.update(&buffer[..n]);
    }
    Ok((count, format!("{:x}", hash.finalize())))
}
fn bounded_read(reader: impl Read, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    reader
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > maximum {
        return Err(HubError::new("LIMIT_EXCEEDED", "元数据超过安全限制。"));
    }
    Ok(bytes)
}
pub(crate) fn valid_instance_provenance(instance: &str, installation: &str, digest: &str) -> bool {
    uuid::Uuid::parse_str(instance).is_ok()
        && uuid::Uuid::parse_str(installation).is_ok()
        && valid_sha(digest)
}
fn validate_manifest(manifest: &Manifest) -> Result<BTreeMap<String, BackupEntry>> {
    if manifest.schema_version == 2 {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct InstanceSource {
            instance_id: String,
            installation_id: String,
            installation_sha256: String,
            binding_verified: bool,
        }
        let context: InstanceSource = serde_json::from_value(
            manifest
                .extensions
                .get("instance_source")
                .cloned()
                .ok_or_else(invalid)?,
        )
        .map_err(|_| invalid())?;
        if manifest.consistency != "best_effort"
            || manifest.game.is_some()
            || manifest.profile_id.is_some()
            || manifest.adapter_version.is_some()
            || manifest.world_alias.is_some()
            || !manifest.characters.is_empty()
            || !manifest.world_contexts.is_empty()
            || context.instance_id != manifest.source_token
            || !valid_instance_provenance(
                &context.instance_id,
                &context.installation_id,
                &context.installation_sha256,
            )
            || context.binding_verified
            || manifest
                .extensions
                .get("boundary_verified")
                .and_then(|v| v.as_bool())
                != Some(false)
        {
            return Err(invalid());
        }
    }
    if !matches!(
        (manifest.schema_version, manifest.scope.as_str()),
        (1, "save_root") | (2, "instance_data_root")
    ) || manifest.world_id.is_some()
        || manifest.compression != "zstd"
        || manifest.archive_format != "tar"
        || manifest.entries.len() > MAX_ENTRIES
        || uuid::Uuid::parse_str(&manifest.backup_id).is_err()
        || manifest.entry_count as usize != manifest.entries.len()
        || !["best_effort", "quiescent_checked", "unknown"].contains(&manifest.consistency.as_str())
    {
        return Err(invalid());
    }
    let mut expected = BTreeMap::new();
    let mut bytes = 0_u64;
    let mut files = 0_u32;
    for entry in &manifest.entries {
        validate_path(&entry.relative_path)?;
        if entry
            .mtime
            .as_ref()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err())
        {
            return Err(invalid());
        }
        let size: u64 = entry.size.parse().map_err(|_| invalid())?;
        if size.to_string() != entry.size {
            return Err(invalid());
        }
        match entry.file_type.as_str() {
            "file" if entry.sha256.as_deref().is_some_and(valid_sha) => {
                files += 1;
                bytes = bytes.checked_add(size).ok_or_else(invalid)?;
            }
            "directory" if size == 0 && entry.sha256.is_none() => {}
            _ => return Err(invalid()),
        }
        if expected
            .insert(entry.relative_path.clone(), entry.clone())
            .is_some()
        {
            return Err(invalid());
        }
    }
    if bytes > MAX_BYTES
        || bytes.to_string() != manifest.total_bytes
        || files != manifest.file_count
    {
        return Err(invalid());
    }
    for path in expected.keys() {
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if expected
                .get(parent)
                .is_none_or(|entry| entry.file_type != "directory")
            {
                return Err(invalid());
            }
            ancestor = parent;
        }
    }
    Ok(expected)
}

pub(crate) fn verify_dir(
    root: &Dir,
    expected_archive: Option<&str>,
    cancel: &AtomicBool,
) -> Result<(Manifest, BackupVerification)> {
    inspect_archive(root, expected_archive, cancel, None)
}
/// The only payload parser: optional extraction is confined to a caller-owned new directory.
pub(crate) fn inspect_archive(
    root: &Dir,
    expected_archive: Option<&str>,
    cancel: &AtomicBool,
    destination: Option<&Dir>,
) -> Result<(Manifest, BackupVerification)> {
    inspect_archive_inner(root, expected_archive, cancel, destination, None)
}
pub(crate) fn read_verified_config(
    root: &Dir,
    expected_archive: &str,
    path: &str,
) -> Result<(Manifest, BackupVerification, Vec<u8>)> {
    if !matches!(
        path,
        "config/options.json" | "config/user-default-mods.json"
    ) {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    let (manifest, verification) = inspect_archive_inner(
        root,
        Some(expected_archive),
        &AtomicBool::new(false),
        None,
        Some((path, &mut bytes)),
    )?;
    if !manifest
        .entries
        .iter()
        .any(|e| e.relative_path == path && e.file_type == "file")
    {
        return Err(invalid());
    }
    Ok((manifest, verification, bytes))
}
fn inspect_archive_inner(
    root: &Dir,
    expected_archive: Option<&str>,
    cancel: &AtomicBool,
    destination: Option<&Dir>,
    mut capture: Option<(&str, &mut Vec<u8>)>,
) -> Result<(Manifest, BackupVerification)> {
    let receipt = match root.symlink_metadata("receipt.json") {
        Ok(_) => {
            let bytes = bounded_read(open_file(root, "receipt.json")?, 64 * 1024)?;
            let receipt: Receipt = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            if receipt.schema_version != 1
                || receipt.archive_file != "archive.tar.zst"
                || receipt.verification != "verified"
                || !valid_sha(&receipt.archive_sha256)
                || !valid_sha(&receipt.manifest_sha256)
            {
                return Err(invalid());
            }
            Some(receipt)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error(error)),
    };
    let mut file = open_file(root, "archive.tar.zst")?;
    let (_, archive_hash) = hash_reader(&mut file, MAX_BYTES + 128 * 1024 * 1024, cancel)?;
    if receipt
        .as_ref()
        .is_some_and(|receipt| archive_hash != receipt.archive_sha256)
        || expected_archive.is_some_and(|expected| expected != archive_hash)
    {
        return Err(HubError::new(
            "HASH_MISMATCH",
            "归档摘要不匹配，完整性校验失败。",
        ));
    }
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    let mut decoder = zstd::stream::read::Decoder::new(&mut file).map_err(io_error)?;
    decoder.window_log_max(27).map_err(io_error)?;
    let mut archive = tar::Archive::new(decoder.take(MAX_BYTES + 128 * 1024 * 1024));
    let mut entries = archive.entries().map_err(io_error)?.raw(true);
    let mut manifest_entry = entries.next().ok_or_else(invalid)?.map_err(io_error)?;
    if manifest_entry.path_bytes().as_ref() != b"manifest.json"
        || !manifest_entry.header().entry_type().is_file()
        || manifest_entry.size() > MAX_MANIFEST
    {
        return Err(invalid());
    }
    let manifest_bytes = bounded_read(&mut manifest_entry, MAX_MANIFEST)?;
    let manifest_hash = sha(&manifest_bytes);
    if receipt
        .as_ref()
        .is_some_and(|receipt| manifest_hash != receipt.manifest_sha256)
    {
        return Err(HubError::new("HASH_MISMATCH", "清单摘要不匹配。"));
    }
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|_| invalid())?;
    if receipt
        .as_ref()
        .is_some_and(|receipt| manifest.backup_id != receipt.backup_id)
    {
        return Err(invalid());
    }
    let expected = validate_manifest(&manifest)?;
    if let Some(destination) = destination {
        // Lexical ordering puts every explicit directory ancestor before its descendants.
        for planned in expected
            .values()
            .filter(|entry| entry.file_type == "directory")
        {
            check_cancel(cancel)?;
            let (directory, name) = parent(destination, &planned.relative_path)?;
            directory.create_dir(name).map_err(io_error)?;
        }
    }
    let mut seen = BTreeSet::new();
    let mut long_name: Option<String> = None;
    let mut raw_count = 0_usize;
    for entry in entries {
        check_cancel(cancel)?;
        let mut entry = entry.map_err(io_error)?;
        raw_count += 1;
        if raw_count > MAX_ENTRIES * 2 {
            return Err(invalid());
        }
        let kind = entry.header().entry_type();
        if kind == tar::EntryType::GNULongName {
            if long_name.is_some() || entry.size() > 4102 {
                return Err(invalid());
            }
            let bytes = bounded_read(&mut entry, 4102)?;
            if bytes.last() != Some(&0) {
                return Err(invalid());
            }
            long_name = Some(
                std::str::from_utf8(&bytes[..bytes.len() - 1])
                    .map_err(|_| invalid())?
                    .to_owned(),
            );
            continue;
        }
        let path = match long_name.take() {
            Some(path) => path,
            None => std::str::from_utf8(entry.path_bytes().as_ref())
                .map_err(|_| invalid())?
                .to_owned(),
        };
        let relative = path.strip_prefix("data/").ok_or_else(invalid)?;
        let relative = if kind.is_dir() {
            relative.strip_suffix('/').unwrap_or(relative)
        } else {
            relative
        };
        validate_path(relative)?;
        let planned = expected.get(relative).ok_or_else(invalid)?;
        if !seen.insert(relative.to_owned()) {
            return Err(invalid());
        }
        let size: u64 = planned.size.parse().map_err(|_| invalid())?;
        if planned.file_type == "directory" {
            if !kind.is_dir() || entry.size() != 0 {
                return Err(invalid());
            }
        } else {
            if !kind.is_file() || entry.size() != size {
                return Err(invalid());
            }
            let (actual_size, hash) = if let Some(destination) = destination {
                let (directory, name) = parent(destination, relative)?;
                let mut options = OpenOptions::new();
                options
                    .write(true)
                    .create_new(true)
                    .follow(FollowSymlinks::No);
                let mut output = directory.open_with(name, &options).map_err(io_error)?;
                let result = transfer_hashed(&mut entry, &mut output, size, cancel)?;
                output.sync_all().map_err(io_error)?;
                #[cfg(test)]
                crate::restore_faults::checkpoint("after_payload_file");
                result
            } else if let Some((wanted, output)) =
                capture.as_mut().filter(|(wanted, _)| *wanted == relative)
            {
                let _ = wanted;
                if size > 1024 * 1024 {
                    return Err(invalid());
                }
                transfer_hashed(&mut entry, *output, size, cancel)?
            } else {
                hash_reader(&mut entry, size, cancel)?
            };
            if actual_size != size || planned.sha256.as_deref() != Some(&hash) {
                return Err(HubError::new("HASH_MISMATCH", "归档内文件摘要不匹配。"));
            }
        }
    }
    if long_name.is_some() || seen.len() != expected.len() {
        return Err(invalid());
    }
    // Drain the decoder to validate frame termination/checksums, and reject data after TAR end.
    let mut remainder = archive.into_inner();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = remainder.read(&mut buffer).map_err(io_error)?;
        if n == 0 {
            break;
        }
        if buffer[..n].iter().any(|byte| *byte != 0) {
            return Err(invalid());
        }
    }
    if remainder.limit() == 0 {
        return Err(invalid());
    }
    drop(remainder);
    file.seek(SeekFrom::Start(0)).map_err(io_error)?;
    if hash_reader(&mut file, MAX_BYTES + 128 * 1024 * 1024, cancel)?.1 != archive_hash {
        return Err(HubError::new("SOURCE_CHANGED", "校验期间归档发生变化。"));
    }
    let result = BackupVerification {
        backup_id: manifest.backup_id.clone(),
        file_count: manifest.file_count,
        entry_count: manifest.entry_count,
        total_bytes: manifest.total_bytes.clone(),
        consistency: manifest.consistency.clone(),
        archive_sha256: archive_hash,
        manifest_sha256: manifest_hash,
        verified_at: now(),
        receipt_present: receipt.is_some(),
    };
    Ok((manifest, result))
}
/// Verify a standalone backup without the application's database or Git repository.
/// This validates bytes and the v1 envelope; it does not establish provenance or game compatibility.
pub fn verify_backup_directory(path: &Path) -> Result<BackupVerification> {
    let root = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(io_error)?;
    verify_dir(&root, None, &AtomicBool::new(false)).map(|(_, result)| result)
}

pub(crate) fn sync_directory(directory: &Dir) -> Result<()> {
    #[cfg(unix)]
    directory
        .try_clone()
        .map_err(io_error)?
        .into_std_file()
        .sync_all()
        .map_err(io_error)?;
    #[cfg(not(unix))]
    let _ = directory;
    Ok(())
}

pub(crate) fn source_failure_status(code: &str) -> Option<&'static str> {
    match code {
        "HASH_MISMATCH" | "ARCHIVE_INVALID" | "SOURCE_CHANGED" => Some("failed"),
        "BACKUP_IO_FAILED" | "PERMISSION_DENIED" | "PATH_INVALID" | "PATH_CHANGED" => {
            Some("unavailable")
        }
        _ => None,
    }
}

#[cfg(test)]
mod instance_manifest_tests {
    use super::*;
    #[test]
    fn instance_context_is_strict_and_cannot_promote_binding_or_relabel_a_scope() {
        let instance = crate::database::id();
        let installation = crate::database::id();
        let value = serde_json::json!({
            "schema_version":2,"backup_id":crate::database::id(),"created_at":now(),"scope":"instance_data_root",
            "world_id":null,"world_alias":null,"source_token":instance,"consistency":"best_effort",
            "game":null,"profile_id":null,"adapter_version":null,"tool_version":"test","compression":"zstd","archive_format":"tar",
            "characters":[],"entry_count":0,"file_count":0,"total_bytes":"0","entries":[],
            "extensions":{"boundary_verified":false,"instance_source":{"instance_id":instance,"installation_id":installation,"installation_sha256":"a".repeat(64),"binding_verified":false}},
            "compression_version":"test","world_contexts":[]
        });
        let parse = |v: serde_json::Value| serde_json::from_value::<Manifest>(v).unwrap();
        validate_manifest(&parse(value.clone())).unwrap();
        for (pointer, replacement) in [
            (
                "/extensions/instance_source/installation_id",
                serde_json::json!("game"),
            ),
            (
                "/extensions/instance_source/instance_id",
                serde_json::json!("instance"),
            ),
            (
                "/extensions/instance_source/binding_verified",
                serde_json::json!(true),
            ),
            (
                "/extensions/instance_source/instance_id",
                serde_json::json!(crate::database::id()),
            ),
            (
                "/extensions/instance_source/installation_sha256",
                serde_json::json!("invalid"),
            ),
            ("/extensions/instance_source", serde_json::Value::Null),
            ("/extensions/boundary_verified", serde_json::json!(true)),
            ("/scope", serde_json::json!("save_root")),
            ("/schema_version", serde_json::json!(99)),
        ] {
            let mut invalid = value.clone();
            *invalid.pointer_mut(pointer).unwrap() = replacement;
            assert!(validate_manifest(&parse(invalid)).is_err(), "{pointer}");
        }
        let mut legacy = value;
        legacy["schema_version"] = serde_json::json!(1);
        legacy["scope"] = serde_json::json!("save_root");
        legacy["extensions"] = serde_json::json!({});
        validate_manifest(&parse(legacy)).unwrap();
    }
}
