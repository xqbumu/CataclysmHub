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
    for _ in 0..500 {
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
