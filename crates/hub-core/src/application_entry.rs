//! Locate a single game entry in an explicitly selected application bundle.
use crate::{HubError, Result, paths};
use std::path::PathBuf;
pub(crate) fn resolve(selected: &str) -> Result<PathBuf> {
    let root = std::fs::canonicalize(selected)
        .map_err(|_| HubError::new("PATH_INVALID", "所选程序或应用不存在。"))?;
    if root.is_file() {
        return paths::canonical(selected, false);
    }
    if !root.is_dir()
        || !root
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("app"))
    {
        return Err(HubError::new(
            "APPLICATION_REQUIRED",
            "请选择游戏程序或 .app 应用，不能选择普通目录。",
        ));
    }
    let mut candidates = Vec::new();
    for entry in [
        "Contents/MacOS/cataclysm-tiles",
        "Contents/MacOS/cataclysm",
        "Contents/MacOS/cataclysm-hub-qa",
        "Contents/Resources/cataclysm-tiles",
        "Contents/Resources/cataclysm",
    ] {
        let path = root.join(entry);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || std::fs::canonicalize(&path).map_err(|_| invalid())? != path
                {
                    return Err(invalid());
                }
                paths::executable(&path)?;
                candidates.push(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(invalid()),
        }
    }
    if candidates.len() != 1 {
        return Err(HubError::new(
            "APPLICATION_ENTRY_AMBIGUOUS",
            "未找到唯一游戏入口，请进入应用包选择实际游戏程序。",
        ));
    }
    Ok(candidates.remove(0))
}
fn invalid() -> HubError {
    HubError::new(
        "APPLICATION_ENTRY_INVALID",
        "应用入口不可访问或包含链接，请直接选择并核对实际程序。",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn program(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"synthetic, never executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn selects_unique_real_program_and_refuses_ambiguous_entries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Game.app");
        let entry = root.join("Contents/Resources/cataclysm-tiles");
        program(&entry);
        assert_eq!(
            resolve(root.to_str().unwrap()).unwrap(),
            std::fs::canonicalize(&entry).unwrap()
        );
        program(&root.join("Contents/MacOS/cataclysm"));
        assert_eq!(
            resolve(root.to_str().unwrap()).unwrap_err().code,
            "APPLICATION_ENTRY_AMBIGUOUS"
        );
    }
    #[test]
    fn never_uses_launcher_script_or_guesses_normal_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Game.app");
        program(&root.join("Contents/MacOS/Cataclysm.sh"));
        assert!(resolve(root.to_str().unwrap()).is_err());
        assert!(resolve(temp.path().to_str().unwrap()).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_linked_programs_and_linked_bundle_subdirectories() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Game.app");
        let outside = temp.path().join("outside/cataclysm-tiles");
        program(&outside);
        std::fs::create_dir_all(root.join("Contents")).unwrap();
        std::os::unix::fs::symlink(outside.parent().unwrap(), root.join("Contents/Resources"))
            .unwrap();
        assert_eq!(
            resolve(root.to_str().unwrap()).unwrap_err().code,
            "APPLICATION_ENTRY_INVALID"
        );
    }
}
