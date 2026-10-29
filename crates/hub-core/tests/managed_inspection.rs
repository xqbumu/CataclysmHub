use hub_core::{Hub, InspectionTask, RestoreTask, WorkspaceInput};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    restore: RestoreTask,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_payload(b"unknown\0bytes").await
    }
    async fn with_payload(bytes: &[u8]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        for name in ["saves", "backups", "content"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        std::fs::create_dir(temp.path().join("saves/empty")).unwrap();
        std::fs::write(temp.path().join("saves/opaque"), bytes).unwrap();
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
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let current = hub.backup_task(&task.id).await.unwrap();
            if current.state == "succeeded" {
                break;
            }
            assert!(Instant::now() < deadline, "backup timed out: {current:?}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let plan = hub
            .prepare_restore(&task.backup_id, temp.path().to_str().unwrap())
            .await
            .unwrap();
        let restore = hub
            .start_restore(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true,
            )
            .await
            .unwrap();
        loop {
            let current = hub.restore_task(&restore.id).await.unwrap();
            if current.state == "succeeded" {
                break;
            }
            assert!(Instant::now() < deadline, "restore timed out: {current:?}");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Self { temp, hub, restore }
    }
    async fn inspect(&self) -> InspectionTask {
        let task = self
            .hub
            .start_inspection(&self.restore.id, &uuid::Uuid::new_v4().to_string())
            .await
            .unwrap();
        wait(&self.hub, &task.id).await
    }
    async fn db(&self) -> sqlx::SqlitePool {
        sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(self.temp.path().join("metadata/metadata.sqlite")),
        )
        .await
        .unwrap()
    }
}
async fn wait(hub: &Hub, id: &str) -> InspectionTask {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let task = hub.inspection_task(id).await.unwrap();
        if !["running", "cancelling"].contains(&task.state.as_str()) {
            return task;
        }
        assert!(Instant::now() < deadline, "inspection timed out");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
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
async fn inspection_persists_exact_result_and_audit_without_rewriting_or_reclassifying_restore() {
    let f = Fixture::new().await;
    let db = f.db().await;
    sqlx::query("UPDATE restore_task SET state='recovery_required',error_code='TASK_INTERRUPTED' WHERE id=?").bind(&f.restore.id).execute(&db).await.unwrap();
    std::fs::write(
        Path::new(&f.restore.destination).join("RESTORE_INCOMPLETE.json"),
        b"{historical marker",
    )
    .unwrap();
    let before = tree(Path::new(&f.restore.destination));
    let source = tree(&f.temp.path().join("saves"));
    let task = f.inspect().await;
    assert_eq!(task.state, "succeeded", "{task:?}");
    let result = task.result.unwrap();
    assert!(result.incomplete_marker_present);
    assert!(result.historical_report_present);
    assert_eq!(result.verification.restored_files, 1);
    assert_eq!(result.verification.restored_entries, 2);
    assert_eq!(
        f.hub.restore_task(&f.restore.id).await.unwrap().state,
        "recovery_required"
    );
    let data = f.hub.dashboard().await.unwrap();
    assert_eq!(
        data.inspection_tasks[0]
            .result
            .as_ref()
            .unwrap()
            .inspected_at,
        result.inspected_at
    );
    assert_eq!(
        data.operations
            .iter()
            .filter(|event| event.task_id == task.id && event.action == "backup.inspect_restored")
            .count(),
        2
    );
    assert_eq!(tree(Path::new(&f.restore.destination)), before);
    assert_eq!(tree(&f.temp.path().join("saves")), source);
}
#[tokio::test]
async fn later_tampering_is_a_separate_failed_check_and_does_not_replace_original_restore_history()
{
    let f = Fixture::new().await;
    assert_eq!(f.inspect().await.state, "succeeded");
    std::fs::write(
        Path::new(&f.restore.destination).join("data/opaque"),
        b"changed\0bytes",
    )
    .unwrap();
    let task = f.inspect().await;
    assert_eq!(task.state, "failed");
    assert_eq!(task.error_code.as_deref(), Some("RESTORED_DATA_MISMATCH"));
    assert!(task.result.is_none());
    assert_eq!(
        f.hub.restore_task(&f.restore.id).await.unwrap().state,
        "succeeded"
    );
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "verified"
    );
}
#[tokio::test]
async fn source_archive_is_bound_to_registered_hash_and_source_failures_revoke_current_health() {
    let f = Fixture::new().await;
    let archive = f
        .temp
        .path()
        .join("backups")
        .join(&f.restore.backup_id)
        .join("archive.tar.zst");
    std::fs::write(&archive, b"corrupt").unwrap();
    let task = f.inspect().await;
    assert_eq!(task.error_code.as_deref(), Some("HASH_MISMATCH"));
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "failed"
    );
    std::fs::remove_file(archive).unwrap();
    let task = f.inspect().await;
    assert_eq!(task.error_code.as_deref(), Some("BACKUP_IO_FAILED"));
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "unavailable"
    );
}
#[tokio::test]
async fn inspection_request_identity_and_result_survive_restart_without_repeating_io() {
    let f = Fixture::new().await;
    let key = uuid::Uuid::new_v4().to_string();
    let task = f.hub.start_inspection(&f.restore.id, &key).await.unwrap();
    assert_eq!(wait(&f.hub, &task.id).await.state, "succeeded");
    assert_eq!(
        f.hub
            .start_inspection("other", &key)
            .await
            .unwrap_err()
            .code,
        "REQUEST_CONFLICT"
    );
    for _ in 0..100 {
        if Arc::strong_count(&f.hub) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let previous = f
        .hub
        .inspection_task(&task.id)
        .await
        .unwrap()
        .result
        .unwrap()
        .inspected_at;
    std::fs::write(
        Path::new(&f.restore.destination).join("data/opaque"),
        b"later modification",
    )
    .unwrap();
    drop(f.hub);
    let hub = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    let replay = hub.start_inspection(&f.restore.id, &key).await.unwrap();
    assert_eq!(replay.id, task.id);
    assert_eq!(replay.result.unwrap().inspected_at, previous);
    let new = hub
        .start_inspection(&f.restore.id, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap();
    assert_eq!(wait(&hub, &new.id).await.state, "failed");
}
#[tokio::test]
async fn cancellation_saves_no_new_verdict_and_inspection_shares_workspace_gate() {
    let f = Fixture::with_payload(&vec![7; 16 * 1024 * 1024]).await;
    let before = tree(Path::new(&f.restore.destination));
    let task = f
        .hub
        .start_inspection(&f.restore.id, &uuid::Uuid::new_v4().to_string())
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
    f.hub.cancel_inspection(&task.id).await.unwrap();
    let task = wait(&f.hub, &task.id).await;
    assert_eq!(task.state, "cancelled");
    assert!(task.result.is_none());
    assert_eq!(tree(Path::new(&f.restore.destination)), before);
    f.hub.prepare_backup().await.unwrap();
}
#[tokio::test]
async fn terminal_persistence_failure_recovers_recording_only_without_repeating_check() {
    let f = Fixture::new().await;
    let db = f.db().await;
    sqlx::query("CREATE TRIGGER reject_inspection_terminal BEFORE UPDATE OF state ON inspection_task WHEN NEW.state!=OLD.state BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&db).await.unwrap();
    let before = tree(Path::new(&f.restore.destination));
    let task = f.inspect().await;
    assert_eq!(task.state, "recovery_required");
    assert_eq!(
        task.error_code.as_deref(),
        Some("TASK_RESULT_PERSIST_FAILED")
    );
    assert!(task.result.is_none());
    f.hub.prepare_backup().await.unwrap();
    sqlx::query("DROP TRIGGER reject_inspection_terminal")
        .execute(&db)
        .await
        .unwrap();
    f.hub.retry_task_recovery(&task.id).await.unwrap();
    let task = f.hub.inspection_task(&task.id).await.unwrap();
    assert_eq!(task.state, "recovery_required");
    assert!(task.result.is_none());
    assert_eq!(tree(Path::new(&f.restore.destination)), before);
}
#[tokio::test]
async fn interrupted_inspection_is_not_replayed_and_does_not_reclassify_original_restore() {
    let f = Fixture::new().await;
    let task = f.inspect().await;
    for _ in 0..100 {
        if Arc::strong_count(&f.hub) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let db = f.db().await;
    sqlx::query("UPDATE inspection_task SET state='running',phase='checking_restored' WHERE id=?")
        .bind(&task.id)
        .execute(&db)
        .await
        .unwrap();
    db.close().await;
    let before = tree(Path::new(&f.restore.destination));
    drop(f.hub);
    let hub = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    let task = hub.inspection_task(&task.id).await.unwrap();
    assert_eq!(task.state, "recovery_required");
    assert_eq!(task.error_code.as_deref(), Some("TASK_INTERRUPTED"));
    assert!(task.result.is_none());
    assert_eq!(
        hub.restore_task(&f.restore.id).await.unwrap().state,
        "succeeded"
    );
    assert_eq!(tree(Path::new(&f.restore.destination)), before);
}

#[cfg(unix)]
#[tokio::test]
async fn replacing_the_recorded_container_with_a_symlink_is_refused_without_following_it() {
    let f = Fixture::new().await;
    let destination = Path::new(&f.restore.destination);
    let original = f.temp.path().join("moved-output");
    std::fs::rename(destination, &original).unwrap();
    std::os::unix::fs::symlink(&original, destination).unwrap();
    let before = tree(&original);
    let task = f.inspect().await;
    assert_eq!(task.error_code.as_deref(), Some("RESTORED_DATA_MISMATCH"));
    assert_eq!(tree(&original), before);
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "verified"
    );
}
#[tokio::test]
async fn invalid_saved_result_is_an_explicit_error_instead_of_a_false_success_or_panic() {
    let f = Fixture::new().await;
    let task = f.inspect().await;
    let db = f.db().await;
    sqlx::query("UPDATE inspection_task SET result_json='{broken' WHERE id=?")
        .bind(&task.id)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        f.hub.inspection_task(&task.id).await.unwrap_err().code,
        "INSPECTION_RESULT_INVALID"
    );
    assert_eq!(
        f.hub.dashboard().await.unwrap_err().code,
        "INSPECTION_RESULT_INVALID"
    );
    assert_eq!(
        f.hub.restore_task(&f.restore.id).await.unwrap().state,
        "succeeded"
    );
}
#[tokio::test]
async fn schema4_upgrade_preserves_restore_history_and_saves_a_valid_metadata_snapshot() {
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
        include_str!("../src/migration-003.sql"),
        include_str!("../src/migration-004.sql")
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
    sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES ('backup-task','key','digest','workspace','backup','backup.create','succeeded','finished','2026-01-01')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO backup_archive VALUES ('backup','backup-task','workspace','backup','2026-01-01',0,0,'0','best_effort','verified','2026-01-01','archive-hash','manifest-hash')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO restore_task (id,request_key,request_digest,workspace_id,backup_id,destination,state,phase,created_at) VALUES ('restore','restore-key','digest','workspace','backup','recorded-target','succeeded','finished','2026-01-01')").execute(&db).await.unwrap();
    db.close().await;
    let hub = Hub::open(&data).await.unwrap();
    assert_eq!(
        hub.dashboard().await.unwrap().restore_tasks[0].destination,
        "recorded-target"
    );
    assert!(hub.dashboard().await.unwrap().inspection_tasks.is_empty());
    let snapshot = std::fs::read_dir(&data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("metadata-v4-")
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
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT destination FROM restore_task")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "recorded-target"
    );
}
#[tokio::test]
async fn failed_schema5_upgrade_keeps_the_old_version_and_data() {
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
        include_str!("../src/migration-003.sql"),
        include_str!("../src/migration-004.sql")
    ))
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("CREATE TABLE inspection_task (sentinel TEXT)")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO inspection_task VALUES ('keep')")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        Hub::open(&data).await.err().unwrap().code,
        "DB_MIGRATION_FAILED"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&db)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sentinel FROM inspection_task")
            .fetch_one(&db)
            .await
            .unwrap(),
        "keep"
    );
}

#[tokio::test]
async fn missing_backup_root_marks_the_registered_archive_unavailable_without_touching_target() {
    let f = Fixture::new().await;
    let before = tree(Path::new(&f.restore.destination));
    std::fs::rename(
        f.temp.path().join("backups"),
        f.temp.path().join("moved-backups"),
    )
    .unwrap();
    let task = f.inspect().await;
    assert_eq!(task.error_code.as_deref(), Some("PATH_INVALID"));
    assert_eq!(
        f.hub.dashboard().await.unwrap().backups[0].verification,
        "unavailable"
    );
    assert_eq!(tree(Path::new(&f.restore.destination)), before);
}
