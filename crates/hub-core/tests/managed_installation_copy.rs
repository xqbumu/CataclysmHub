use hub_core::{Hub, InstallationCopyTask, ManagedInstallationCopyPlan, WorkspaceInput};
use std::{path::PathBuf, sync::Arc};
struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    source_id: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for dir in ["saves", "backups", "content", "game", "copies"] {
            std::fs::create_dir(temp.path().join(dir)).unwrap();
        }
        std::fs::create_dir(temp.path().join("game/data")).unwrap();
        std::fs::write(
            temp.path().join("game/test.exe"),
            b"synthetic executable, never run",
        )
        .unwrap();
        std::fs::write(temp.path().join("game/data/cache"), b"source cache").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                temp.path().join("game/test.exe"),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
        }
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let text = |name: &str| temp.path().join(name).to_str().unwrap().to_owned();
        hub.initialize(WorkspaceInput {
            save_dir: text("saves"),
            backup_dir: text("backups"),
            content_dir: text("content"),
        })
        .await
        .unwrap();
        let source = hub
            .import_installation(&text("game/test.exe"), "Source")
            .await
            .unwrap();
        Self {
            temp,
            hub,
            source_id: source.id,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }
    async fn plan(&self) -> ManagedInstallationCopyPlan {
        self.hub
            .prepare_managed_installation_copy(
                &self.source_id,
                self.path("game").to_str().unwrap(),
                self.path("copies").to_str().unwrap(),
            )
            .await
            .unwrap()
    }
    async fn start(&self, plan: &ManagedInstallationCopyPlan) -> InstallationCopyTask {
        self.hub
            .start_installation_copy(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap()
    }
    async fn complete(&self) -> InstallationCopyTask {
        let plan = self.plan().await;
        let task = self.start(&plan).await;
        wait(&self.hub, &task.id).await
    }
    async fn pool(&self) -> sqlx::SqlitePool {
        sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            self.path("metadata/metadata.sqlite").display()
        ))
        .await
        .unwrap()
    }
}
async fn wait(hub: &Arc<Hub>, id: &str) -> InstallationCopyTask {
    for _ in 0..3000 {
        let task = hub.installation_copy_task(id).await.unwrap();
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return task;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("copy task did not finish");
}
#[tokio::test]
async fn explicit_copy_is_readonly_until_confirmed_and_registration_is_separate_atomic_and_idempotent()
 {
    let f = Fixture::new().await;
    let before = f.hub.dashboard().await.unwrap().operations.len();
    let plan = f.plan().await;
    assert!(!std::path::Path::new(&plan.destination).exists());
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), before);
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .start_installation_copy(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let task = f
        .hub
        .start_installation_copy(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    let task = wait(&f.hub, &task.id).await;
    assert_eq!(task.state, "succeeded");
    assert_eq!(task.processed_entries, task.entry_count);
    assert!(task.verified_at.is_some());
    assert_eq!(f.hub.dashboard().await.unwrap().installations.len(), 1);
    assert_eq!(
        f.hub
            .start_installation_copy(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        task.id
    );
    assert_eq!(
        f.hub
            .start_installation_copy(&plan.id, "different", &key, true)
            .await
            .unwrap_err()
            .code,
        "REQUEST_CONFLICT"
    );
    let copied = f
        .hub
        .register_copied_installation(&task.id, "Copy")
        .await
        .unwrap();
    assert_ne!(copied.id, f.source_id);
    assert_eq!(copied.sha256, plan.executable_sha256);
    assert!(copied.executable.starts_with(&plan.destination));
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(data.installations.len(), 2);
    assert!(data.sessions.is_empty());
    assert!(data.play_instances.is_empty());
    let count = data.operations.len();
    assert_eq!(
        f.hub
            .register_copied_installation(&task.id, "Copy")
            .await
            .unwrap()
            .id,
        copied.id
    );
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), count);
    assert_eq!(
        f.hub
            .register_copied_installation(&task.id, "Renamed")
            .await
            .unwrap_err()
            .code,
        "COPY_ALREADY_REGISTERED"
    );
    assert_eq!(
        std::fs::read(f.path("game/data/cache")).unwrap(),
        b"source cache"
    );
}
#[tokio::test]
async fn registration_rechecks_payload_and_pins_the_original_confirmed_manifest() {
    for forge_manifest in [false, true] {
        let f = Fixture::new().await;
        let task = f.complete().await;
        assert_eq!(task.state, "succeeded");
        let root = PathBuf::from(&task.destination);
        if forge_manifest {
            use sha2::{Digest, Sha256};
            let mut manifest = std::fs::read(root.join("copy-manifest.json")).unwrap();
            manifest.push(b'\n');
            std::fs::write(root.join("copy-manifest.json"), &manifest).unwrap();
            let path = root.join("copy-verification.json");
            let mut receipt: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            receipt["manifest_sha256"] =
                serde_json::json!(format!("{:x}", Sha256::digest(&manifest)));
            std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
            hub_core::verify_installation_copy(&root).unwrap();
        } else {
            std::fs::write(
                root.join(&task.payload_root).join("data/cache"),
                b"changed data",
            )
            .unwrap();
        }
        assert!(
            f.hub
                .register_copied_installation(&task.id, "Copy")
                .await
                .is_err()
        );
        assert_eq!(f.hub.dashboard().await.unwrap().installations.len(), 1);
        assert!(
            f.hub
                .installation_copy_task(&task.id)
                .await
                .unwrap()
                .registered_installation_id
                .is_none()
        );
    }
}
#[tokio::test]
async fn intent_audit_failure_writes_no_copy_and_registration_audit_failure_rolls_back_both_records()
 {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    let pool = f.pool().await;
    sqlx::query("CREATE TRIGGER reject_copy BEFORE INSERT ON operation_log WHEN NEW.action='installation.copy' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    assert!(
        f.hub
            .start_installation_copy(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .is_err()
    );
    assert!(!std::path::Path::new(&plan.destination).exists());
    assert!(f.hub.dashboard().await.unwrap().copy_tasks.is_empty());
    sqlx::query("DROP TRIGGER reject_copy")
        .execute(&pool)
        .await
        .unwrap();
    let task = f.complete().await;
    assert_eq!(task.state, "succeeded");
    let before = f.hub.dashboard().await.unwrap().operations.len();
    sqlx::query("CREATE TRIGGER reject_registration BEFORE INSERT ON operation_log WHEN NEW.action='installation.copy_register' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    assert!(
        f.hub
            .register_copied_installation(&task.id, "Copy")
            .await
            .is_err()
    );
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(data.installations.len(), 1);
    assert_eq!(data.operations.len(), before);
    assert!(data.copy_tasks[0].registered_installation_id.is_none());
    sqlx::query("DROP TRIGGER reject_registration")
        .execute(&pool)
        .await
        .unwrap();
    f.hub
        .register_copied_installation(&task.id, "Copy")
        .await
        .unwrap();
}
#[tokio::test]
async fn pinned_source_and_parent_directory_replacement_are_refused() {
    for name in ["game", "copies"] {
        let f = Fixture::new().await;
        let plan = f.plan().await;
        std::fs::rename(f.path(name), f.path(&format!("old-{name}"))).unwrap();
        std::fs::create_dir(f.path(name)).unwrap();
        if name == "game" {
            std::fs::create_dir(f.path("game/data")).unwrap();
            std::fs::copy(f.path("old-game/test.exe"), f.path("game/test.exe")).unwrap();
            std::fs::copy(f.path("old-game/data/cache"), f.path("game/data/cache")).unwrap();
        }
        let task = f.start(&plan).await;
        let task = wait(&f.hub, &task.id).await;
        assert_eq!(task.state, "failed");
        assert_eq!(
            task.error_code.as_deref(),
            Some("INSTALLATION_SOURCE_CHANGED")
        );
        assert!(!std::path::Path::new(&plan.destination).exists());
    }
}
#[tokio::test]
async fn scopes_and_unknown_sessions_cannot_bypass_the_backend_gate() {
    let f = Fixture::new().await;
    for name in ["saves", "backups", "content", "metadata", "game"] {
        assert!(
            f.hub
                .prepare_managed_installation_copy(
                    &f.source_id,
                    f.path("game").to_str().unwrap(),
                    f.path(name).to_str().unwrap()
                )
                .await
                .is_err()
        );
    }
    assert!(
        f.hub
            .prepare_managed_installation_copy(
                &f.source_id,
                f.temp.path().to_str().unwrap(),
                f.path("copies").to_str().unwrap()
            )
            .await
            .is_err()
    );
    let plan = f.plan().await;
    let pool = f.pool().await;
    sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('unknown',?,'interrupted_unknown','now')").bind(&f.source_id).execute(&pool).await.unwrap();
    assert_eq!(
        f.hub
            .start_installation_copy(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert!(!std::path::Path::new(&plan.destination).exists());
    f.hub.acknowledge_interrupted("unknown").await.unwrap();
    let task = f.start(&plan).await;
    assert_eq!(wait(&f.hub, &task.id).await.state, "succeeded");
}
#[tokio::test]
async fn terminal_persistence_failure_releases_ended_claims_without_replaying_copy() {
    let f = Fixture::new().await;
    let pool = f.pool().await;
    sqlx::query("CREATE TRIGGER reject_terminal_copy BEFORE UPDATE OF state ON installation_copy_task WHEN NEW.state!=OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    let plan = f.plan().await;
    let task = f.start(&plan).await;
    let task = wait(&f.hub, &task.id).await;
    assert_eq!(task.state, "recovery_required");
    assert_eq!(
        task.error_code.as_deref(),
        Some("TASK_RESULT_PERSIST_FAILED")
    );
    assert!(
        !f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.owner_id == task.id)
    );
    f.hub.prepare_backup().await.unwrap();
    assert_eq!(
        f.hub
            .register_copied_installation(&task.id, "Copy")
            .await
            .unwrap_err()
            .code,
        "TASK_RESULT_PERSIST_FAILED"
    );
    sqlx::query("DROP TRIGGER reject_terminal_copy")
        .execute(&pool)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&task.id).await.unwrap();
    f.hub
        .register_copied_installation(&task.id, "Copy")
        .await
        .unwrap();
    assert_eq!(
        f.hub.installation_copy_task(&task.id).await.unwrap().state,
        "recovery_required"
    );
    assert_eq!(std::fs::read_dir(f.path("copies")).unwrap().count(), 1);
}
#[tokio::test]
async fn restart_keeps_interruption_history_and_never_replays_or_auto_registers_a_complete_artifact()
 {
    let f = Fixture::new().await;
    let task = f.complete().await;
    assert_eq!(task.state, "succeeded");
    let pool = f.pool().await;
    sqlx::query("UPDATE installation_copy_task SET state='running',verified_at=NULL WHERE id=?")
        .bind(&task.id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    for _ in 0..1000 {
        if Arc::strong_count(&f.hub) == 1 {
            break;
        }
        tokio::task::yield_now().await;
    }
    let data = f.path("metadata");
    drop(f.hub);
    let hub = Hub::open(&data).await.unwrap();
    let recovered = hub.installation_copy_task(&task.id).await.unwrap();
    assert_eq!(recovered.state, "recovery_required");
    assert_eq!(recovered.error_code.as_deref(), Some("TASK_INTERRUPTED"));
    assert_eq!(hub.dashboard().await.unwrap().installations.len(), 1);
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("copies"))
            .unwrap()
            .count(),
        1
    );
    hub.register_copied_installation(&task.id, "Recovered Copy")
        .await
        .unwrap();
    let result = hub.installation_copy_task(&task.id).await.unwrap();
    assert_eq!(result.state, "recovery_required");
    assert!(result.registered_installation_id.is_some());
}
#[tokio::test]
async fn cancellation_has_a_serialized_boundary_and_never_registers_automatically() {
    let f = Fixture::new().await;
    for n in 0..120 {
        std::fs::write(f.path(&format!("game/data/{n:03}")), [n as u8; 1024]).unwrap();
    }
    let plan = f.plan().await;
    let task = f.start(&plan).await;
    match f.hub.cancel_installation_copy(&task.id).await {
        Ok(request) => {
            let ended = wait(&f.hub, &task.id).await;
            if request.state == "cancelling" {
                assert_eq!(ended.state, "cancelled");
                assert!(
                    hub_core::verify_installation_copy(std::path::Path::new(&task.destination))
                        .is_err()
                );
            } else {
                assert_eq!(ended.state, "succeeded");
            }
        }
        Err(error) => {
            assert_eq!(error.code, "COPY_COMMITTING");
            assert_eq!(wait(&f.hub, &task.id).await.state, "succeeded");
        }
    }
    assert_eq!(f.hub.dashboard().await.unwrap().installations.len(), 1);
    assert!(
        f.hub
            .installation_copy_task(&task.id)
            .await
            .unwrap()
            .registered_installation_id
            .is_none()
    );
}

#[tokio::test]
async fn schema_nine_upgrade_keeps_snapshot_and_failed_schema_ten_rolls_back_created_table() {
    for fail in [false, true] {
        let f = Fixture::new().await;
        let data = f.path("metadata");
        let pool = f.pool().await;
        sqlx::query("DROP TABLE installation_copy_task")
            .execute(&pool)
            .await
            .unwrap();
        if fail {
            sqlx::raw_sql("CREATE TABLE collision (id TEXT); CREATE INDEX installation_copy_created ON collision(id);").execute(&pool).await.unwrap();
        }
        sqlx::query("PRAGMA user_version=9")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        drop(f.hub);
        let opened = Hub::open(&data).await;
        if fail {
            assert_eq!(opened.err().unwrap().code, "DB_MIGRATION_FAILED");
        } else {
            let hub = opened.unwrap();
            assert!(hub.dashboard().await.unwrap().copy_tasks.is_empty());
            assert_eq!(hub.dashboard().await.unwrap().installations.len(), 1);
            drop(hub);
        }
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            data.join("metadata.sqlite").display()
        ))
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&pool)
                .await
                .unwrap(),
            if fail { 9 } else { 10 }
        );
        if fail {
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='installation_copy_task'").fetch_one(&pool).await.unwrap(),0);
        }
        assert!(std::fs::read_dir(&data).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("metadata-v9-")
        }));
    }
}
#[tokio::test]
async fn app_source_is_narrowed_from_authorized_parent_and_partial_bundle_selection_is_rejected() {
    let f = Fixture::new().await;
    let bundle = f.path("Bundle.app");
    std::fs::create_dir_all(bundle.join("Contents/Resources")).unwrap();
    let exe = bundle.join("Contents/Resources/test.exe");
    std::fs::copy(f.path("game/test.exe"), &exe).unwrap();
    let installation = f
        .hub
        .import_installation(exe.to_str().unwrap(), "Bundle")
        .await
        .unwrap();
    let plan = f
        .hub
        .prepare_managed_installation_copy(
            &installation.id,
            f.temp.path().to_str().unwrap(),
            f.path("copies").to_str().unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        plan.source_root,
        std::fs::canonicalize(&bundle).unwrap().to_str().unwrap()
    );
    assert_eq!(plan.executable_relative, "Contents/Resources/test.exe");
    assert_eq!(
        f.hub
            .prepare_managed_installation_copy(
                &installation.id,
                bundle.join("Contents/Resources").to_str().unwrap(),
                f.path("copies").to_str().unwrap()
            )
            .await
            .unwrap_err()
            .code,
        "COPY_SOURCE_ROOT_INCOMPLETE"
    );
}

#[tokio::test]
async fn ignored_registration_update_cannot_commit_an_orphan_installation() {
    let f = Fixture::new().await;
    let task = f.complete().await;
    let pool = f.pool().await;
    let count = f.hub.dashboard().await.unwrap().operations.len();
    sqlx::query("CREATE TRIGGER ignore_registration BEFORE UPDATE OF registered_installation_id ON installation_copy_task BEGIN SELECT RAISE(IGNORE); END").execute(&pool).await.unwrap();
    assert_eq!(
        f.hub
            .register_copied_installation(&task.id, "Copy")
            .await
            .unwrap_err()
            .code,
        "COPY_RECORD_CHANGED"
    );
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(data.installations.len(), 1);
    assert_eq!(data.operations.len(), count);
    assert!(data.copy_tasks[0].registered_installation_id.is_none());
}
