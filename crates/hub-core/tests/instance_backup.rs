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
    sqlx::raw_sql("DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; ALTER TABLE backup_archive DROP COLUMN source_root; PRAGMA user_version=8;").execute(&pool).await.unwrap();
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
        10
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
    sqlx::raw_sql("DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; PRAGMA user_version=8;").execute(&pool).await.unwrap();
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
