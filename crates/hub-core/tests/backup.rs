use hub_core::{BackupTask, Hub, WorkspaceInput, verify_backup_directory};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, sync::Arc};
use tempfile::TempDir;
struct Fixture {
    _temp: TempDir,
    source: PathBuf,
    backups: PathBuf,
    hub: Arc<Hub>,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("saves");
        let backups = temp.path().join("backups");
        let content = temp.path().join("content");
        for path in [&source, &backups, &content] {
            std::fs::create_dir(path).unwrap();
        }
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        hub.initialize(WorkspaceInput {
            save_dir: source.to_str().unwrap().into(),
            backup_dir: backups.to_str().unwrap().into(),
            content_dir: content.to_str().unwrap().into(),
        })
        .await
        .unwrap();
        Self {
            _temp: temp,
            source,
            backups,
            hub,
        }
    }
    async fn create(&self) -> BackupTask {
        let plan = self.hub.prepare_backup().await.unwrap();
        let task = self
            .hub
            .create_backup(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap();
        wait(&self.hub, &task.id).await
    }
}
async fn wait(hub: &Hub, id: &str) -> BackupTask {
    for _ in 0..1500 {
        let task = hub.backup_task(id).await.unwrap();
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return task;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("backup should finish");
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn update_receipt(root: &std::path::Path) {
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("receipt.json")).unwrap()).unwrap();
    receipt["archive_sha256"] = hash(&std::fs::read(root.join("archive.tar.zst")).unwrap()).into();
    std::fs::write(
        root.join("receipt.json"),
        serde_json::to_vec(&receipt).unwrap(),
    )
    .unwrap();
}
#[tokio::test]
async fn full_root_backup_preserves_unknown_bytes_empty_directories_and_long_names_without_git_or_db()
 {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.source.join("world/empty")).unwrap();
    let long = "x".repeat(150);
    std::fs::write(f.source.join("world").join(&long), b"unknown\0binary").unwrap();
    std::fs::write(f.source.join("shared.json"), b"{invalid json").unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "succeeded", "{:?}", task);
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(data.backups.len(), 1);
    assert_eq!(data.backups[0].consistency, "best_effort");
    let backup = f.backups.join(&task.backup_id);
    let result = verify_backup_directory(&backup).unwrap();
    assert_eq!(result.entry_count, 4);
    assert_eq!(result.file_count, 2);
    assert!(!backup.join("data").exists());
    assert!(!backup.join("owner.json").exists());
    // An independent normal TAR reader is used as a byte-level oracle.
    let decoder = zstd::stream::read::Decoder::new(
        std::fs::File::open(backup.join("archive.tar.zst")).unwrap(),
    )
    .unwrap();
    let mut archive = tar::Archive::new(decoder);
    let mut payloads = std::collections::BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes).unwrap();
        payloads.insert(path, bytes);
    }
    assert_eq!(payloads[&format!("data/world/{long}")], b"unknown\0binary");
    assert_eq!(payloads["data/shared.json"], b"{invalid json");
    assert!(
        payloads
            .keys()
            .any(|p| p.trim_end_matches('/') == "data/world/empty")
    );
    assert_eq!(
        std::fs::read(f.source.join("shared.json")).unwrap(),
        b"{invalid json"
    );
    drop(f.hub);
    assert_eq!(
        verify_backup_directory(&backup).unwrap().backup_id,
        task.backup_id
    );
}
#[tokio::test]
async fn empty_source_is_a_complete_independently_verifiable_archive() {
    let f = Fixture::new().await;
    let task = f.create().await;
    assert_eq!(task.state, "succeeded");
    let result = verify_backup_directory(&f.backups.join(task.backup_id)).unwrap();
    assert_eq!(result.entry_count, 0);
    assert_eq!(result.total_bytes, "0");
}
#[tokio::test]
async fn scope_changes_after_preview_fail_without_publishing_a_partial_backup() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("before"), "old").unwrap();
    let plan = f.hub.prepare_backup().await.unwrap();
    std::fs::write(f.source.join("new-file"), "new").unwrap();
    let task = f
        .hub
        .create_backup(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    let task = wait(&f.hub, &task.id).await;
    assert_eq!(task.state, "failed");
    assert_eq!(task.error_code.as_deref(), Some("PLAN_STALE"));
    assert!(f.hub.dashboard().await.unwrap().backups.is_empty());
    assert!(!f.backups.join(task.backup_id).exists());
    assert_eq!(std::fs::read(f.source.join("before")).unwrap(), b"old");
}
#[tokio::test]
async fn confirmation_plan_integrity_and_request_replay_are_enforced_on_backend() {
    let f = Fixture::new().await;
    let plan = f.hub.prepare_backup().await.unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .create_backup(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let first = f
        .hub
        .create_backup(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .create_backup(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(
        f.hub
            .create_backup(&plan.id, "different", &key, true)
            .await
            .unwrap_err()
            .code,
        "REQUEST_CONFLICT"
    );
    assert_eq!(wait(&f.hub, &first.id).await.state, "succeeded");
    assert_eq!(f.hub.dashboard().await.unwrap().backups.len(), 1);
}
#[cfg(unix)]
#[tokio::test]
async fn symlinks_and_hardlinks_are_not_silently_omitted_or_dereferenced() {
    let f = Fixture::new().await;
    std::os::unix::fs::symlink(&f.backups, f.source.join("escape")).unwrap();
    assert_eq!(
        f.hub.prepare_backup().await.unwrap_err().code,
        "UNSUPPORTED_FILE_TYPE"
    );
    std::fs::remove_file(f.source.join("escape")).unwrap();
    std::fs::write(f.source.join("one"), "same").unwrap();
    std::fs::hard_link(f.source.join("one"), f.source.join("two")).unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "failed");
    assert_eq!(task.error_code.as_deref(), Some("UNSUPPORTED_FILE_TYPE"));
    assert!(!f.backups.join(task.backup_id).exists());
}
#[tokio::test]
async fn cancellation_never_publishes_an_archive() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("large"), vec![7_u8; 16 * 1024 * 1024]).unwrap();
    let plan = f.hub.prepare_backup().await.unwrap();
    let task = f
        .hub
        .create_backup(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    f.hub.cancel_backup(&task.id).await.unwrap();
    assert_eq!(wait(&f.hub, &task.id).await.state, "cancelled");
    assert!(f.hub.dashboard().await.unwrap().backups.is_empty());
    assert!(!f.backups.join(task.backup_id).exists());
    assert_eq!(
        std::fs::metadata(f.source.join("large")).unwrap().len(),
        16 * 1024 * 1024
    );
}
#[tokio::test]
async fn corruption_is_reported_by_standalone_and_managed_verification() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("a"), "test").unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "succeeded");
    let root = f.backups.join(&task.backup_id);
    std::fs::write(root.join("archive.tar.zst"), b"corrupt").unwrap();
    assert_eq!(
        verify_backup_directory(&root).unwrap_err().code,
        "HASH_MISMATCH"
    );
    let verification = f
        .hub
        .verify_backup(&task.backup_id, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let verification = wait(&f.hub, &verification.id).await;
    assert_eq!(verification.state, "failed");
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "failed"
    );
}
#[tokio::test]
async fn truncated_zstd_frame_is_rejected_even_when_the_outer_hash_is_recomputed() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("a"), "bytes").unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "succeeded");
    let root = f.backups.join(task.backup_id);
    let bytes = std::fs::read(root.join("archive.tar.zst")).unwrap();
    std::fs::write(root.join("archive.tar.zst"), &bytes[..bytes.len() - 3]).unwrap();
    update_receipt(&root);
    assert!(verify_backup_directory(&root).is_err());
}
#[tokio::test]
async fn a_traversal_payload_is_rejected_without_writing_to_source_or_destination() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("a"), "bytes").unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "succeeded");
    let root = f.backups.join(task.backup_id);
    let decoder = zstd::stream::read::Decoder::new(
        std::fs::File::open(root.join("archive.tar.zst")).unwrap(),
    )
    .unwrap();
    let mut original = tar::Archive::new(decoder);
    let mut manifest = vec![];
    original
        .entries()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .read_to_end(&mut manifest)
        .unwrap();
    drop(original);
    let mut out = tar::Builder::new(zstd::stream::write::Encoder::new(Vec::new(), 1).unwrap());
    let mut h = tar::Header::new_gnu();
    h.set_size(manifest.len() as u64);
    h.set_mode(0o600);
    h.set_cksum();
    out.append_data(&mut h, "manifest.json", manifest.as_slice())
        .unwrap();
    let mut h = tar::Header::new_gnu();
    h.set_size(5);
    h.set_mode(0o600);
    h.as_mut_bytes()[..17].copy_from_slice(b"data/../escaped\0\0");
    h.set_cksum();
    out.append(&h, b"bytes".as_slice()).unwrap();
    let bytes = out.into_inner().unwrap().finish().unwrap();
    std::fs::write(root.join("archive.tar.zst"), bytes).unwrap();
    update_receipt(&root);
    assert!(verify_backup_directory(&root).is_err());
    let target = f._temp.path().join("rejected-restore");
    assert!(hub_core::restore_backup_to_new_directory(&root, &target).is_err());
    assert!(!target.exists());
    assert!(!f.backups.join("escaped").exists());
    assert_eq!(std::fs::read(f.source.join("a")).unwrap(), b"bytes");
}

#[tokio::test]
async fn interrupted_backup_is_not_replayed_or_exposed_as_a_valid_backup() {
    let f = Fixture::new().await;
    let workspace_id = f.hub.dashboard().await.unwrap().workspace.unwrap().id;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(f._temp.path().join("metadata/metadata.sqlite")),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES ('interrupted','key','digest',?,'archive','backup.create','running','copying','2026-10-27')")
        .bind(workspace_id).execute(&pool).await.unwrap();
    let partial = f.backups.join(".partial-archive");
    std::fs::create_dir(&partial).unwrap();
    std::fs::write(partial.join("owner.json"), "unpublished").unwrap();
    assert_eq!(
        f.hub
            .start_world_scan(&uuid::Uuid::new_v4().to_string())
            .await
            .unwrap_err()
            .code,
        "TASK_CONFLICT"
    );
    assert_eq!(
        f.hub
            .launch_plan("any-installation", vec![])
            .await
            .unwrap_err()
            .code,
        "TASK_CONFLICT"
    );
    pool.close().await;
    drop(f.hub);
    let reopened = Hub::open(&f._temp.path().join("metadata")).await.unwrap();
    assert_eq!(
        reopened.backup_task("interrupted").await.unwrap().state,
        "recovery_required"
    );
    assert!(reopened.dashboard().await.unwrap().backups.is_empty());
    assert!(partial.join("owner.json").exists());
}
#[tokio::test]
async fn finished_request_replay_survives_restart_without_a_live_plan() {
    let f = Fixture::new().await;
    let plan = f.hub.prepare_backup().await.unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    let first = f
        .hub
        .create_backup(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(wait(&f.hub, &first.id).await.state, "succeeded");
    // The worker may hold its Arc briefly while releasing the cancellation control.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    drop(f.hub);
    let reopened = Hub::open(&f._temp.path().join("metadata")).await.unwrap();
    assert_eq!(
        reopened
            .create_backup(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(reopened.dashboard().await.unwrap().backups.len(), 1);
}

#[tokio::test]
async fn internal_manifest_remains_verifiable_if_the_external_receipt_is_lost() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("opaque"), b"unknown bytes").unwrap();
    let task = f.create().await;
    assert_eq!(task.state, "succeeded");
    let root = f.backups.join(&task.backup_id);
    std::fs::remove_file(root.join("receipt.json")).unwrap();
    let result = verify_backup_directory(&root).unwrap();
    assert!(!result.receipt_present);
    assert_eq!(result.file_count, 1);
    let job = f
        .hub
        .verify_backup(&task.backup_id, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(wait(&f.hub, &job.id).await.state, "succeeded");
}

#[tokio::test]
async fn missing_archive_revokes_current_verified_status_but_retains_last_success_time() {
    let f = Fixture::new().await;
    let created = f.create().await;
    assert_eq!(created.state, "succeeded");
    let previous = f.hub.dashboard().await.unwrap().backups[0].clone();
    std::fs::remove_file(f.backups.join(&created.backup_id).join("archive.tar.zst")).unwrap();
    let task = f
        .hub
        .verify_backup(&created.backup_id, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(wait(&f.hub, &task.id).await.state, "failed");
    let current = f.hub.dashboard().await.unwrap().backups[0].clone();
    assert_eq!(current.verification, "unavailable");
    assert_eq!(current.verified_at, previous.verified_at);
}

#[tokio::test]
async fn terminal_write_failure_is_visible_and_recording_can_retry_without_replaying_backup() {
    let f = Fixture::new().await;
    let db = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f._temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_terminal BEFORE UPDATE OF state ON backup_task WHEN NEW.state != OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&db).await.unwrap();
    let ended = f.create().await;
    assert_eq!(ended.state, "recovery_required");
    assert_eq!(
        ended.error_code.as_deref(),
        Some("TASK_RESULT_PERSIST_FAILED")
    );
    let raw: String = sqlx::query_scalar("SELECT state FROM backup_task WHERE id=?")
        .bind(&ended.id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(raw, "running");
    assert_eq!(
        f.hub.cancel_backup(&ended.id).await.unwrap().state,
        "recovery_required"
    );
    assert_eq!(
        f.hub.dashboard().await.unwrap().backup_tasks[0].state,
        "recovery_required"
    );
    f.hub
        .prepare_backup()
        .await
        .expect("ended worker must not block workspace");
    let published = std::fs::read_dir(&f.backups).unwrap().count();
    sqlx::query("DROP TRIGGER reject_terminal")
        .execute(&db)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&ended.id).await.unwrap();
    let raw: String = sqlx::query_scalar("SELECT state FROM backup_task WHERE id=?")
        .bind(&ended.id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(raw, "recovery_required");
    assert_eq!(std::fs::read_dir(&f.backups).unwrap().count(), published);
    assert!(f.hub.dashboard().await.unwrap().backups.is_empty());
    assert_eq!(f.create().await.state, "succeeded");
}

#[tokio::test]
async fn standalone_restore_materializes_exact_bytes_and_empty_directories_without_metadata() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.source.join("世界/empty")).unwrap();
    let long = "x".repeat(150);
    std::fs::write(f.source.join("世界").join(&long), b"unknown\0binary").unwrap();
    std::fs::write(f.source.join("shared.json"), b"{invalid json").unwrap();
    let task = f.create().await;
    let archive = f.backups.join(&task.backup_id);
    // Disaster recovery must not depend on the SQLite registry or the external receipt.
    std::fs::remove_file(archive.join("receipt.json")).unwrap();
    let output = f._temp.path().join("recovery");
    let result = hub_core::restore_backup_to_new_directory(&archive, &output).unwrap();
    assert_eq!(result.state, "verified");
    assert_eq!(result.restored_files, 2);
    assert_eq!(result.restored_entries, 4);
    assert!(!result.backup.receipt_present);
    assert!(output.join("data/世界/empty").is_dir());
    assert_eq!(
        std::fs::read(output.join("data/世界").join(&long)).unwrap(),
        b"unknown\0binary"
    );
    assert_eq!(
        std::fs::read(output.join("data/shared.json")).unwrap(),
        b"{invalid json"
    );
    assert!(!output.join("RESTORE_INCOMPLETE.json").exists());
    assert!(output.join("restore-verification.json").is_file());
    assert_eq!(
        std::fs::read(f.source.join("shared.json")).unwrap(),
        b"{invalid json"
    );
    assert_eq!(
        hub_core::restore_backup_to_new_directory(&archive, &output)
            .unwrap_err()
            .code,
        "RESTORE_TARGET_EXISTS"
    );
}

#[tokio::test]
async fn restore_refuses_existing_empty_populated_and_overlapping_destinations() {
    let f = Fixture::new().await;
    let task = f.create().await;
    let archive = f.backups.join(&task.backup_id);
    for name in ["empty", "populated"] {
        let output = f._temp.path().join(name);
        std::fs::create_dir(&output).unwrap();
        if name == "populated" {
            std::fs::write(output.join("keep"), b"unchanged").unwrap();
        }
        assert_eq!(
            hub_core::restore_backup_to_new_directory(&archive, &output)
                .unwrap_err()
                .code,
            "RESTORE_TARGET_EXISTS"
        );
    }
    assert_eq!(
        hub_core::restore_backup_to_new_directory(&archive, &archive.join("nested"))
            .unwrap_err()
            .code,
        "RESTORE_PATH_OVERLAP"
    );
    assert_eq!(
        std::fs::read(f._temp.path().join("populated/keep")).unwrap(),
        b"unchanged"
    );
}

#[tokio::test]
async fn restore_rejects_corruption_before_creating_any_target_directory() {
    let f = Fixture::new().await;
    let task = f.create().await;
    let archive = f.backups.join(&task.backup_id);
    std::fs::write(archive.join("archive.tar.zst"), b"corrupt").unwrap();
    let output = f._temp.path().join("recovery");
    assert_eq!(
        hub_core::restore_backup_to_new_directory(&archive, &output)
            .unwrap_err()
            .code,
        "HASH_MISMATCH"
    );
    assert!(!output.exists());
}

#[tokio::test]
async fn restoring_an_empty_archive_still_requires_and_creates_a_new_container() {
    let f = Fixture::new().await;
    let task = f.create().await;
    let output = f._temp.path().join("empty-recovery");
    let result =
        hub_core::restore_backup_to_new_directory(&f.backups.join(&task.backup_id), &output)
            .unwrap();
    assert_eq!(result.restored_entries, 0);
    assert_eq!(std::fs::read_dir(output.join("data")).unwrap().count(), 0);
}

#[cfg(unix)]
#[tokio::test]
async fn restore_refuses_a_target_symlink_without_touching_its_contents() {
    let f = Fixture::new().await;
    let task = f.create().await;
    let output = f._temp.path().join("linked-target");
    std::os::unix::fs::symlink(&f.source, &output).unwrap();
    assert_eq!(
        hub_core::restore_backup_to_new_directory(&f.backups.join(&task.backup_id), &output)
            .unwrap_err()
            .code,
        "RESTORE_TARGET_EXISTS"
    );
    assert_eq!(std::fs::read_dir(&f.source).unwrap().count(), 0);
}

#[tokio::test]
async fn restore_never_silently_merges_case_colliding_archive_paths() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("a"), b"bytes").unwrap();
    let insensitive = f.source.join("A").exists();
    let task = f.create().await;
    let root = f.backups.join(&task.backup_id);
    let mut original = tar::Archive::new(
        zstd::stream::read::Decoder::new(
            std::fs::File::open(root.join("archive.tar.zst")).unwrap(),
        )
        .unwrap(),
    );
    let mut bytes = vec![];
    original
        .entries()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut alias = manifest["entries"][0].clone();
    alias["relative_path"] = "A".into();
    manifest["entries"].as_array_mut().unwrap().push(alias);
    manifest["entry_count"] = 2.into();
    manifest["file_count"] = 2.into();
    manifest["total_bytes"] = "10".into();
    let manifest = serde_json::to_vec(&manifest).unwrap();
    let mut archive = tar::Builder::new(zstd::stream::write::Encoder::new(Vec::new(), 1).unwrap());
    for (path, contents) in [
        ("manifest.json", manifest.as_slice()),
        ("data/a", b"bytes".as_slice()),
        ("data/A", b"bytes".as_slice()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive.append_data(&mut header, path, contents).unwrap();
    }
    std::fs::write(
        root.join("archive.tar.zst"),
        archive.into_inner().unwrap().finish().unwrap(),
    )
    .unwrap();
    std::fs::remove_file(root.join("receipt.json")).unwrap();
    verify_backup_directory(&root).unwrap();
    let output = f._temp.path().join("collision-recovery");
    let result = hub_core::restore_backup_to_new_directory(&root, &output);
    if insensitive {
        assert!(result.is_err());
        assert!(output.join("RESTORE_INCOMPLETE.json").exists());
        assert!(!output.join("restore-verification.json").exists());
    } else {
        assert_eq!(result.unwrap().restored_files, 2);
    }
    assert_eq!(std::fs::read(f.source.join("a")).unwrap(), b"bytes");
}

#[tokio::test]
async fn standalone_restore_cli_requires_explicit_confirmation_and_reports_verified_output() {
    let f = Fixture::new().await;
    let task = f.create().await;
    let root = f.backups.join(task.backup_id);
    let destination = f._temp.path().join("cli-recovery");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_restore-backup"))
        .arg(&root)
        .arg(&destination)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!destination.exists());
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_restore-backup"))
        .arg(&root)
        .arg(&destination)
        .arg("--confirm-new-directory")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["state"], "verified");
    assert!(destination.join("data").is_dir());
}

async fn wait_restore(hub: &Hub, id: &str) -> hub_core::RestoreTask {
    for _ in 0..1000 {
        let task = hub.restore_task(id).await.unwrap();
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return task;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("restore should finish");
}
#[tokio::test]
async fn managed_restore_requires_confirmation_and_audits_a_new_verified_directory() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.source.join("world/empty")).unwrap();
    std::fs::write(f.source.join("world/a"), b"unknown\0data").unwrap();
    let backup = f.create().await;
    let parent = f._temp.path().to_str().unwrap();
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, parent)
        .await
        .unwrap();
    assert!(!std::path::Path::new(&plan.destination).exists());
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .start_restore(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let task = f
        .hub
        .start_restore(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    let ended = wait_restore(&f.hub, &task.id).await;
    assert_eq!(ended.state, "succeeded", "{ended:?}");
    assert_eq!(
        std::fs::read(std::path::Path::new(&plan.destination).join("data/world/a")).unwrap(),
        b"unknown\0data"
    );
    assert!(
        std::path::Path::new(&plan.destination)
            .join("data/world/empty")
            .is_dir()
    );
    assert_eq!(
        f.hub.cancel_restore(&task.id).await.unwrap().state,
        "succeeded"
    );
    assert_eq!(
        f.hub
            .start_restore(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        task.id
    );
    assert_eq!(
        f.hub
            .start_restore(&plan.id, "other", &key, true)
            .await
            .unwrap_err()
            .code,
        "REQUEST_CONFLICT"
    );
    let dashboard = f.hub.dashboard().await.unwrap();
    assert_eq!(dashboard.restore_tasks[0].destination, plan.destination);
    assert!(
        dashboard
            .operations
            .iter()
            .any(|operation| operation.task_id == task.id
                && operation.status == "succeeded"
                && operation.action == "backup.restore_new")
    );
    assert_eq!(
        std::fs::read(f.source.join("world/a")).unwrap(),
        b"unknown\0data"
    );
}
#[tokio::test]
async fn managed_restore_refuses_protected_roots_and_digest_changes() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    for parent in [&f.source, &f.backups, &f._temp.path().join("metadata")] {
        assert_eq!(
            f.hub
                .prepare_restore(&backup.backup_id, parent.to_str().unwrap())
                .await
                .unwrap_err()
                .code,
            "RESTORE_PATH_OVERLAP"
        );
    }
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .start_restore(&plan.id, "bad", &uuid::Uuid::new_v4().to_string(), true)
            .await
            .unwrap_err()
            .code,
        "PLAN_STALE"
    );
    assert!(!std::path::Path::new(&plan.destination).exists());
}
#[tokio::test]
async fn changed_archive_or_preexisting_destination_never_gets_restored_or_overwritten() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    std::fs::create_dir(&plan.destination).unwrap();
    std::fs::write(
        std::path::Path::new(&plan.destination).join("keep"),
        b"keep",
    )
    .unwrap();
    let task = f
        .hub
        .start_restore(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    let ended = wait_restore(&f.hub, &task.id).await;
    assert_eq!(ended.error_code.as_deref(), Some("RESTORE_TARGET_EXISTS"));
    assert_eq!(
        std::fs::read(std::path::Path::new(&plan.destination).join("keep")).unwrap(),
        b"keep"
    );
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    std::fs::write(
        f.backups.join(&backup.backup_id).join("archive.tar.zst"),
        b"corrupt",
    )
    .unwrap();
    let task = f
        .hub
        .start_restore(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        wait_restore(&f.hub, &task.id).await.error_code.as_deref(),
        Some("HASH_MISMATCH")
    );
    assert!(!std::path::Path::new(&plan.destination).exists());
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "failed"
    );
}
#[tokio::test]
async fn replaced_parent_is_refused_even_when_the_path_text_is_unchanged() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    let parent = f._temp.path().join("chosen-parent");
    std::fs::create_dir(&parent).unwrap();
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, parent.to_str().unwrap())
        .await
        .unwrap();
    std::fs::rename(&parent, f._temp.path().join("old-parent")).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert_eq!(
        f.hub
            .start_restore(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .unwrap_err()
            .code,
        "PLAN_STALE"
    );
    assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 0);
}
#[tokio::test]
async fn cancellation_and_conflict_gates_are_enforced_by_the_restore_backend() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("large"), vec![7_u8; 16 * 1024 * 1024]).unwrap();
    let backup = f.create().await;
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    let task = f
        .hub
        .start_restore(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        f.hub.prepare_backup().await.unwrap_err().code,
        "TASK_CONFLICT"
    );
    assert_eq!(
        f.hub
            .start_world_scan(&uuid::Uuid::new_v4().to_string())
            .await
            .unwrap_err()
            .code,
        "TASK_CONFLICT"
    );
    assert_eq!(
        f.hub
            .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "TASK_CONFLICT"
    );
    f.hub.cancel_restore(&task.id).await.unwrap();
    assert_eq!(wait_restore(&f.hub, &task.id).await.state, "cancelled");
    assert!(
        !std::path::Path::new(&plan.destination)
            .join("restore-verification.json")
            .exists()
    );
    f.hub.prepare_backup().await.unwrap();
}
#[tokio::test]
async fn restore_terminal_persistence_failure_is_recoverable_without_replaying_files() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    let db = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f._temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_restore_terminal BEFORE UPDATE OF state ON restore_task WHEN NEW.state != OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&db).await.unwrap();
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    let task = f
        .hub
        .start_restore(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    let ended = wait_restore(&f.hub, &task.id).await;
    assert_eq!(ended.state, "recovery_required");
    assert_eq!(
        ended.error_code.as_deref(),
        Some("TASK_RESULT_PERSIST_FAILED")
    );
    f.hub.prepare_backup().await.unwrap();
    let report = std::path::Path::new(&plan.destination).join("restore-verification.json");
    let before = std::fs::read(&report).unwrap();
    sqlx::query("DROP TRIGGER reject_restore_terminal")
        .execute(&db)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&task.id).await.unwrap();
    assert_eq!(std::fs::read(&report).unwrap(), before);
    assert_eq!(
        f.hub.restore_task(&task.id).await.unwrap().state,
        "recovery_required"
    );
}
#[tokio::test]
async fn interrupted_restores_are_not_replayed_on_restart_and_request_identity_survives() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    let plan = f
        .hub
        .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
        .await
        .unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    let task = f
        .hub
        .start_restore(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(wait_restore(&f.hub, &task.id).await.state, "succeeded");
    for _ in 0..100 {
        if Arc::strong_count(&f.hub) == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let db = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f._temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query(
        "UPDATE restore_task SET state='running',phase='extracting',finished_at=NULL WHERE id=?",
    )
    .bind(&task.id)
    .execute(&db)
    .await
    .unwrap();
    db.close().await;
    let report = std::path::Path::new(&plan.destination).join("restore-verification.json");
    let before = std::fs::read(&report).unwrap();
    drop(f.hub);
    let hub = Hub::open(&f._temp.path().join("metadata")).await.unwrap();
    let reconciled = hub.restore_task(&task.id).await.unwrap();
    assert_eq!(reconciled.state, "recovery_required");
    assert_eq!(reconciled.error_code.as_deref(), Some("TASK_INTERRUPTED"));
    assert_eq!(
        hub.start_restore(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        task.id
    );
    assert_eq!(std::fs::read(&report).unwrap(), before);
}

#[tokio::test]
async fn schema3_migration_preserves_archives_and_creates_a_valid_metadata_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    std::fs::create_dir(&data).unwrap();
    let db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data.join("metadata.sqlite"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::raw_sql(concat!(
        include_str!("../src/schema.sql"),
        include_str!("../src/migration-002.sql"),
        include_str!("../src/migration-003.sql")
    ))
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace (id,save_dir,backup_dir,content_dir,created_at) VALUES ('workspace','saves','backups','content','2026-01-01')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES ('task','key','digest','workspace','backup','backup.create','succeeded','finished','2026-01-01')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO backup_archive VALUES ('backup','task','workspace','backup','2026-01-01',0,0,'0','best_effort','verified','2026-01-01','archive-hash','manifest-hash')").execute(&db).await.unwrap();
    db.close().await;
    let hub = Hub::open(&data).await.unwrap();
    let dashboard = hub.dashboard().await.unwrap();
    assert_eq!(dashboard.backups[0].archive_sha256, "archive-hash");
    assert!(dashboard.restore_tasks.is_empty());
    let snapshot = std::fs::read_dir(&data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("metadata-v3-")
        })
        .unwrap();
    let db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(snapshot)
            .read_only(true),
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&db)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&db)
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT archive_sha256 FROM backup_archive")
            .fetch_one(&db)
            .await
            .unwrap(),
        "archive-hash"
    );
}
#[tokio::test]
async fn failed_schema4_migration_rolls_back_without_changing_schema3_records() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    std::fs::create_dir(&data).unwrap();
    let db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data.join("metadata.sqlite"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::raw_sql(concat!(
        include_str!("../src/schema.sql"),
        include_str!("../src/migration-002.sql"),
        include_str!("../src/migration-003.sql")
    ))
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE restore_task (sentinel TEXT)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO restore_task VALUES ('preserve')")
        .execute(&db)
        .await
        .unwrap();
    let error = Hub::open(&data).await.err().unwrap();
    assert_eq!(error.code, "DB_MIGRATION_FAILED");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&db)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sentinel FROM restore_task")
            .fetch_one(&db)
            .await
            .unwrap(),
        "preserve"
    );
}

#[tokio::test]
async fn restore_preview_revokes_verified_status_when_the_registered_archive_is_missing() {
    let f = Fixture::new().await;
    let backup = f.create().await;
    let previous = f.hub.dashboard().await.unwrap().backups[0].clone();
    std::fs::remove_file(f.backups.join(&backup.backup_id).join("archive.tar.zst")).unwrap();
    assert_eq!(
        f.hub
            .prepare_restore(&backup.backup_id, f._temp.path().to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "BACKUP_IO_FAILED"
    );
    let current = f.hub.dashboard().await.unwrap().backups[0].clone();
    assert_eq!(current.verification, "unavailable");
    assert_eq!(current.verified_at, previous.verified_at);
    assert!(f.hub.dashboard().await.unwrap().restore_tasks.is_empty());
}

#[tokio::test]
async fn directory_change_cannot_detach_existing_backup_history() {
    let f = Fixture::new().await;
    std::fs::write(f.source.join("world"), b"save").unwrap();
    assert_eq!(f.create().await.state, "succeeded");
    let w = f.hub.dashboard().await.unwrap().workspace.unwrap();
    let old = WorkspaceInput {
        save_dir: w.save_dir,
        backup_dir: w.backup_dir,
        content_dir: w.content_dir,
    };
    let new = f._temp.path().join("new-backups");
    std::fs::create_dir(&new).unwrap();
    let mut input = old.clone();
    input.backup_dir = new.to_str().unwrap().into();
    assert_eq!(
        f.hub
            .update_workspace(old.clone(), input, f._temp.path().to_str().unwrap().into())
            .await
            .unwrap_err()
            .code,
        "BACKUP_ROOT_IN_USE"
    );
    assert_eq!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .workspace
            .unwrap()
            .backup_dir,
        old.backup_dir
    );
}
