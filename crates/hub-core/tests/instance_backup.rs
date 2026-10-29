use hub_core::{
    BackupTask, Hub, PlayInstanceInput, WorkspaceInput, restore_backup_to_new_directory,
    verify_backup_directory,
};
use std::{path::PathBuf, sync::Arc};

struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    a: String,
    b: String,
    installation: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for dir in ["workspace-save", "backups", "content", "game", "a", "b"] {
            std::fs::create_dir(temp.path().join(dir)).unwrap();
        }
        let path = |name: &str| temp.path().join(name).to_str().unwrap().to_owned();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        hub.initialize(WorkspaceInput {
            save_dir: path("workspace-save"),
            backup_dir: path("backups"),
            content_dir: path("content"),
        })
        .await
        .unwrap();
        let executable = temp.path().join("game/test.exe");
        std::fs::write(&executable, b"synthetic executable, never run").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let installation = hub
            .import_installation(executable.to_str().unwrap(), "synthetic installation")
            .await
            .unwrap();
        let mut instances = Vec::new();
        for name in ["a", "b"] {
            let instance = hub
                .register_play_instance(PlayInstanceInput {
                    installation_id: installation.id.clone(),
                    name: name.into(),
                    data_root: path(name),
                })
                .await
                .unwrap();
            instances.push(instance.id);
        }
        std::fs::create_dir(temp.path().join("a/save")).unwrap();
        std::fs::create_dir(temp.path().join("a/config")).unwrap();
        std::fs::write(temp.path().join("a/save/world.dat"), b"instance A world").unwrap();
        std::fs::write(
            temp.path().join("a/config/options.json"),
            b"instance A config",
        )
        .unwrap();
        std::fs::write(temp.path().join("b/only-b.dat"), b"instance B only").unwrap();
        std::fs::write(
            temp.path().join("workspace-save/marker.dat"),
            b"workspace only",
        )
        .unwrap();
        Self {
            temp,
            hub,
            a: instances[0].clone(),
            b: instances[1].clone(),
            installation: installation.id,
        }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }
    async fn create(&self, instance: &str) -> BackupTask {
        let plan = self.hub.prepare_instance_backup(instance).await.unwrap();
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
    async fn pool(&self) -> sqlx::SqlitePool {
        sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            self.path("metadata/metadata.sqlite").display()
        ))
        .await
        .unwrap()
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
    panic!("backup did not finish")
}

#[tokio::test]
async fn instance_archive_is_independent_covers_config_and_save_and_restores_without_mixing_sources()
 {
    let f = Fixture::new().await;
    let audit_before = f.hub.dashboard().await.unwrap().operations.len();
    let plan = f.hub.prepare_instance_backup(&f.a).await.unwrap();
    assert_eq!(plan.play_instance_id.as_deref(), Some(f.a.as_str()));
    assert_eq!(plan.file_count, 2);
    assert!(
        plan.entries
            .iter()
            .any(|e| e.relative_path == "config/options.json")
    );
    assert!(
        !plan
            .entries
            .iter()
            .any(|e| e.relative_path.contains("marker") || e.relative_path.contains("only-b"))
    );
    assert_eq!(
        f.hub.dashboard().await.unwrap().operations.len(),
        audit_before
    );
    assert_eq!(std::fs::read_dir(f.path("backups")).unwrap().count(), 0);
    let key = uuid::Uuid::new_v4().to_string();
    let started = f
        .hub
        .create_backup(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    let task = wait(&f.hub, &started.id).await;
    assert_eq!(task.state, "succeeded");
    assert_eq!(task.play_instance_id.as_deref(), Some(f.a.as_str()));
    assert_eq!(
        f.hub
            .create_backup(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        task.id
    );
    let record = f.hub.dashboard().await.unwrap().backups.remove(0);
    assert_eq!(record.play_instance_id.as_deref(), Some(f.a.as_str()));
    assert_eq!(
        record.source_root.as_deref(),
        Some(plan.source_root.as_str())
    );
    let root = f.path("backups").join(&task.backup_id);
    use std::io::Read;
    let mut archive = tar::Archive::new(
        zstd::stream::read::Decoder::new(
            std::fs::File::open(root.join("archive.tar.zst")).unwrap(),
        )
        .unwrap(),
    );
    let mut manifest_bytes = Vec::new();
    archive
        .entries()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .read_to_end(&mut manifest_bytes)
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["scope"], "instance_data_root");
    assert_eq!(manifest["source_token"], f.a);
    assert_eq!(
        manifest["extensions"]["instance_source"]["installation_id"],
        f.installation
    );
    assert_eq!(
        manifest["extensions"]["instance_source"]["binding_verified"],
        false
    );
    assert!(
        !std::str::from_utf8(&manifest_bytes)
            .unwrap()
            .contains(&plan.source_root)
    );
    verify_backup_directory(&root).unwrap();
    let restored = f.path("restored-instance");
    restore_backup_to_new_directory(&root, &restored).unwrap();
    assert_eq!(
        std::fs::read(restored.join("data/save/world.dat")).unwrap(),
        b"instance A world"
    );
    assert_eq!(
        std::fs::read(restored.join("data/config/options.json")).unwrap(),
        b"instance A config"
    );
    assert!(!restored.join("data/marker.dat").exists());
    assert_eq!(
        std::fs::read(f.path("b/only-b.dat")).unwrap(),
        b"instance B only"
    );
    assert_eq!(
        std::fs::read(f.path("workspace-save/marker.dat")).unwrap(),
        b"workspace only"
    );
    let retention = f
        .hub
        .preview_retention(hub_core::RetentionPolicy {
            keep_recent: 7,
            daily_days: 14,
            weekly_weeks: 8,
        })
        .await
        .unwrap();
    assert!(
        !retention
            .items
            .iter()
            .any(|i| i.backup_id == task.backup_id)
    );
}

#[tokio::test]
async fn distinct_instance_provenance_survives_verification_with_the_source_offline() {
    let f = Fixture::new().await;
    let a = f.create(&f.a).await;
    assert_eq!(a.state, "succeeded");
    let b = f.create(&f.b).await;
    assert_eq!(b.state, "succeeded");
    std::fs::rename(f.path("a"), f.path("offline-a")).unwrap();
    let verify = f
        .hub
        .verify_backup(&a.backup_id, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let verify = wait(&f.hub, &verify.id).await;
    assert_eq!(verify.state, "succeeded");
    assert_eq!(verify.play_instance_id.as_deref(), Some(f.a.as_str()));
    let dashboard = f.hub.dashboard().await.unwrap();
    assert_eq!(
        dashboard
            .backups
            .iter()
            .find(|r| r.id == b.backup_id)
            .unwrap()
            .play_instance_id
            .as_deref(),
        Some(f.b.as_str())
    );
}

#[tokio::test]
async fn source_or_backup_directory_replacement_cannot_reuse_a_plan() {
    for name in ["a", "backups"] {
        let f = Fixture::new().await;
        let plan = f.hub.prepare_instance_backup(&f.a).await.unwrap();
        std::fs::rename(f.path(name), f.path(&format!("old-{name}"))).unwrap();
        std::fs::create_dir(f.path(name)).unwrap();
        if name == "a" {
            for dir in ["save", "config"] {
                std::fs::create_dir(f.path(&format!("a/{dir}"))).unwrap();
            }
            std::fs::copy(f.path("old-a/save/world.dat"), f.path("a/save/world.dat")).unwrap();
            std::fs::copy(
                f.path("old-a/config/options.json"),
                f.path("a/config/options.json"),
            )
            .unwrap();
        }
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
        assert_eq!(std::fs::read_dir(f.path("backups")).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn unknown_sessions_block_creation_and_active_claims_use_the_instance_root() {
    let f = Fixture::new().await;
    let plan = f.hub.prepare_instance_backup(&f.a).await.unwrap();
    let pool = f.pool().await;
    sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('unknown',?,'interrupted_unknown','now')").bind(&f.installation).execute(&pool).await.unwrap();
    assert_eq!(
        f.hub.prepare_instance_backup(&f.b).await.unwrap_err().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub
            .create_backup(
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
    f.hub.acknowledge_interrupted("unknown").await.unwrap();
    sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at,play_instance_id,source_root) SELECT 'active','active','digest',id,'archive','backup.create','running','preflight','now',?,? FROM workspace")
        .bind(&f.a).bind(&plan.source_root).execute(&pool).await.unwrap();
    let claims = f.hub.dashboard().await.unwrap().resource_claims;
    assert!(claims.iter().any(|c| c.owner_id == "active"
        && c.path.as_deref() == Some(plan.source_root.as_str())
        && c.access == "read"));
    assert!(
        !claims
            .iter()
            .any(|c| c.owner_id == "active"
                && c.path.as_deref() == f.path("workspace-save").to_str())
    );
    pool.close().await;
}

#[tokio::test]
async fn schema_eight_upgrade_preserves_records_and_keeps_a_verified_snapshot() {
    let f = Fixture::new().await;
    let data = f.path("metadata");
    let pool = f.pool().await;
    sqlx::raw_sql("ALTER TABLE workspace DROP COLUMN root_dir; DROP TABLE hub_preferences; DROP TABLE config_change; DROP TABLE content_install_task; DROP TABLE content_entry; DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; ALTER TABLE backup_archive DROP COLUMN source_root; PRAGMA user_version=8;").execute(&pool).await.unwrap();
    pool.close().await;
    drop(f.hub);
    let hub = Hub::open(&data).await.unwrap();
    assert_eq!(hub.dashboard().await.unwrap().play_instances.len(), 2);
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
        16
    );
    let snapshot = std::fs::read_dir(&data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("metadata-v8-")
        })
        .unwrap();
    let snapshot = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(snapshot)
            .read_only(true),
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        8
    );
}

#[tokio::test]
async fn failed_schema_nine_migration_rolls_back_earlier_source_columns() {
    let f = Fixture::new().await;
    let data = f.path("metadata");
    let pool = f.pool().await;
    sqlx::raw_sql("ALTER TABLE workspace DROP COLUMN root_dir; DROP TABLE hub_preferences; DROP TABLE config_change; DROP TABLE content_install_task; DROP TABLE content_entry; DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; PRAGMA user_version=8;").execute(&pool).await.unwrap();
    pool.close().await;
    drop(f.hub);
    assert_eq!(
        Hub::open(&data).await.err().unwrap().code,
        "DB_MIGRATION_FAILED"
    );
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
        8
    );
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('backup_task')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        !columns
            .iter()
            .any(|c| c == "play_instance_id" || c == "source_root")
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM play_instance")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn invalid_persisted_instance_identity_is_rejected_before_any_staging_write() {
    let f = Fixture::new().await;
    let pool = f.pool().await;
    sqlx::query("UPDATE play_instance SET id='instance' WHERE id=?")
        .bind(&f.a)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .prepare_instance_backup("instance")
            .await
            .unwrap_err()
            .code,
        "INSTANCE_CONTEXT_INVALID"
    );
    assert_eq!(std::fs::read_dir(f.path("backups")).unwrap().count(), 0);
    assert!(f.hub.dashboard().await.unwrap().backup_tasks.is_empty());
}

fn sound_draft() -> hub_core::ContentConfigInput {
    hub_core::ContentConfigInput {
        default_mods: None,
        soundpack: None,
        sound_enabled: Some(true),
    }
}
#[tokio::test]
async fn configuration_protection_binds_original_bytes_and_restores_them_without_applying_draft() {
    let f = Fixture::new().await;
    let original = b"[ {\"name\":\"SOUND_ENABLED\", \"value\":\"false\", \"custom\": 42} ]\n";
    std::fs::write(f.path("a/config/options.json"), original).unwrap();
    let protection = f
        .hub
        .prepare_content_config_protection(&f.a, sound_draft())
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(f.path("backups")).unwrap().count(), 0);
    assert!(f.hub.dashboard().await.unwrap().backup_tasks.is_empty());
    assert_eq!(
        protection.backup.play_instance_id.as_deref(),
        Some(f.a.as_str())
    );
    let file = &protection.configuration.files[0];
    assert_eq!(
        protection
            .backup
            .entries
            .iter()
            .find(|e| e.relative_path == file.path)
            .unwrap()
            .sha256
            .as_deref(),
        Some(file.before_sha256.as_str())
    );
    assert!(!protection.configuration.write_supported);
    let plan = protection.backup;
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .create_backup(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let started = f
        .hub
        .create_backup(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    let finished = wait(&f.hub, &started.id).await;
    assert_eq!(finished.state, "succeeded");
    assert_eq!(
        f.hub
            .create_backup(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        finished.id
    );
    let archive = f.path("backups").join(&finished.backup_id);
    verify_backup_directory(&archive).unwrap();
    let restored = f.path("protected-restored");
    restore_backup_to_new_directory(&archive, &restored).unwrap();
    assert_eq!(
        std::fs::read(restored.join("data/config/options.json")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(restored.join("data/save/world.dat")).unwrap(),
        b"instance A world"
    );
}

#[tokio::test]
async fn configuration_protection_refuses_stale_bytes_at_creation() {
    let f = Fixture::new().await;
    std::fs::write(
        f.path("a/config/options.json"),
        br#"[{"name":"SOUND_ENABLED","value":"false"}]"#,
    )
    .unwrap();
    let protection = f
        .hub
        .prepare_content_config_protection(&f.a, sound_draft())
        .await
        .unwrap();
    let changed = br#"[{"name":"SOUND_ENABLED","value":"other"}]"#;
    std::fs::write(f.path("a/config/options.json"), changed).unwrap();
    let plan = protection.backup;
    let started = f
        .hub
        .create_backup(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    let failed = wait(&f.hub, &started.id).await;
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.error_code.as_deref(), Some("CONTENT_CONFIG_CHANGED"));
    assert!(f.hub.dashboard().await.unwrap().backups.is_empty());
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        changed
    );
}

#[tokio::test]
async fn configuration_protection_rejects_no_changes_and_unknown_sessions() {
    let f = Fixture::new().await;
    std::fs::write(
        f.path("a/config/options.json"),
        br#"[{"name":"SOUND_ENABLED","value":"false"}]"#,
    )
    .unwrap();
    assert_eq!(
        f.hub
            .prepare_content_config_protection(
                &f.a,
                hub_core::ContentConfigInput {
                    default_mods: None,
                    soundpack: None,
                    sound_enabled: Some(false)
                }
            )
            .await
            .unwrap_err()
            .code,
        "CONTENT_CONFIG_NO_CHANGES"
    );
    let pool = f.pool().await;
    sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('unknown',?,'interrupted_unknown','now')").bind(&f.installation).execute(&pool).await.unwrap();
    assert_eq!(
        f.hub
            .prepare_content_config_protection(&f.a, sound_draft())
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(std::fs::read_dir(f.path("backups")).unwrap().count(), 0);
}

async fn config_ready(f: &Fixture) -> (hub_core::ConfigChangePlan, Vec<u8>) {
    let original = br#"[{"name":"SOUND_ENABLED","value":"false","extra":42}]"#.to_vec();
    std::fs::write(f.path("a/config/options.json"), &original).unwrap();
    let backup = f.create(&f.a).await;
    assert_eq!(backup.state, "succeeded");
    (
        f.hub
            .prepare_config_change(&f.a, sound_draft(), &backup.backup_id)
            .await
            .unwrap(),
        original,
    )
}
async fn config_wait(hub: &Hub, id: &str) -> hub_core::ConfigChange {
    for _ in 0..1000 {
        let row = hub
            .config_changes()
            .await
            .unwrap()
            .into_iter()
            .find(|r| r.id == id)
            .unwrap();
        if row.state != "applying" {
            return row;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("config change did not finish")
}
#[tokio::test]
async fn single_config_apply_requires_confirmation_and_never_replays_request() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
    assert!(f.hub.config_changes().await.unwrap().is_empty());
    let key = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        f.hub
            .apply_config_change(&plan.id, &plan.plan_digest, &key, false)
            .await
            .unwrap_err()
            .code,
        "CONFIRMATION_REQUIRED"
    );
    let started = f
        .hub
        .apply_config_change(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &started.id).await.state, "applied");
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        plan.proposed_json.as_bytes()
    );
    let before = std::fs::metadata(f.path("a/config/options.json"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        f.hub
            .apply_config_change(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .state,
        "applied"
    );
    assert_eq!(
        std::fs::metadata(f.path("a/config/options.json"))
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
    assert_eq!(
        std::fs::read(f.path("a/save/world.dat")).unwrap(),
        b"instance A world"
    );
    let restored = f.path("before-config-apply");
    restore_backup_to_new_directory(&f.path("backups").join(&plan.backup_id), &restored).unwrap();
    assert_eq!(
        std::fs::read(restored.join("data/config/options.json")).unwrap(),
        original
    );
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
}
#[tokio::test]
async fn replaced_config_identity_preserves_protection_and_requires_explicit_resolution() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    std::fs::rename(f.path("a/config/options.json"), f.path("old-options.json")).unwrap();
    std::fs::write(f.path("a/config/options.json"), &original).unwrap();
    let task = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        config_wait(&f.hub, &task.id).await.state,
        "recovery_required"
    );
    assert!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.owner_kind == "config_change")
    );
    let observation = f.hub.inspect_config_change(&task.id).await.unwrap();
    assert_eq!(observation.matches, "original");
    assert!(
        f.hub
            .resolve_config_change(&task.id, &observation.current_sha256, false)
            .await
            .is_err()
    );
    std::fs::write(f.path("a/config/options.json"), b"external").unwrap();
    assert!(
        f.hub
            .resolve_config_change(&task.id, &observation.current_sha256, true)
            .await
            .is_err()
    );
    assert_eq!(
        f.hub.inspect_config_change(&task.id).await.unwrap().matches,
        "other"
    );
    std::fs::write(f.path("a/config/options.json"), &original).unwrap();
    assert_eq!(
        f.hub
            .resolve_config_change(&task.id, &observation.current_sha256, true)
            .await
            .unwrap()
            .change
            .state,
        "resolved_original"
    );
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
}
#[tokio::test]
async fn archive_changes_after_prepare_block_config_publication() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    std::fs::write(
        f.path("backups")
            .join(&plan.backup_id)
            .join("archive.tar.zst"),
        b"tampered",
    )
    .unwrap();
    let task = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        config_wait(&f.hub, &task.id).await.state,
        "recovery_required"
    );
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
}
#[tokio::test]
async fn ignored_intent_or_failed_audit_never_starts_config_io() {
    for trigger in [
        "CREATE TRIGGER reject_config BEFORE INSERT ON config_change BEGIN SELECT RAISE(IGNORE); END",
        "CREATE TRIGGER reject_config BEFORE INSERT ON operation_log WHEN NEW.action='config.apply' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END",
    ] {
        let f = Fixture::new().await;
        let (plan, original) = config_ready(&f).await;
        let pool = f.pool().await;
        sqlx::query(trigger).execute(&pool).await.unwrap();
        assert!(
            f.hub
                .apply_config_change(
                    &plan.id,
                    &plan.plan_digest,
                    &uuid::Uuid::new_v4().to_string(),
                    true
                )
                .await
                .is_err()
        );
        assert!(f.hub.config_changes().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read(f.path("a/config/options.json")).unwrap(),
            original
        );
    }
}
#[tokio::test]
async fn failed_terminal_record_retains_claim_across_restart_without_replaying_io() {
    let f = Fixture::new().await;
    let (plan, _) = config_ready(&f).await;
    let pool = f.pool().await;
    sqlx::query("CREATE TRIGGER reject_result BEFORE INSERT ON operation_log WHEN NEW.action='config.apply' AND NEW.status='applied' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END").execute(&pool).await.unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    let task = f
        .hub
        .apply_config_change(&plan.id, &plan.plan_digest, &key, true)
        .await
        .unwrap();
    for _ in 0..500 {
        if std::fs::read(f.path("a/config/options.json")).unwrap() == plan.proposed_json.as_bytes()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let observation = f.hub.inspect_config_change(&task.id).await.unwrap();
    assert_eq!(observation.matches, "proposed");
    assert_eq!(observation.change.state, "applying");
    assert!(!f.hub.dashboard().await.unwrap().resource_claims.is_empty());
    let source = f.path("a/config/options.json");
    let metadata = f.path("metadata");
    let modified = std::fs::metadata(&source).unwrap().modified().unwrap();
    pool.close().await;
    drop(f.hub);
    let reopened = Hub::open(&metadata).await.unwrap();
    let row = reopened.config_changes().await.unwrap().remove(0);
    assert_eq!(row.state, "recovery_required");
    assert_eq!(
        std::fs::metadata(&source).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(
        reopened
            .apply_config_change(&plan.id, &plan.plan_digest, &key, true)
            .await
            .unwrap()
            .state,
        "recovery_required"
    );
    assert_eq!(
        reopened
            .resolve_config_change(&task.id, &observation.current_sha256, true)
            .await
            .unwrap()
            .change
            .state,
        "resolved_proposed"
    );
    assert!(
        reopened
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .is_empty()
    );
    assert_eq!(
        std::fs::metadata(&source).unwrap().modified().unwrap(),
        modified
    );
}
#[tokio::test]
async fn config_apply_rechecks_session_admission_and_rejects_multiple_files() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    std::fs::write(
        f.path("a/config/user-default-mods.json"),
        br#"{"type":"MOD_INFO","id":"user:default","dependencies":["dda"]}"#,
    )
    .unwrap();
    assert_eq!(
        f.hub
            .prepare_config_change(
                &f.a,
                hub_core::ContentConfigInput {
                    default_mods: Some(vec![]),
                    soundpack: None,
                    sound_enabled: Some(true)
                },
                &plan.backup_id
            )
            .await
            .unwrap_err()
            .code,
        "CONFIG_CHANGE_SINGLE_FILE"
    );
    let pool = f.pool().await;
    sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('unknown',?,'interrupted_unknown','now')").bind(&f.installation).execute(&pool).await.unwrap();
    assert_eq!(
        f.hub
            .apply_config_change(
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
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
}

#[tokio::test]
async fn schema_twelve_upgrade_preserves_instance_backup_and_snapshots_old_metadata() {
    let f = Fixture::new().await;
    let backup = f.create(&f.a).await;
    assert_eq!(backup.state, "succeeded");
    let metadata = f.path("metadata");
    let pool = f.pool().await;
    sqlx::raw_sql("ALTER TABLE workspace DROP COLUMN root_dir; DROP TABLE hub_preferences; DROP TABLE config_change; PRAGMA user_version=12;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    drop(f.hub);
    let reopened = Hub::open(&metadata).await.unwrap();
    assert_eq!(
        reopened.dashboard().await.unwrap().backups[0].id,
        backup.backup_id
    );
    assert!(reopened.config_changes().await.unwrap().is_empty());
    assert!(std::fs::read_dir(&metadata).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("metadata-v12-")
    }));
}

#[tokio::test]
async fn candidate_metadata_changed_after_preparation_prevents_application() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    std::fs::create_dir_all(f.path("a/sound/new")).unwrap();
    std::fs::write(
        f.path("a/sound/new/soundpack.txt"),
        b"NAME: new\nVIEW: New\n",
    )
    .unwrap();
    let task = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        config_wait(&f.hub, &task.id).await.state,
        "recovery_required"
    );
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
}

#[tokio::test]
async fn changing_backup_workspace_invalidates_prepared_config_application() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    std::fs::create_dir(f.path("different-backups")).unwrap();
    let pool = f.pool().await;
    sqlx::query("UPDATE workspace SET backup_dir=?")
        .bind(f.path("different-backups").to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        f.hub
            .apply_config_change(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .is_err()
    );
    assert!(f.hub.config_changes().await.unwrap().is_empty());
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
}

#[tokio::test]
async fn rollback_restores_exact_original_bytes_in_a_separate_idempotent_record() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    let task = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &task.id).await.state, "applied");
    let rollback = f.hub.prepare_config_rollback(&task.id).await.unwrap();
    assert_eq!(rollback.rollback_of.as_deref(), Some(task.id.as_str()));
    assert_eq!(rollback.proposed_json.as_bytes(), original);
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        plan.proposed_json.as_bytes()
    );
    let key = uuid::Uuid::new_v4().to_string();
    assert!(
        f.hub
            .apply_config_change(&rollback.id, &rollback.plan_digest, &key, false)
            .await
            .is_err()
    );
    let started = f
        .hub
        .apply_config_change(&rollback.id, &rollback.plan_digest, &key, true)
        .await
        .unwrap();
    let result = config_wait(&f.hub, &started.id).await;
    assert_eq!(result.state, "applied");
    assert_eq!(result.rollback_of.as_deref(), Some(task.id.as_str()));
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
    assert_eq!(
        f.hub
            .apply_config_change(&rollback.id, &rollback.plan_digest, &key, true)
            .await
            .unwrap()
            .id,
        result.id
    );
    assert!(f.hub.prepare_config_rollback(&task.id).await.is_err());
    assert!(f.hub.prepare_config_rollback(&started.id).await.is_err());
    assert_eq!(
        std::fs::read(f.path("a/save/world.dat")).unwrap(),
        b"instance A world"
    );
}
#[tokio::test]
async fn rollback_refuses_later_edits_and_tampered_archives() {
    for tamper_archive in [false, true] {
        let f = Fixture::new().await;
        let (plan, _) = config_ready(&f).await;
        let task = f
            .hub
            .apply_config_change(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap();
        assert_eq!(config_wait(&f.hub, &task.id).await.state, "applied");
        let rollback = f.hub.prepare_config_rollback(&task.id).await.unwrap();
        let expected = if tamper_archive {
            std::fs::write(
                f.path("backups")
                    .join(&plan.backup_id)
                    .join("archive.tar.zst"),
                b"broken",
            )
            .unwrap();
            plan.proposed_json.as_bytes().to_vec()
        } else {
            let bytes = b"later user edit".to_vec();
            std::fs::write(f.path("a/config/options.json"), &bytes).unwrap();
            bytes
        };
        assert!(f.hub.prepare_config_rollback(&task.id).await.is_err());
        let started = f
            .hub
            .apply_config_change(
                &rollback.id,
                &rollback.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap();
        assert_eq!(
            config_wait(&f.hub, &started.id).await.state,
            "recovery_required"
        );
        assert_eq!(
            std::fs::read(f.path("a/config/options.json")).unwrap(),
            expected
        );
        assert!(!f.hub.dashboard().await.unwrap().resource_claims.is_empty());
    }
}
#[tokio::test]
async fn rollback_terminal_failure_is_reconciled_without_repeating_file_io() {
    let f = Fixture::new().await;
    let (plan, original) = config_ready(&f).await;
    let task = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &task.id).await.state, "applied");
    let rollback = f.hub.prepare_config_rollback(&task.id).await.unwrap();
    let pool = f.pool().await;
    sqlx::query("CREATE TRIGGER reject_rollback_result BEFORE INSERT ON operation_log WHEN NEW.status='applied' BEGIN SELECT RAISE(ABORT,'unavailable'); END").execute(&pool).await.unwrap();
    let started = f
        .hub
        .apply_config_change(
            &rollback.id,
            &rollback.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    for _ in 0..500 {
        if std::fs::read(f.path("a/config/options.json")).unwrap() == original {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let observation = f.hub.inspect_config_change(&started.id).await.unwrap();
    assert_eq!(observation.matches, "proposed");
    assert_eq!(observation.change.state, "applying");
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
    let mtime = std::fs::metadata(f.path("a/config/options.json"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(
        f.hub
            .resolve_config_change(&started.id, &observation.current_sha256, true)
            .await
            .unwrap()
            .change
            .state,
        "resolved_proposed"
    );
    assert_eq!(
        std::fs::metadata(f.path("a/config/options.json"))
            .unwrap()
            .modified()
            .unwrap(),
        mtime
    );
}
#[tokio::test]
async fn schema_thirteen_upgrade_keeps_existing_config_history_and_snapshots_database() {
    let f = Fixture::new().await;
    let (plan, _) = config_ready(&f).await;
    let started = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &started.id).await.state, "applied");
    let metadata = f.path("metadata");
    let pool = f.pool().await;
    sqlx::raw_sql("ALTER TABLE workspace DROP COLUMN root_dir; DROP TABLE hub_preferences; ALTER TABLE config_change DROP COLUMN rollback_of; PRAGMA user_version=13;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    drop(f.hub);
    let reopened = Hub::open(&metadata).await.unwrap();
    let rows = reopened.config_changes().await.unwrap();
    assert_eq!(rows[0].id, started.id);
    assert!(rows[0].rollback_of.is_none());
    assert!(std::fs::read_dir(&metadata).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("metadata-v13-")
    }));
}

#[tokio::test]
async fn sound_change_ignores_unrelated_mod_diagnostics_but_preserves_mod_guard() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.path("game/data/mods/dda")).unwrap();
    std::fs::write(
        f.path("game/data/mods/dda/modinfo.json"),
        br#"[{"type":"MOD_INFO","id":"dda","name":"Core","core":true}]"#,
    )
    .unwrap();
    let (plan, original) = config_ready(&f).await;
    let preview = f
        .hub
        .preview_content_config(&f.a, sound_draft())
        .await
        .unwrap();
    assert!(
        preview
            .report
            .discovery
            .findings
            .iter()
            .any(|d| d.code == "dependencies_unknown")
    );
    assert!(preview.proposed_discovery.findings.is_empty());
    let applied = f
        .hub
        .apply_config_change(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &applied.id).await.state, "applied");
    let rollback = f.hub.prepare_config_rollback(&applied.id).await.unwrap();
    let restored = f
        .hub
        .apply_config_change(
            &rollback.id,
            &rollback.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(config_wait(&f.hub, &restored.id).await.state, "applied");
    assert_eq!(
        std::fs::read(f.path("a/config/options.json")).unwrap(),
        original
    );
    std::fs::write(
        f.path("a/config/user-default-mods.json"),
        br#"{"type":"MOD_INFO","id":"user:default","dependencies":[]}"#,
    )
    .unwrap();
    let backup = f.create(&f.a).await;
    let error = f
        .hub
        .prepare_config_change(
            &f.a,
            hub_core::ContentConfigInput {
                default_mods: Some(vec!["dda".into()]),
                soundpack: None,
                sound_enabled: None,
            },
            &backup.backup_id,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "CONFIG_CHANGE_DIAGNOSTICS");
}

#[tokio::test]
async fn sound_switch_keeps_retained_pack_and_scan_integrity_guards() {
    let f = Fixture::new().await;
    std::fs::write(
        f.path("a/config/options.json"),
        br#"[{"name":"SOUND_ENABLED","value":"false"},{"name":"SOUNDPACKS","value":"basic"}]"#,
    )
    .unwrap();
    let backup = f.create(&f.a).await;
    // A declared but missing pack remains a blocker even for a switch-only draft.
    assert_eq!(
        f.hub
            .prepare_config_change(&f.a, sound_draft(), &backup.backup_id)
            .await
            .unwrap_err()
            .code,
        "CONFIG_CHANGE_DIAGNOSTICS"
    );
    for dir in ["game/data/sound/one", "game/data/sound/two"] {
        std::fs::create_dir_all(f.path(dir)).unwrap();
        std::fs::write(
            f.path(&format!("{dir}/soundpack.txt")),
            b"NAME: basic\nVIEW: Basic\n",
        )
        .unwrap();
    }
    assert_eq!(
        f.hub
            .prepare_config_change(&f.a, sound_draft(), &backup.backup_id)
            .await
            .unwrap_err()
            .code,
        "CONFIG_CHANGE_DIAGNOSTICS"
    );
    std::fs::remove_dir_all(f.path("game/data/sound/two")).unwrap();
    f.hub
        .prepare_config_change(&f.a, sound_draft(), &backup.backup_id)
        .await
        .unwrap();
    // Incomplete scanning is still rejected, even in an unrelated mod root.
    std::fs::create_dir_all(f.path("game/data/mods/broken")).unwrap();
    std::fs::write(f.path("game/data/mods/broken/modinfo.json"), b"not JSON").unwrap();
    assert_eq!(
        f.hub
            .prepare_config_change(&f.a, sound_draft(), &backup.backup_id)
            .await
            .unwrap_err()
            .code,
        "CONFIG_CHANGE_DIAGNOSTICS"
    );
    assert!(f.hub.config_changes().await.unwrap().is_empty());
}
