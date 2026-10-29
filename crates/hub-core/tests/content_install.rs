use hub_core::{ContentInstallPlan, ContentInstallTask, Hub};
use std::{path::PathBuf, sync::Arc};
struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    entry: String,
    source: PathBuf,
    parent: PathBuf,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let parent = temp.path().join("library");
        std::fs::create_dir(&source).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::write(
            source.join("soundpack.txt"),
            b"NAME: example\nVIEW: Example\n",
        )
        .unwrap();
        std::fs::create_dir(source.join("audio")).unwrap();
        std::fs::write(source.join("audio/sample.ogg"), b"synthetic audio bytes").unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let entry = hub
            .register_content(source.to_str().unwrap(), "soundpack")
            .await
            .unwrap()
            .id;
        Self {
            temp,
            hub,
            entry,
            source,
            parent,
        }
    }
    async fn plan(&self) -> ContentInstallPlan {
        self.hub
            .prepare_content_install(&self.entry, self.parent.to_str().unwrap())
            .await
            .unwrap()
    }
    async fn wait(&self, id: &str) -> ContentInstallTask {
        for _ in 0..500 {
            let row = self
                .hub
                .content_install_tasks()
                .await
                .unwrap()
                .into_iter()
                .find(|t| t.id == id)
                .unwrap();
            if !matches!(row.state.as_str(), "running" | "cancelling") {
                return row;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("task did not finish");
    }
    async fn pool(&self) -> sqlx::SqlitePool {
        sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            self.temp.path().join("metadata/metadata.sqlite").display()
        ))
        .await
        .unwrap()
    }
}
#[tokio::test]
async fn install_is_confirmed_idempotent_verified_and_source_preserving() {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    let key = uuid::Uuid::new_v4().to_string();
    assert!(!std::path::Path::new(&plan.destination).exists());
    assert_eq!(
        f.hub
            .start_content_install(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let id = f
        .hub
        .start_content_install(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .start_content_install(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap(),
        id
    );
    assert_eq!(f.wait(&id).await.state, "succeeded");
    assert_eq!(
        std::fs::read(PathBuf::from(&plan.payload_root).join("audio/sample.ogg")).unwrap(),
        b"synthetic audio bytes"
    );
    assert_eq!(
        std::fs::read(f.source.join("audio/sample.ogg")).unwrap(),
        b"synthetic audio bytes"
    );
    assert!(
        !PathBuf::from(&plan.destination)
            .join("CONTENT_INSTALL_INCOMPLETE")
            .exists()
    );
    let manifest =
        std::fs::read(PathBuf::from(&plan.destination).join("content-manifest.json")).unwrap();
    use sha2::{Digest, Sha256};
    assert_eq!(
        format!("{:x}", Sha256::digest(manifest)),
        plan.manifest_sha256
    );
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
    assert_eq!(f.hub.content_entries().await.unwrap().len(), 1);
}
#[tokio::test]
async fn changed_source_and_existing_target_never_publish_or_overwrite() {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    std::fs::write(f.source.join("audio/sample.ogg"), b"changed").unwrap();
    let task = f
        .hub
        .start_content_install(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        f.wait(&task).await.error_code.as_deref(),
        Some("CONTENT_SOURCE_CHANGED")
    );
    assert!(!PathBuf::from(&plan.destination).exists());
    let plan = f.plan().await;
    std::fs::create_dir(&plan.destination).unwrap();
    std::fs::write(PathBuf::from(&plan.destination).join("keep"), b"keep").unwrap();
    let task = f
        .hub
        .start_content_install(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(f.wait(&task).await.state, "failed");
    assert_eq!(
        std::fs::read(PathBuf::from(&plan.destination).join("keep")).unwrap(),
        b"keep"
    );
    assert!(!PathBuf::from(&plan.payload_root).exists());
}
#[tokio::test]
async fn replaced_parent_and_active_game_are_rejected_before_admission() {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    std::fs::rename(&f.parent, f.temp.path().join("old-library")).unwrap();
    std::fs::create_dir(&f.parent).unwrap();
    assert_eq!(
        f.hub
            .start_content_install(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .unwrap_err()
            .code,
        "CONTENT_SOURCE_CHANGED"
    );
    let pool = f.pool().await;
    sqlx::raw_sql("INSERT INTO installation VALUES ('i','game','/game/exe','/game','hash',NULL,NULL,'test','test','now'); INSERT INTO session(id,installation_id,state,started_at) VALUES('s','i','interrupted_unknown','now');").execute(&pool).await.unwrap();
    assert_eq!(
        f.hub
            .prepare_content_install(&f.entry, f.parent.to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert!(f.hub.content_install_tasks().await.unwrap().is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn package_links_are_rejected_before_any_target_creation() {
    let f = Fixture::new().await;
    std::os::unix::fs::symlink("sample.ogg", f.source.join("audio/alias.ogg")).unwrap();
    assert_eq!(
        f.hub
            .prepare_content_install(&f.entry, f.parent.to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "CONTENT_LINK_UNSUPPORTED"
    );
    assert_eq!(std::fs::read_dir(&f.parent).unwrap().count(), 0);
}
#[tokio::test]
async fn audit_failure_does_not_spawn_and_terminal_failure_never_replays_copy() {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    let pool = f.pool().await;
    sqlx::query("CREATE TRIGGER reject_start BEFORE INSERT ON operation_log WHEN NEW.action='content.install' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    assert!(
        f.hub
            .start_content_install(&plan.id, &plan.plan_digest, &key, true)
            .await
            .is_err()
    );
    assert!(!PathBuf::from(&plan.destination).exists());
    assert!(f.hub.content_install_tasks().await.unwrap().is_empty());
    sqlx::query("DROP TRIGGER reject_start")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER reject_end BEFORE UPDATE OF state ON content_install_task WHEN NEW.state!=OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    let id = f
        .hub
        .start_content_install(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(f.wait(&id).await.state, "recovery_required");
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
    let receipt = PathBuf::from(&plan.destination).join("content-verification.json");
    let before = std::fs::read(&receipt).unwrap();
    sqlx::query("DROP TRIGGER reject_end")
        .execute(&pool)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&id).await.unwrap();
    assert_eq!(std::fs::read(receipt).unwrap(), before);
    assert_eq!(
        f.hub.content_install_tasks().await.unwrap()[0].state,
        "recovery_required"
    );
}
#[tokio::test]
async fn restart_marks_abandoned_task_without_replaying_io() {
    let f = Fixture::new().await;
    let plan = f.plan().await;
    let id = f
        .hub
        .start_content_install(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    f.wait(&id).await;
    let pool = f.pool().await;
    sqlx::query("UPDATE content_install_task SET state='running' WHERE id=?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let receipt = PathBuf::from(&plan.destination).join("content-verification.json");
    let before = std::fs::read(&receipt).unwrap();
    drop(f.hub);
    let hub = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    assert_eq!(
        hub.content_install_tasks().await.unwrap()[0].state,
        "recovery_required"
    );
    assert_eq!(std::fs::read(receipt).unwrap(), before);
}

#[tokio::test]
async fn targets_inside_another_registered_package_are_rejected() {
    let f = Fixture::new().await;
    std::fs::write(f.parent.join("soundpack.txt"), "NAME: other\nVIEW: Other\n").unwrap();
    f.hub
        .register_content(f.parent.to_str().unwrap(), "soundpack")
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .prepare_content_install(&f.entry, f.parent.to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "CONTENT_SCOPE_OVERLAP"
    );
    assert_eq!(std::fs::read_dir(&f.parent).unwrap().count(), 1);
}

#[tokio::test]
async fn schema_eleven_upgrade_preserves_catalog_and_rolls_back_on_collision() {
    for fail in [false, true] {
        let f = Fixture::new().await;
        let pool = f.pool().await;
        if !fail {
            sqlx::raw_sql("DROP TABLE hub_preferences; DROP TABLE config_change; DROP TABLE content_install_task")
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("ALTER TABLE workspace DROP COLUMN root_dir")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA user_version=11")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        drop(f.hub);
        let opened = Hub::open(&f.temp.path().join("metadata")).await;
        if fail {
            assert_eq!(opened.err().unwrap().code, "DB_MIGRATION_FAILED");
        } else {
            assert_eq!(
                opened.unwrap().content_entries().await.unwrap()[0].id,
                f.entry
            );
        }
        let pool = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            f.temp.path().join("metadata/metadata.sqlite").display()
        ))
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&pool)
                .await
                .unwrap(),
            if fail { 11 } else { 16 }
        );
        assert!(
            std::fs::read_dir(f.temp.path().join("metadata"))
                .unwrap()
                .any(|p| p
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("metadata-v11-"))
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Fixture {
    async fn instance(&self, id: &str) -> PathBuf {
        use sha2::{Digest, Sha256};
        let game = self.temp.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("game"), b"synthetic not executed").unwrap();
        let game = std::fs::canonicalize(game).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"synthetic not executed"));
        let root = self.temp.path().join(id);
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let pool = self.pool().await;
        sqlx::query("INSERT OR IGNORE INTO installation VALUES('game','Game',?,?,?,NULL,NULL,'test','test','now')").bind(game.join("game").to_str().unwrap()).bind(game.to_str().unwrap()).bind(&hash).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO play_instance VALUES(?,'game',?,?,?,'unverified','now')")
            .bind(id)
            .bind(hash)
            .bind(id)
            .bind(root.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        root
    }
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn deployment_publishes_complete_soundpack_only_to_selected_instance_and_is_idempotent() {
    let f = Fixture::new().await;
    let root = f.instance("a").await;
    let other = f.instance("b").await;
    let deployment = f
        .hub
        .prepare_instance_content_install(&f.entry, "a")
        .await
        .unwrap();
    let plan = &deployment.plan;
    assert_eq!(deployment.instance_id, "a");
    assert!(!root.join("sound").exists());
    assert!(PathBuf::from(&plan.payload_root).starts_with(root.join("sound")));
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .start_content_install(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    assert!(!PathBuf::from(&plan.destination).exists());
    let task = f
        .hub
        .start_content_install(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(f.wait(&task).await.state, "succeeded");
    assert_eq!(
        f.hub
            .start_content_install(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap(),
        task
    );
    let payload = PathBuf::from(&plan.payload_root);
    assert_eq!(
        std::fs::read(payload.join("soundpack.txt")).unwrap(),
        std::fs::read(f.source.join("soundpack.txt")).unwrap()
    );
    assert_eq!(
        std::fs::read(payload.join("audio/sample.ogg")).unwrap(),
        b"synthetic audio bytes"
    );
    assert!(!PathBuf::from(&plan.destination).join("data").exists());
    assert!(!other.join("sound").exists());
    assert!(!root.join("config").exists());
    let report = f.hub.inspect_content_config("a").await.unwrap();
    assert!(
        report
            .discovery
            .packages
            .iter()
            .any(|p| p.path == plan.payload_root && p.kind == "soundpack")
    );
    assert_eq!(
        f.hub
            .prepare_instance_content_install(&f.entry, "a")
            .await
            .unwrap_err()
            .code,
        "CONTENT_ID_CONFLICT"
    );
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn deployment_rechecks_duplicate_ids_and_refuses_active_sessions() {
    let f = Fixture::new().await;
    let root = f.instance("a").await;
    let plan = f
        .hub
        .prepare_instance_content_install(&f.entry, "a")
        .await
        .unwrap()
        .plan;
    std::fs::create_dir_all(root.join("sound/existing")).unwrap();
    std::fs::copy(
        f.source.join("soundpack.txt"),
        root.join("sound/existing/soundpack.txt"),
    )
    .unwrap();
    assert_eq!(
        f.hub
            .start_content_install(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .unwrap_err()
            .code,
        "CONTENT_ID_CONFLICT"
    );
    assert!(!PathBuf::from(&plan.destination).exists());
    std::fs::remove_dir_all(root.join("sound")).unwrap();
    let pool = f.pool().await;
    sqlx::query("INSERT INTO session(id,installation_id,state,started_at,play_instance_id) VALUES('active','game','running','now','a')").execute(&pool).await.unwrap();
    assert!(
        f.hub
            .start_content_install(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .is_err()
    );
    assert!(!PathBuf::from(&plan.destination).exists());
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn deployment_never_overwrites_existing_output_and_keeps_failed_staging_outside_discovery() {
    let f = Fixture::new().await;
    f.instance("a").await;
    let plan = f
        .hub
        .prepare_instance_content_install(&f.entry, "a")
        .await
        .unwrap()
        .plan;
    std::fs::create_dir_all(&plan.payload_root).unwrap();
    std::fs::write(PathBuf::from(&plan.payload_root).join("sentinel"), b"keep").unwrap();
    let task = f
        .hub
        .start_content_install(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(f.wait(&task).await.state, "failed");
    assert_eq!(
        std::fs::read(PathBuf::from(&plan.payload_root).join("sentinel")).unwrap(),
        b"keep"
    );
    assert!(
        !PathBuf::from(&plan.payload_root)
            .join("soundpack.txt")
            .exists()
    );
    assert!(
        PathBuf::from(&plan.destination)
            .join("CONTENT_INSTALL_INCOMPLETE")
            .exists()
    );
    assert!(
        f.hub
            .inspect_content_config("a")
            .await
            .unwrap()
            .discovery
            .packages
            .is_empty()
    );
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn deployment_refuses_linked_role_and_replaced_program() {
    let f = Fixture::new().await;
    let root = f.instance("a").await;
    std::os::unix::fs::symlink(&f.parent, root.join("sound")).unwrap();
    assert!(
        f.hub
            .prepare_instance_content_install(&f.entry, "a")
            .await
            .is_err()
    );
    std::fs::remove_file(root.join("sound")).unwrap();
    let plan = f
        .hub
        .prepare_instance_content_install(&f.entry, "a")
        .await
        .unwrap()
        .plan;
    std::fs::write(f.temp.path().join("game/game"), b"changed").unwrap();
    assert!(
        f.hub
            .start_content_install(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .is_err()
    );
    assert!(!PathBuf::from(&plan.destination).exists());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn mod_deployment_preserves_worlds_and_default_configuration() {
    let f = Fixture::new().await;
    let root = f.instance("a").await;
    let source = f.temp.path().join("mod");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        source.join("modinfo.json"),
        br#"[{"type":"MOD_INFO","id":"extra","name":"Extra","dependencies":[]}]"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("save/world")).unwrap();
    std::fs::write(root.join("save/world/mods.json"), b"[\"dda\"]").unwrap();
    std::fs::create_dir(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/user-default-mods.json"),
        b"{\"dependencies\":[\"dda\"]}",
    )
    .unwrap();
    let entry = f
        .hub
        .register_content(source.to_str().unwrap(), "mod")
        .await
        .unwrap();
    let plan = f
        .hub
        .prepare_instance_content_install(&entry.id, "a")
        .await
        .unwrap()
        .plan;
    let task = f
        .hub
        .start_content_install(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(f.wait(&task).await.state, "succeeded");
    assert!(PathBuf::from(&plan.payload_root).starts_with(root.join("mods")));
    assert_eq!(
        std::fs::read(root.join("save/world/mods.json")).unwrap(),
        b"[\"dda\"]"
    );
    assert_eq!(
        std::fs::read(root.join("config/user-default-mods.json")).unwrap(),
        b"{\"dependencies\":[\"dda\"]}"
    );
    assert!(
        f.hub
            .inspect_content_config("a")
            .await
            .unwrap()
            .discovery
            .packages
            .iter()
            .any(|p| p.path == plan.payload_root && p.kind == "mod")
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn deployment_rejects_nested_resource_roots_before_creating_staging() {
    let f = Fixture::new().await;
    let root = f.instance("a").await;
    std::fs::create_dir(f.source.join("nested")).unwrap();
    std::fs::copy(
        f.source.join("soundpack.txt"),
        f.source.join("nested/soundpack.txt"),
    )
    .unwrap();
    assert_eq!(
        f.hub
            .prepare_instance_content_install(&f.entry, "a")
            .await
            .unwrap_err()
            .code,
        "CONTENT_NESTED_PACKAGE_UNSUPPORTED"
    );
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 0);
}
