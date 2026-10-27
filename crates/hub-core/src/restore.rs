//! Standalone, non-overwriting recovery. A destination is a new container, not a live save root.
use crate::{HubError, Result, backup_format::*};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Component, Path},
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
};

#[derive(Debug, Serialize)]
pub struct RestoreVerification {
    pub schema_version: u32,
    pub state: String,
    pub backup: BackupVerification,
    pub restored_files: u32,
    pub restored_entries: u32,
    pub restored_bytes: String,
}
fn exists() -> HubError {
    HubError::new(
        "RESTORE_TARGET_EXISTS",
        "恢复目标已存在；请选择尚不存在的新目录，已有目录不会被覆盖。",
    )
}
fn write_json(root: &Dir, name: &str, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| invalid())?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = root.open_with(name, &options).map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}
/// Independently read the materialized tree, including the exact path/type set.
fn verify_tree(root: &Dir, manifest: &Manifest, cancel: &AtomicBool) -> Result<()> {
    let expected: BTreeMap<_, _> = manifest
        .entries
        .iter()
        .map(|entry| (entry.relative_path.as_str(), entry))
        .collect();
    let mut pending = vec![String::new()];
    let mut count = 0;
    while let Some(prefix) = pending.pop() {
        let directory = if prefix.is_empty() {
            root.try_clone().map_err(io_error)?
        } else {
            let (parent, name) = parent(root, &prefix)?;
            parent.open_dir_nofollow(name).map_err(io_error)?
        };
        for entry in directory.entries().map_err(io_error)? {
            check_cancel(cancel)?;
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name().into_string().map_err(|_| invalid())?;
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let planned = expected.get(path.as_str()).ok_or_else(invalid)?;
            count += 1;
            if count > expected.len() {
                return Err(invalid());
            }
            let metadata = directory.symlink_metadata(&name).map_err(io_error)?;
            if planned.file_type == "directory" && metadata.is_dir() {
                pending.push(path);
            } else if planned.file_type == "file" && metadata.is_file() {
                let mut file = open_file(&directory, &name)?;
                let size = planned.size.parse().map_err(|_| invalid())?;
                let (actual, hash) = hash_reader(&mut file, size, cancel)?;
                if actual != size || planned.sha256.as_deref() != Some(&hash) {
                    return Err(HubError::new(
                        "HASH_MISMATCH",
                        "恢复后的文件回读校验失败；请保留未完成目录。",
                    ));
                }
            } else {
                return Err(invalid());
            }
        }
        sync_directory(&directory)?;
    }
    if count != expected.len() {
        return Err(invalid());
    }
    // Sync children before parents after every file has been synced and independently read.
    let mut directories: Vec<_> = manifest
        .entries
        .iter()
        .filter(|entry| entry.file_type == "directory")
        .collect();
    directories.sort_by_key(|entry| std::cmp::Reverse(entry.relative_path.matches('/').count()));
    for entry in directories {
        let (parent, name) = parent(root, &entry.relative_path)?;
        sync_directory(&parent.open_dir_nofollow(name).map_err(io_error)?)?;
    }
    sync_directory(root)
}
/// Verify a standalone archive, recover into a newly-created container/data, then read back every file.
/// No application database or Git is opened. Existing targets are always refused, including empty ones.
/// Failure retains RESTORE_INCOMPLETE.json and any partial data; it never cleans or retries automatically.
pub fn restore_backup_to_new_directory(
    backup: &Path,
    destination: &Path,
) -> Result<RestoreVerification> {
    restore(backup, destination, &AtomicBool::new(false))
}
fn restore(backup: &Path, destination: &Path, cancel: &AtomicBool) -> Result<RestoreVerification> {
    check_cancel(cancel)?;
    let source_path = std::fs::canonicalize(backup).map_err(io_error)?;
    let name = destination.file_name().ok_or_else(exists)?;
    if !matches!(
        destination.components().next_back(),
        Some(Component::Normal(_))
    ) {
        return Err(exists());
    }
    let parent_path = std::fs::canonicalize(
        destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .map_err(io_error)?;
    let target_path = parent_path.join(name);
    if target_path.starts_with(&source_path) || source_path.starts_with(&target_path) {
        return Err(HubError::new(
            "RESTORE_PATH_OVERLAP",
            "恢复目标与备份目录不能重叠。",
        ));
    }
    let source =
        Dir::open_ambient_dir(&source_path, cap_std::ambient_authority()).map_err(io_error)?;
    let parent =
        Dir::open_ambient_dir(&parent_path, cap_std::ambient_authority()).map_err(io_error)?;
    match parent.symlink_metadata(name) {
        Ok(_) => return Err(exists()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    stage_restore(
        &source,
        RestoreDestination {
            parent: &parent,
            parent_path: &parent_path,
            name: name.to_str().ok_or_else(invalid)?,
        },
        None,
        cancel,
        &AtomicU8::new(0),
    )?
    .complete(cancel)
}

pub(crate) struct PendingRestore {
    container: Dir,
    parent: Dir,
    result: RestoreVerification,
}
impl PendingRestore {
    pub(crate) fn complete(self, cancel: &AtomicBool) -> Result<RestoreVerification> {
        check_cancel(cancel)?;
        let outcome = (|| {
            write_json(&self.container, "restore-verification.json", &self.result)?;
            sync_directory(&self.container)?;
            self.container
                .remove_file("RESTORE_INCOMPLETE.json")
                .map_err(io_error)?;
            sync_directory(&self.container)?;
            sync_directory(&self.parent)?;
            Ok(())
        })();
        if outcome.is_err()
            && self
                .container
                .symlink_metadata("RESTORE_INCOMPLETE.json")
                .is_err()
        {
            let _ = write_json(
                &self.container,
                "RESTORE_INCOMPLETE.json",
                &serde_json::json!({"schema_version":1,"state":"incomplete","backup_id":self.result.backup.backup_id}),
            );
            let _ = sync_directory(&self.container);
        }
        outcome?;
        Ok(self.result)
    }
}

pub(crate) struct RestoreDestination<'a> {
    pub parent: &'a Dir,
    pub parent_path: &'a Path,
    pub name: &'a str,
}
pub(crate) fn stage_restore(
    source: &Dir,
    target: RestoreDestination<'_>,
    expected_archive: Option<&str>,
    cancel: &AtomicBool,
    phase: &AtomicU8,
) -> Result<PendingRestore> {
    let RestoreDestination {
        parent,
        parent_path,
        name,
    } = target;
    check_cancel(cancel)?;
    phase.store(1, Ordering::Release);
    let (_, before) = verify_dir(source, expected_archive, cancel)?;
    phase.store(2, Ordering::Release);
    let required = before
        .total_bytes
        .parse::<u64>()
        .map_err(|_| invalid())?
        .checked_add(16 * 1024 * 1024)
        .ok_or_else(invalid)?;
    if fs2::available_space(parent_path).map_err(io_error)? < required {
        return Err(HubError::new(
            "DISK_FULL",
            "目标磁盘可用空间不足，恢复未开始。",
        ));
    }
    check_cancel(cancel)?;
    parent.create_dir(name).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            exists()
        } else {
            io_error(error)
        }
    })?;
    let container = parent.open_dir_nofollow(name).map_err(io_error)?;
    write_json(
        &container,
        "RESTORE_INCOMPLETE.json",
        &serde_json::json!({"schema_version":1,"state":"incomplete","backup_id":before.backup_id}),
    )?;
    sync_directory(&container)?;
    sync_directory(parent)?;
    container.create_dir("data").map_err(io_error)?;
    let data = container.open_dir_nofollow("data").map_err(io_error)?;
    let (manifest, verified) =
        inspect_archive(source, Some(&before.archive_sha256), cancel, Some(&data))?;
    phase.store(3, Ordering::Release);
    verify_tree(&data, &manifest, cancel)?;
    check_cancel(cancel)?;
    let result = RestoreVerification {
        schema_version: 1,
        state: "verified".into(),
        restored_files: verified.file_count,
        restored_entries: verified.entry_count,
        restored_bytes: verified.total_bytes.clone(),
        backup: verified,
    };
    Ok(PendingRestore {
        container,
        parent: parent.try_clone().map_err(io_error)?,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_at_completion_boundary_preserves_incomplete_marker_and_no_success_report() {
        let temporary = tempfile::tempdir().unwrap();
        let parent = Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority()).unwrap();
        parent.create_dir("new").unwrap();
        let container = parent.open_dir_nofollow("new").unwrap();
        write_json(
            &container,
            "RESTORE_INCOMPLETE.json",
            &serde_json::json!({"state":"incomplete"}),
        )
        .unwrap();
        let result = RestoreVerification {
            schema_version: 1,
            state: "verified".into(),
            restored_files: 0,
            restored_entries: 0,
            restored_bytes: "0".into(),
            backup: BackupVerification {
                backup_id: "backup".into(),
                file_count: 0,
                entry_count: 0,
                total_bytes: "0".into(),
                consistency: "best_effort".into(),
                archive_sha256: "a".repeat(64),
                manifest_sha256: "b".repeat(64),
                verified_at: "2026-10-27T00:00:00Z".into(),
                receipt_present: true,
            },
        };
        let pending = PendingRestore {
            parent,
            container,
            result,
        };
        assert_eq!(
            pending.complete(&AtomicBool::new(true)).unwrap_err().code,
            "TASK_CANCELLED"
        );
        assert!(
            temporary
                .path()
                .join("new/RESTORE_INCOMPLETE.json")
                .exists()
        );
        assert!(
            !temporary
                .path()
                .join("new/restore-verification.json")
                .exists()
        );
    }
    #[test]
    fn cancellation_before_work_never_creates_a_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("new");
        assert_eq!(
            restore(temporary.path(), &destination, &AtomicBool::new(true))
                .unwrap_err()
                .code,
            "TASK_CANCELLED"
        );
        assert!(!destination.exists());
    }
}
