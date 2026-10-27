use hub_core::{Hub, WorkspaceInput, restore_backup_to_new_directory, verify_restored_directory};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
struct Fixture {
    _temp: tempfile::TempDir,
    archive: PathBuf,
    output: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for name in ["saves", "backups", "content"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        std::fs::create_dir(temp.path().join("saves/empty")).unwrap();
        std::fs::write(temp.path().join("saves/opaque"), b"opaque\0binary").unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        hub.initialize(WorkspaceInput {
            save_dir: temp.path().join("saves").to_str().unwrap().into(),
            backup_dir: temp.path().join("backups").to_str().unwrap().into(),
            content_dir: temp.path().join("content").to_str().unwrap().into(),
        })
        .await
        .unwrap();
        let plan = hub.prepare_backup().await.unwrap();
        let task = hub
            .create_backup(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let task = hub.backup_task(&task.id).await.unwrap();
            if task.state == "succeeded" && Arc::strong_count(&hub) == 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "backup fixture timed out: {task:?}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let archive = temp.path().join("backups").join(task.backup_id);
        let output = temp.path().join("recovered");
        restore_backup_to_new_directory(&archive, &output).unwrap();
        Self {
            _temp: temp,
            archive,
            output,
        }
    }
}
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            let value = if path.is_dir() {
                pending.push(path.clone());
                "directory".into()
            } else {
                format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()))
            };
            result.insert(path.strip_prefix(root).unwrap().into(), value);
        }
    }
    result
}
#[tokio::test]
async fn standalone_cli_reads_exact_payload_without_writing_or_opening_metadata() {
    let f = Fixture::new().await;
    // SQLx closes pooled connections asynchronously; Windows may retain delete protection briefly.
    let metadata = f._temp.path().join("metadata");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match std::fs::remove_dir_all(&metadata) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(error) => panic!("could not remove fixture metadata: {error}"),
        }
    }
    assert!(!metadata.exists());
    let before = tree(f._temp.path());
    let usage = std::process::Command::new(env!("CARGO_BIN_EXE_verify-restored"))
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_verify-restored"))
        .arg(&f.archive)
        .arg(&f.output)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["verification"]["state"], "verified");
    assert_eq!(result["verification"]["restored_files"], 1);
    assert_eq!(result["incomplete_marker_present"], false);
    assert_eq!(result["historical_report_present"], true);
    assert_eq!(tree(f._temp.path()), before);
}
#[tokio::test]
async fn full_payload_can_verify_while_the_original_workflow_remains_incomplete() {
    let f = Fixture::new().await;
    std::fs::remove_file(f.output.join("restore-verification.json")).unwrap();
    std::fs::write(
        f.output.join("RESTORE_INCOMPLETE.json"),
        b"{untrusted historical marker",
    )
    .unwrap();
    std::fs::remove_file(f.archive.join("receipt.json")).unwrap();
    let before = tree(&f.output);
    let result = verify_restored_directory(&f.archive, &f.output).unwrap();
    assert_eq!(result.verification.state, "verified");
    assert!(result.incomplete_marker_present);
    assert!(!result.historical_report_present);
    assert!(!result.verification.backup.receipt_present);
    assert_eq!(tree(&f.output), before);
}
#[tokio::test]
async fn stale_success_report_cannot_hide_changed_missing_extra_or_wrong_type_payloads() {
    let f = Fixture::new().await;
    for scenario in [
        "changed",
        "missing",
        "extra",
        "missing_directory",
        "wrong_type",
    ] {
        let output = f._temp.path().join(scenario);
        restore_backup_to_new_directory(&f.archive, &output).unwrap();
        match scenario {
            "changed" => std::fs::write(output.join("data/opaque"), b"tamper\0bytes!").unwrap(),
            "missing" => std::fs::remove_file(output.join("data/opaque")).unwrap(),
            "extra" => std::fs::write(output.join("data/unexpected"), b"extra").unwrap(),
            "missing_directory" => std::fs::remove_dir(output.join("data/empty")).unwrap(),
            "wrong_type" => {
                std::fs::remove_dir(output.join("data/empty")).unwrap();
                std::fs::write(output.join("data/empty"), b"").unwrap();
            }
            _ => unreachable!(),
        }
        let before = tree(&output);
        assert_eq!(
            verify_restored_directory(&f.archive, &output)
                .unwrap_err()
                .code,
            "RESTORED_DATA_MISMATCH",
            "{scenario}"
        );
        assert_eq!(tree(&output), before);
        assert!(output.join("restore-verification.json").exists());
    }
}
#[tokio::test]
async fn missing_or_corrupt_source_archive_cannot_be_replaced_by_a_success_report() {
    let f = Fixture::new().await;
    let archive = f.archive.join("archive.tar.zst");
    std::fs::write(&archive, b"corrupt").unwrap();
    assert_eq!(
        verify_restored_directory(&f.archive, &f.output)
            .unwrap_err()
            .code,
        "HASH_MISMATCH"
    );
    std::fs::remove_file(archive).unwrap();
    assert_eq!(
        verify_restored_directory(&f.archive, &f.output)
            .unwrap_err()
            .code,
        "BACKUP_IO_FAILED"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn inspection_refuses_payload_and_control_marker_symlinks() {
    let f = Fixture::new().await;
    std::fs::rename(f.output.join("data"), f.output.join("original-data")).unwrap();
    std::os::unix::fs::symlink(f.output.join("original-data"), f.output.join("data")).unwrap();
    assert_eq!(
        verify_restored_directory(&f.archive, &f.output)
            .unwrap_err()
            .code,
        "RESTORED_DATA_MISMATCH"
    );
    std::fs::remove_file(f.output.join("data")).unwrap();
    std::fs::rename(f.output.join("original-data"), f.output.join("data")).unwrap();
    let report = f.output.join("restore-verification.json");
    std::fs::rename(&report, f.output.join("original-report")).unwrap();
    std::os::unix::fs::symlink(f.output.join("original-report"), &report).unwrap();
    assert_eq!(
        verify_restored_directory(&f.archive, &f.output)
            .unwrap_err()
            .code,
        "RESTORE_MARKER_INVALID"
    );
}
