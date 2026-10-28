use hub_core::{
    copy_installation_to_new_directory, plan_installation_copy, verify_installation_copy,
};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
struct Fixture {
    _temp: tempfile::TempDir,
    source: PathBuf,
    dest: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("Synthetic game.app");
        let dest = temp.path().join("copy container");
        std::fs::create_dir_all(source.join("Contents/Resources/data")).unwrap();
        std::fs::write(
            source.join("Contents/Resources/game.exe"),
            b"synthetic executable, never run",
        )
        .unwrap();
        std::fs::write(
            source.join("Contents/Resources/data/cache.dat"),
            b"source cache",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                source.join("Contents/Resources/game.exe"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        Self {
            _temp: temp,
            source,
            dest,
        }
    }
    fn executable() -> &'static str {
        "Contents/Resources/game.exe"
    }
    fn copy(&self) -> hub_core::InstallationCopyVerification {
        copy_installation_to_new_directory(&self.source, Self::executable(), &self.dest).unwrap()
    }
}
#[test]
fn preview_is_readonly_and_complete_copy_is_independent_and_verifies_without_source() {
    let f = Fixture::new();
    let plan = plan_installation_copy(&f.source, Fixture::executable(), &f.dest).unwrap();
    assert!(!f.dest.exists());
    assert_eq!(plan.entries.iter().filter(|e| e.kind == "file").count(), 2);
    let copied = f.copy();
    assert!(!copied.binding_verified && !copied.isolation_verified);
    let payload = f.dest.join(&copied.payload_root);
    for name in [Fixture::executable(), "Contents/Resources/data/cache.dat"] {
        assert_eq!(
            std::fs::read(f.source.join(name)).unwrap(),
            std::fs::read(payload.join(name)).unwrap()
        );
        assert!(!same_file::is_same_file(f.source.join(name), payload.join(name)).unwrap());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(payload.join(Fixture::executable()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }
    std::fs::rename(&f.source, f.source.with_file_name("offline-source")).unwrap();
    verify_installation_copy(&f.dest).unwrap();
    std::fs::write(
        payload.join("Contents/Resources/data/cache.dat"),
        b"target cache",
    )
    .unwrap();
    assert!(verify_installation_copy(&f.dest).is_err());
    assert_eq!(
        std::fs::read(
            f.source
                .with_file_name("offline-source")
                .join("Contents/Resources/data/cache.dat")
        )
        .unwrap(),
        b"source cache"
    );
}
#[test]
fn existing_targets_and_nested_destinations_are_never_overwritten() {
    let f = Fixture::new();
    std::fs::create_dir(&f.dest).unwrap();
    let marker = f.dest.join("keep");
    std::fs::write(&marker, b"preserve").unwrap();
    assert_eq!(
        copy_installation_to_new_directory(&f.source, Fixture::executable(), &f.dest)
            .unwrap_err()
            .code,
        "COPY_TARGET_EXISTS"
    );
    assert_eq!(std::fs::read(marker).unwrap(), b"preserve");
    assert_eq!(
        plan_installation_copy(&f.source, Fixture::executable(), &f.source.join("nested"))
            .unwrap_err()
            .code,
        "COPY_SCOPE_OVERLAP"
    );
    assert!(!f.source.join("nested").exists());
}
#[cfg(unix)]
#[test]
fn relative_framework_links_are_preserved_and_external_or_dangling_links_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let framework = f.source.join("Contents/Resources/SDL.framework");
    std::fs::create_dir_all(framework.join("Versions/A/Resources")).unwrap();
    std::fs::write(framework.join("Versions/A/SDL"), b"synthetic framework").unwrap();
    symlink("A", framework.join("Versions/Current")).unwrap();
    symlink("Versions/Current/SDL", framework.join("SDL")).unwrap();
    symlink("Versions/Current/Resources", framework.join("Resources")).unwrap();
    let copy = f.copy();
    assert_eq!(copy.link_count, 3);
    verify_installation_copy(&f.dest).unwrap();
    assert_eq!(
        std::fs::read_link(
            f.dest
                .join(copy.payload_root)
                .join("Contents/Resources/SDL.framework/Versions/Current")
        )
        .unwrap(),
        Path::new("A")
    );
    for target in ["../../outside", "missing", "loop"] {
        let f = Fixture::new();
        symlink(target, f.source.join("loop")).unwrap();
        assert!(plan_installation_copy(&f.source, Fixture::executable(), &f.dest).is_err());
        assert!(!f.dest.exists());
    }
    let f = Fixture::new();
    symlink(&f.source, f.source.join("absolute")).unwrap();
    assert!(plan_installation_copy(&f.source, Fixture::executable(), &f.dest).is_err());
}
#[cfg(unix)]
#[test]
fn special_files_setid_bits_and_hardlinked_target_files_are_rejected() {
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};
    let f = Fixture::new();
    let _socket = UnixListener::bind(f.source.join("socket")).unwrap();
    assert_eq!(
        plan_installation_copy(&f.source, Fixture::executable(), &f.dest)
            .unwrap_err()
            .code,
        "COPY_FILE_TYPE_UNSUPPORTED"
    );
    let f = Fixture::new();
    std::fs::set_permissions(
        f.source.join(Fixture::executable()),
        std::fs::Permissions::from_mode(0o4755),
    )
    .unwrap();
    assert_eq!(
        plan_installation_copy(&f.source, Fixture::executable(), &f.dest)
            .unwrap_err()
            .code,
        "COPY_PERMISSION_UNSUPPORTED"
    );
    let f = Fixture::new();
    let copied = f.copy();
    let target = f
        .dest
        .join(copied.payload_root)
        .join("Contents/Resources/data/cache.dat");
    let extra = f._temp.path().join("external-hardlink");
    std::fs::hard_link(target, extra).unwrap();
    assert!(verify_installation_copy(&f.dest).is_err());
}
#[test]
fn malformed_controls_extra_files_and_false_verification_promotions_are_rejected() {
    let f = Fixture::new();
    f.copy();
    let receipt = f.dest.join("copy-verification.json");
    let original = std::fs::read(&receipt).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["isolation_verified"] = serde_json::json!(true);
    std::fs::write(&receipt, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(verify_installation_copy(&f.dest).is_err());
    std::fs::write(&receipt, &original).unwrap();
    let extra = f.dest.join("untracked");
    std::fs::write(&extra, b"extra").unwrap();
    assert!(verify_installation_copy(&f.dest).is_err());
    std::fs::remove_file(extra).unwrap();
    std::fs::write(
        f.dest.join("INSTALLATION_COPY_INCOMPLETE.json"),
        b"incomplete",
    )
    .unwrap();
    assert_eq!(
        verify_installation_copy(&f.dest).unwrap_err().code,
        "INSTALLATION_COPY_INCOMPLETE"
    );
}
#[test]
fn cli_requires_explicit_mode_and_does_not_execute_the_copied_program() {
    let f = Fixture::new();
    let binary = env!("CARGO_BIN_EXE_copy-installation");
    let output = std::process::Command::new(binary)
        .args([
            f.source.as_os_str(),
            std::ffi::OsStr::new(Fixture::executable()),
            f.dest.as_os_str(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!f.dest.exists());
    let output = std::process::Command::new(binary)
        .arg(&f.source)
        .arg(Fixture::executable())
        .arg(&f.dest)
        .arg("--confirm-new-directory")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::process::Command::new(binary)
        .arg("--verify")
        .arg(&f.dest)
        .output()
        .unwrap();
    assert!(output.status.success());
}
