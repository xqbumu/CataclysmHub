use crate::{
    HubError, RestoreVerification, Result,
    backup_format::{io_error, verify_dir},
    database::now,
    restore::verify_tree,
};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use serde::Serialize;
use std::{path::Path, sync::atomic::AtomicBool};

#[derive(Debug, Serialize)]
pub struct RestoreInspection {
    pub schema_version: u32,
    pub inspected_at: String,
    pub verification: RestoreVerification,
    pub incomplete_marker_present: bool,
    pub historical_report_present: bool,
}
fn regular_marker(root: &Dir, name: &str) -> Result<bool> {
    match root.symlink_metadata(name) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(HubError::new(
            "RESTORE_MARKER_INVALID",
            "恢复目录中的状态标记不是普通文件，请保留目录并检查。",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}
/// Read-only inspection of payload bytes against a freshly verified source archive.
/// Marker/report presence never changes the byte verdict or reconstructs the original task outcome.
pub fn verify_restored_directory(backup: &Path, container: &Path) -> Result<RestoreInspection> {
    let cancel = AtomicBool::new(false);
    let archive = Dir::open_ambient_dir(backup, cap_std::ambient_authority()).map_err(io_error)?;
    let root = Dir::open_ambient_dir(container, cap_std::ambient_authority()).map_err(io_error)?;
    let (manifest, verified) = verify_dir(&archive, None, &cancel)?;
    let data = root.open_dir_nofollow("data").map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            io_error(error)
        } else {
            HubError::new(
                "RESTORED_DATA_MISMATCH",
                "恢复载荷目录缺失、不可读或不是独立目录。",
            )
        }
    })?;
    verify_tree(&data, &manifest, &cancel).map_err(|error| match error.code {
        "PERMISSION_DENIED" => error,
        _ => HubError::new(
            "RESTORED_DATA_MISMATCH",
            "恢复目录的路径、类型、大小或 SHA-256 与归档不一致，或未能完整读取。",
        ),
    })?;
    Ok(RestoreInspection {
        schema_version: 1,
        inspected_at: now(),
        incomplete_marker_present: regular_marker(&root, "RESTORE_INCOMPLETE.json")?,
        historical_report_present: regular_marker(&root, "restore-verification.json")?,
        verification: RestoreVerification {
            schema_version: 1,
            state: "verified".into(),
            restored_files: verified.file_count,
            restored_entries: verified.entry_count,
            restored_bytes: verified.total_bytes.clone(),
            backup: verified,
        },
    })
}
