use crate::{HubError, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Conservative package boundary; an .app suffix is not a trust or layout check.
pub(crate) fn bundle_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")))
        .map(Path::to_path_buf)
}
pub(crate) fn installation_scope(working_dir: &str) -> PathBuf {
    let path = Path::new(working_dir);
    bundle_root(path).unwrap_or_else(|| path.to_owned())
}

pub(crate) fn canonical(path: &str, directory: bool) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path)
        .map_err(|_| HubError::new("PATH_INVALID", "路径不存在或无法访问，请重新选择。"))?;
    let metadata = path
        .metadata()
        .map_err(|_| HubError::new("PERMISSION_DENIED", "无法读取所选路径。"))?;
    if metadata.is_dir() != directory || (!directory && !metadata.is_file()) {
        return Err(HubError::new("PATH_INVALID", "所选路径类型不符合要求。"));
    }
    text(&path)?;
    Ok(path)
}
pub(crate) fn text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or(HubError::new(
        "PATH_UNSUPPORTED",
        "首个版本暂不支持非 UTF-8 路径。",
    ))
}
pub(crate) fn overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
pub(crate) fn writable(dir: &Path) -> Result<()> {
    // Only probe a new, uniquely named manager-owned file; never overwrite existing data.
    let probe = dir.join(format!(".cataclysm-hub-probe-{}", uuid::Uuid::new_v4()));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|_| {
            HubError::new(
                "PERMISSION_DENIED",
                "目录不可写，请选择具有写入权限的目录。",
            )
        })?;
    drop(file);
    std::fs::remove_file(probe)
        .map_err(|_| HubError::new("PERMISSION_DENIED", "无法清理目录权限检查文件。"))
}
pub(crate) async fn digest(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let mut file = File::open(path)
            .map_err(|_| HubError::new("PERMISSION_DENIED", "无法读取游戏可执行文件。"))?;
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|_| {
                HubError::new("SOURCE_CHANGED", "无法完成可执行文件校验，请重新导入。")
            })?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        Ok(format!("{:x}", hash.finalize()))
    })
    .await
    .map_err(|_| HubError::new("HASH_FAILED", "校验任务未完成。"))?
}
pub(crate) fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path
            .metadata()
            .map_err(|_| HubError::new("PATH_INVALID", "可执行文件已不可访问。"))?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err(HubError::new(
                "NOT_EXECUTABLE",
                "所选文件没有执行权限，请选择游戏可执行文件。",
            ));
        }
    }
    #[cfg(windows)]
    {
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err(HubError::new("NOT_EXECUTABLE", "请选择 .exe 游戏程序。"));
        }
    }
    Ok(())
}
