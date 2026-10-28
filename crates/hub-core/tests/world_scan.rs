use hub_core::{Hub, WorkspaceInput};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;
struct Fixture {
    temp: TempDir,
    data: PathBuf,
    saves: PathBuf,
    hub: Arc<Hub>,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("metadata");
        let saves = temp.path().join("saves");
        let backups = temp.path().join("backups");
        let content = temp.path().join("content");
        for dir in [&saves, &backups, &content] {
            std::fs::create_dir(dir).unwrap();
        }
        let hub = Hub::open(&data).await.unwrap();
        hub.initialize(WorkspaceInput {
            save_dir: saves.to_str().unwrap().into(),
            backup_dir: backups.to_str().unwrap().into(),
            content_dir: content.to_str().unwrap().into(),
        })
        .await
        .unwrap();
        Self {
            temp,
            data,
            saves,
            hub,
        }
    }
    async fn scan(&self) -> hub_core::ScanReport {
        let task = self
            .hub
            .start_world_scan(&uuid::Uuid::new_v4().to_string())
            .await
            .unwrap();
        wait_finished(&self.hub, &task.id).await;
        self.hub.scan_report(&task.id).await.unwrap()
    }
}
async fn wait_finished(hub: &Hub, id: &str) -> hub_core::ScanTask {
    for _ in 0..200 {
        let task = hub.scan_task(id).await.unwrap();
        if !matches!(task.state.as_str(), "running" | "cancelling") {
            return task;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("scan should finish");
}
#[tokio::test]
async fn discovers_opaque_candidates_and_shared_files_without_interpreting_characters() {
    let f = Fixture::new().await;
    std::fs::create_dir_all(f.saves.join("World-A/maps/empty")).unwrap();
    std::fs::create_dir(f.saves.join("World-B")).unwrap();
    std::fs::write(f.saves.join("World-A/角色.sav"), b"unrecognized\0binary").unwrap();
    std::fs::write(f.saves.join("World-B/角色.sav"), b"different bytes").unwrap();
    std::fs::write(f.saves.join("shared.json"), b"{invalid json").unwrap();
    let before = std::fs::read(f.saves.join("World-A/角色.sav")).unwrap();
    let report = f.scan().await;
    assert!(report.enumeration_complete);
    assert_eq!(report.candidates.len(), 2);
    assert!(
        report
            .candidates
            .iter()
            .all(|c| c.identity_confidence == "unknown" && !c.boundary_verified)
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.relative_path == "World-A/maps/empty" && e.file_type == "directory")
    );
    assert!(
        report
            .entries
            .iter()
            .any(|e| e.relative_path == "shared.json" && e.candidate_root.is_none())
    );
    assert_ne!(report.candidates[0].id, report.candidates[1].id);
    assert_eq!(
        std::fs::read(f.saves.join("World-A/角色.sav")).unwrap(),
        before
    );
    assert_eq!(
        report
            .entries
            .iter()
            .filter(|e| e.file_type == "file")
            .count(),
        3
    );
    let again = f.scan().await;
    assert_eq!(report.candidates[0].id, again.candidates[0].id);
}
#[tokio::test]
async fn request_keys_are_persistent_and_do_not_repeat_a_scan() {
    let f = Fixture::new().await;
    let key = uuid::Uuid::new_v4().to_string();
    let first = f.hub.start_world_scan(&key).await.unwrap();
    assert_eq!(f.hub.start_world_scan(&key).await.unwrap().id, first.id);
    wait_finished(&f.hub, &first.id).await;
    drop(f.hub);
    let hub = Hub::open(&f.data).await.unwrap();
    assert_eq!(hub.start_world_scan(&key).await.unwrap().id, first.id);
    assert_eq!(hub.dashboard().await.unwrap().scan_tasks.len(), 1);
}
#[tokio::test]
async fn empty_root_has_an_explicit_complete_empty_inventory() {
    let f = Fixture::new().await;
    let report = f.scan().await;
    assert!(report.enumeration_complete);
    assert_eq!(report.visited_entries, 0);
    assert!(report.candidates.is_empty());
    assert!(report.entries.is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_listed_but_external_data_is_not_traversed() {
    let f = Fixture::new().await;
    let external = f.temp.path().join("external");
    std::fs::create_dir(&external).unwrap();
    std::fs::write(external.join("private-marker"), "do not inspect").unwrap();
    std::os::unix::fs::symlink(&external, f.saves.join("linked-world")).unwrap();
    let report = f.scan().await;
    assert!(report.candidates.is_empty());
    assert_eq!(report.entries[0].file_type, "symlink");
    assert!(
        !report
            .entries
            .iter()
            .any(|e| e.relative_path.contains("private-marker"))
    );
    assert_eq!(report.issues[0].code, "LINK_NOT_FOLLOWED");
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn unrepresentable_names_are_reported_as_incomplete_instead_of_lossily_renamed() {
    use std::os::unix::ffi::OsStringExt;
    let f = Fixture::new().await;
    std::fs::write(
        f.saves.join(std::ffi::OsString::from_vec(vec![0xff, 0xfe])),
        b"opaque",
    )
    .unwrap();
    let report = f.scan().await;
    assert!(!report.enumeration_complete);
    assert!(report.issues.iter().any(|i| i.code == "NON_UTF8_NAME"));
}
#[tokio::test]
async fn cancellation_never_publishes_a_success_report() {
    let f = Fixture::new().await;
    // Large enough to keep enumeration active until the immediate cancellation request.
    for i in 0..1000 {
        std::fs::write(f.saves.join(format!("file-{i}")), b"untouched").unwrap();
    }
    let task = f
        .hub
        .start_world_scan(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let requested = f.hub.cancel_scan(&task.id).await.unwrap();
    assert!(matches!(
        requested.state.as_str(),
        "cancelling" | "cancelled"
    ));
    assert_eq!(wait_finished(&f.hub, &task.id).await.state, "cancelled");
    assert_eq!(
        f.hub.scan_report(&task.id).await.unwrap_err().code,
        "SCAN_RESULT_UNAVAILABLE"
    );
    assert_eq!(std::fs::read_dir(&f.saves).unwrap().count(), 1000);
}
#[tokio::test]
async fn interrupted_scan_is_reported_on_restart_and_never_resumed_blindly() {
    let f = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(f.data.join("metadata.sqlite")),
    )
    .await
    .unwrap();
    let workspace_id = f.hub.dashboard().await.unwrap().workspace.unwrap().id;
    sqlx::query("INSERT INTO task (id,request_key,workspace_id,operation,state,phase,created_at) VALUES ('interrupted','test-key',?,'world.scan','running','enumerating','2026-10-27')")
        .bind(workspace_id).execute(&pool).await.unwrap();
    pool.close().await;
    drop(f.hub);
    let hub = Hub::open(&f.data).await.unwrap();
    let task = hub.scan_task("interrupted").await.unwrap();
    assert_eq!(task.state, "recovery_required");
    assert_eq!(task.error_code.as_deref(), Some("TASK_INTERRUPTED"));
    assert!(hub.scan_report(&task.id).await.is_err());
    let next = hub
        .start_world_scan(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(wait_finished(&hub, &next.id).await.state, "succeeded");
}
#[tokio::test]
async fn migration_preserves_v1_database_and_saves_a_consistent_backup() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    std::fs::create_dir(&data).unwrap();
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(data.join("metadata.sqlite"))
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../src/schema.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspace VALUES ('original','save','backup','content','date')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let hub = Hub::open(&data).await.unwrap();
    assert_eq!(
        hub.dashboard().await.unwrap().workspace.unwrap().id,
        "original"
    );
    let backup = std::fs::read_dir(&data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("metadata-v1-")
        })
        .unwrap();
    let snapshot = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(backup)
            .read_only(true),
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT id FROM workspace")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "original"
    );
    snapshot.close().await;
}
#[tokio::test]
async fn failed_migration_keeps_v1_schema_and_original_rows() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    std::fs::create_dir(&data).unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(data.join("metadata.sqlite"))
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options.clone())
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../src/schema.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE task (collision TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert!(Hub::open(&data).await.is_err());
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    pool.close().await;
}

#[tokio::test]
async fn scan_and_launch_share_a_backend_gate_even_if_the_ui_is_bypassed() {
    let f = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(f.data.join("metadata.sqlite")),
    )
    .await
    .unwrap();
    let workspace_id = f.hub.dashboard().await.unwrap().workspace.unwrap().id;
    sqlx::query("INSERT INTO task (id,request_key,workspace_id,operation,state,phase,created_at) VALUES ('active','active-key',?,'world.scan','running','enumerating','2026-10-27')")
        .bind(workspace_id).execute(&pool).await.unwrap();
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
    sqlx::query("UPDATE task SET state='failed' WHERE id='active'")
        .execute(&pool)
        .await
        .unwrap();
    let task = f
        .hub
        .start_world_scan(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(wait_finished(&f.hub, &task.id).await.state, "succeeded");
    pool.close().await;
}

#[tokio::test]
async fn terminal_write_failure_does_not_leave_a_phantom_running_scan() {
    let f = Fixture::new().await;
    let db = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.data.join("metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_terminal BEFORE UPDATE OF state ON task WHEN NEW.state != OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&db).await.unwrap();
    let task = f
        .hub
        .start_world_scan(&uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    let ended = wait_finished(&f.hub, &task.id).await;
    assert_eq!(ended.state, "recovery_required");
    assert_eq!(
        ended.error_code.as_deref(),
        Some("TASK_RESULT_PERSIST_FAILED")
    );
    assert_eq!(
        f.hub.cancel_scan(&task.id).await.unwrap().state,
        "recovery_required"
    );
    assert_eq!(
        f.hub.dashboard().await.unwrap().scan_tasks[0].state,
        "recovery_required"
    );
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
    f.hub
        .prepare_backup()
        .await
        .expect("ended scan must not block workspace");
    sqlx::query("DROP TRIGGER reject_terminal")
        .execute(&db)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&task.id).await.unwrap();
    let raw: String = sqlx::query_scalar("SELECT state FROM task WHERE id=?")
        .bind(&task.id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(raw, "recovery_required");
    assert!(f.hub.scan_report(&task.id).await.is_err());
    assert!(f.scan().await.enumeration_complete);
}
