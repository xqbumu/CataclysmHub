use hub_core::{BackupDetailsInput, BackupRecord, Hub, WorkspaceInput};
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
    backup_id: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        for name in ["saves", "backups", "content"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        std::fs::write(temp.path().join("saves/unknown"), b"unknown\0bytes").unwrap();
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
            let current = hub.backup_task(&task.id).await.unwrap();
            if current.state == "succeeded" && Arc::strong_count(&hub) == 1 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "backup fixture timed out: {current:?}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        Self {
            temp,
            hub,
            backup_id: task.backup_id,
        }
    }
    fn input(&self) -> BackupDetailsInput {
        BackupDetailsInput {
            backup_id: self.backup_id.clone(),
            display_name: "升级前".into(),
            notes: "private note\n第二行\t保留".into(),
            retention_locked: true,
            expected_revision: 0,
        }
    }
    async fn record(&self) -> BackupRecord {
        self.hub.dashboard().await.unwrap().backups.remove(0)
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
fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut values = BTreeMap::new();
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
            values.insert(path.strip_prefix(root).unwrap().into(), value);
        }
    }
    values
}
fn annotation_events(data: &hub_core::Dashboard) -> Vec<&hub_core::OperationLog> {
    data.operations
        .iter()
        .filter(|event| {
            [
                "backup.details_update",
                "backup.retention_lock",
                "backup.retention_unlock",
            ]
            .contains(&event.action.as_str())
        })
        .collect()
}
#[tokio::test]
async fn annotation_is_local_persistent_and_does_not_rewrite_archives_paths_or_verification() {
    let f = Fixture::new().await;
    let before = f.record().await;
    let files = tree(&f.temp.path().join("backups"));
    let source = tree(&f.temp.path().join("saves"));
    let mut input = f.input();
    input.display_name = "  升级前  ".into();
    let record = f.hub.update_backup_details(input).await.unwrap();
    assert_eq!(record.display_name, "升级前");
    assert_eq!(record.notes, f.input().notes);
    assert!(record.retention_locked);
    assert_eq!(record.metadata_revision, 1);
    assert!(record.metadata_updated_at.is_some());
    assert_eq!(record.id, before.id);
    assert_eq!(record.relative_dir, before.relative_dir);
    assert_eq!(record.archive_sha256, before.archive_sha256);
    assert_eq!(record.manifest_sha256, before.manifest_sha256);
    assert_eq!(record.verified_at, before.verified_at);
    assert_eq!(record.verification, before.verification);
    let data = f.hub.dashboard().await.unwrap();
    let events = annotation_events(&data);
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|event| event.task_id == f.backup_id && event.status == "succeeded")
    );
    assert!(
        !serde_json::to_string(&data.operations)
            .unwrap()
            .contains("private note")
    );
    assert_eq!(tree(&f.temp.path().join("backups")), files);
    assert_eq!(tree(&f.temp.path().join("saves")), source);
    drop(f.hub);
    let hub = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    let persisted = hub.dashboard().await.unwrap().backups.remove(0);
    assert_eq!(persisted.display_name, "升级前");
    assert!(persisted.retention_locked);
    assert_eq!(persisted.metadata_revision, 1);
    hub_core::verify_backup_directory(&f.temp.path().join("backups").join(&f.backup_id)).unwrap();
}
#[tokio::test]
async fn same_value_retry_is_idempotent_but_stale_edits_and_unlocks_are_rejected() {
    let f = Fixture::new().await;
    let input = f.input();
    let record = f.hub.update_backup_details(input.clone()).await.unwrap();
    let replay = f.hub.update_backup_details(input.clone()).await.unwrap();
    assert_eq!(replay.metadata_revision, record.metadata_revision);
    assert_eq!(replay.metadata_updated_at, record.metadata_updated_at);
    assert_eq!(
        annotation_events(&f.hub.dashboard().await.unwrap()).len(),
        2
    );
    let mut stale = input.clone();
    stale.retention_locked = false;
    assert_eq!(
        f.hub.update_backup_details(stale).await.unwrap_err().code,
        "BACKUP_DETAILS_CONFLICT"
    );
    assert!(f.record().await.retention_locked);
    let mut stale = input;
    stale.display_name = "older edit".into();
    assert_eq!(
        f.hub.update_backup_details(stale).await.unwrap_err().code,
        "BACKUP_DETAILS_CONFLICT"
    );
    let mut unlock = f.input();
    unlock.expected_revision = 1;
    unlock.display_name.clear();
    unlock.notes.clear();
    unlock.retention_locked = false;
    let record = f.hub.update_backup_details(unlock).await.unwrap();
    assert!(!record.retention_locked);
    assert!(record.display_name.is_empty());
    assert!(record.notes.is_empty());
    assert_eq!(record.metadata_revision, 2);
    assert!(
        annotation_events(&f.hub.dashboard().await.unwrap())
            .iter()
            .any(|event| event.action == "backup.retention_unlock")
    );
}
#[tokio::test]
async fn unicode_limits_and_control_characters_are_enforced_before_any_write() {
    let f = Fixture::new().await;
    for input in [
        BackupDetailsInput {
            display_name: "😀".repeat(129),
            ..f.input()
        },
        BackupDetailsInput {
            display_name: "\nname".into(),
            ..f.input()
        },
        BackupDetailsInput {
            display_name: "a\0b".into(),
            ..f.input()
        },
        BackupDetailsInput {
            notes: "x".repeat(4001),
            ..f.input()
        },
        BackupDetailsInput {
            notes: "a\rb".into(),
            ..f.input()
        },
        BackupDetailsInput {
            expected_revision: -1,
            ..f.input()
        },
    ] {
        assert_eq!(
            f.hub.update_backup_details(input).await.unwrap_err().code,
            "BACKUP_DETAILS_INVALID"
        );
    }
    assert_eq!(f.record().await.metadata_revision, 0);
    assert!(annotation_events(&f.hub.dashboard().await.unwrap()).is_empty());
    let record = f
        .hub
        .update_backup_details(BackupDetailsInput {
            display_name: "😀".repeat(128),
            notes: "字".repeat(4000),
            ..f.input()
        })
        .await
        .unwrap();
    assert_eq!(record.display_name.chars().count(), 128);
    assert_eq!(record.notes.chars().count(), 4000);
}
#[tokio::test]
async fn audit_failure_rolls_back_metadata_and_any_earlier_audit_in_the_same_change() {
    let f = Fixture::new().await;
    let db = f.db().await;
    sqlx::query("CREATE TRIGGER reject_lock_audit BEFORE INSERT ON operation_log WHEN NEW.action='backup.retention_lock' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&db).await.unwrap();
    assert_eq!(
        f.hub
            .update_backup_details(f.input())
            .await
            .unwrap_err()
            .code,
        "DATABASE_ERROR"
    );
    let record = f.record().await;
    assert_eq!(record.metadata_revision, 0);
    assert!(record.display_name.is_empty());
    assert!(!record.retention_locked);
    assert!(annotation_events(&f.hub.dashboard().await.unwrap()).is_empty());
    sqlx::query("DROP TRIGGER reject_lock_audit")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .update_backup_details(f.input())
            .await
            .unwrap()
            .metadata_revision,
        1
    );
}
#[tokio::test]
async fn offline_media_does_not_prevent_local_annotation_or_create_another_archive() {
    let f = Fixture::new().await;
    std::fs::rename(
        f.temp.path().join("backups"),
        f.temp.path().join("offline-media"),
    )
    .unwrap();
    let before = tree(&f.temp.path().join("offline-media"));
    let record = f.hub.update_backup_details(f.input()).await.unwrap();
    assert!(record.retention_locked);
    assert_eq!(tree(&f.temp.path().join("offline-media")), before);
    assert!(!f.temp.path().join("backups").exists());
}
#[tokio::test]
async fn absent_records_and_revision_exhaustion_fail_without_changing_annotations() {
    let f = Fixture::new().await;
    assert_eq!(
        f.hub
            .update_backup_details(BackupDetailsInput {
                backup_id: uuid::Uuid::new_v4().to_string(),
                ..f.input()
            })
            .await
            .unwrap_err()
            .code,
        "BACKUP_NOT_FOUND"
    );
    let db = f.db().await;
    sqlx::query("UPDATE backup_archive SET metadata_revision=9007199254740991 WHERE id=?")
        .bind(&f.backup_id)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .update_backup_details(BackupDetailsInput {
                expected_revision: 9_007_199_254_740_991,
                ..f.input()
            })
            .await
            .unwrap_err()
            .code,
        "BACKUP_DETAILS_REVISION_EXHAUSTED"
    );
    assert!(f.record().await.display_name.is_empty());
    assert!(annotation_events(&f.hub.dashboard().await.unwrap()).is_empty());
}

async fn schema5_database(data: &Path) -> sqlx::SqlitePool {
    std::fs::create_dir(data).unwrap();
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
        include_str!("../src/migration-004.sql"),
        include_str!("../src/migration-005.sql")
    ))
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO workspace VALUES ('workspace','saves','backups','content','2026-01-01')",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO backup_task (id,request_key,request_digest,workspace_id,backup_id,operation,state,phase,created_at) VALUES ('task','key','digest','workspace','backup','backup.create','succeeded','finished','2026-01-01')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO backup_archive VALUES ('backup','task','workspace','backup','2026-01-01',0,0,'0','best_effort','verified','2026-01-01','archive-hash','manifest-hash')").execute(&db).await.unwrap();
    db
}
#[tokio::test]
async fn schema5_upgrade_sets_safe_annotation_defaults_and_preserves_a_valid_old_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    let db = schema5_database(&data).await;
    db.close().await;
    let hub = Hub::open(&data).await.unwrap();
    let record = hub.dashboard().await.unwrap().backups.remove(0);
    assert!(record.display_name.is_empty());
    assert!(record.notes.is_empty());
    assert!(!record.retention_locked);
    assert_eq!(record.metadata_revision, 0);
    assert!(record.metadata_updated_at.is_none());
    assert_eq!(record.archive_sha256, "archive-hash");
    let snapshot = std::fs::read_dir(&data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("metadata-v5-")
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
        5
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA quick_check")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT archive_sha256 FROM backup_archive")
            .fetch_one(&snapshot)
            .await
            .unwrap(),
        "archive-hash"
    );
}
#[tokio::test]
async fn schema6_failure_rolls_back_earlier_alter_statements_and_preserves_old_data() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("metadata");
    let db = schema5_database(&data).await;
    sqlx::query("ALTER TABLE backup_archive ADD COLUMN notes TEXT DEFAULT 'legacy'")
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
        5
    );
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('backup_archive')")
            .fetch_all(&db)
            .await
            .unwrap();
    assert!(!columns.iter().any(|column| column == "display_name"));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT notes FROM backup_archive")
            .fetch_one(&db)
            .await
            .unwrap(),
        "legacy"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT archive_sha256 FROM backup_archive")
            .fetch_one(&db)
            .await
            .unwrap(),
        "archive-hash"
    );
}
