use hub_core::{Hub, PlayInstanceInput, WorkspaceInput};
use tempfile::TempDir;

async fn setup() -> (TempDir, std::sync::Arc<Hub>, String) {
    let temp = tempfile::tempdir().unwrap();
    for name in ["save", "backup", "content", "game", "instance", "other"] {
        std::fs::create_dir(temp.path().join(name)).unwrap();
    }
    let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
    hub.initialize(WorkspaceInput {
        save_dir: temp.path().join("save").to_str().unwrap().into(),
        backup_dir: temp.path().join("backup").to_str().unwrap().into(),
        content_dir: temp.path().join("content").to_str().unwrap().into(),
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
        .import_installation(executable.to_str().unwrap(), "test")
        .await
        .unwrap();
    (temp, hub, installation.id)
}
fn input(temp: &TempDir, installation_id: &str, root: &str) -> PlayInstanceInput {
    PlayInstanceInput {
        installation_id: installation_id.into(),
        name: "isolated".into(),
        data_root: temp.path().join(root).to_str().unwrap().into(),
    }
}
#[tokio::test]
async fn independent_instances_persist_without_writing_data_or_granting_launch_binding() {
    let (temp, hub, installation) = setup().await;
    let a = hub
        .register_play_instance(input(&temp, &installation, "instance"))
        .await
        .unwrap();
    let b = hub
        .register_play_instance(input(&temp, &installation, "other"))
        .await
        .unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(a.binding_status, "unverified");
    let before = hub.dashboard().await.unwrap().operations.len();
    assert_eq!(
        hub.register_play_instance(input(&temp, &installation, "instance"))
            .await
            .unwrap()
            .id,
        a.id
    );
    assert_eq!(hub.dashboard().await.unwrap().operations.len(), before);
    assert!(
        !hub.launch_plan(&installation, vec![])
            .await
            .unwrap()
            .save_binding_verified
    );
    assert_eq!(
        std::fs::read_dir(temp.path().join("instance"))
            .unwrap()
            .count(),
        0
    );
    drop(hub);
    let reopened = Hub::open(&temp.path().join("metadata")).await.unwrap();
    assert_eq!(reopened.dashboard().await.unwrap().play_instances.len(), 2);
}
#[tokio::test]
async fn reject_overlaps_nonempty_and_changed_executable_without_registration() {
    let (temp, hub, installation) = setup().await;
    for root in ["save", "backup", "content", "metadata", "game", "."] {
        assert_eq!(
            hub.register_play_instance(input(&temp, &installation, root))
                .await
                .unwrap_err()
                .code,
            "INSTANCE_SCOPE_OVERLAP"
        );
    }
    std::fs::write(temp.path().join("instance/keep"), b"unchanged").unwrap();
    assert_eq!(
        hub.register_play_instance(input(&temp, &installation, "instance"))
            .await
            .unwrap_err()
            .code,
        "INSTANCE_ROOT_NOT_EMPTY"
    );
    std::fs::write(temp.path().join("game/test.exe"), b"changed").unwrap();
    assert_eq!(
        hub.register_play_instance(input(&temp, &installation, "other"))
            .await
            .unwrap_err()
            .code,
        "SOURCE_CHANGED"
    );
    assert!(hub.play_instances().await.unwrap().is_empty());
    assert_eq!(
        std::fs::read(temp.path().join("instance/keep")).unwrap(),
        b"unchanged"
    );
}
#[tokio::test]
async fn reject_nested_and_aliased_reuse() {
    let (temp, hub, installation) = setup().await;
    hub.register_play_instance(input(&temp, &installation, "instance"))
        .await
        .unwrap();
    std::fs::create_dir(temp.path().join("instance/child")).unwrap();
    assert_eq!(
        hub.register_play_instance(input(&temp, &installation, "instance/child"))
            .await
            .unwrap_err()
            .code,
        "INSTANCE_SCOPE_OVERLAP"
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(temp.path().join("instance"), temp.path().join("alias"))
            .unwrap();
        let mut duplicate = input(&temp, &installation, "alias");
        duplicate.name = "different".into();
        assert_eq!(
            hub.register_play_instance(duplicate)
                .await
                .unwrap_err()
                .code,
            "INSTANCE_ROOT_IN_USE"
        );
    }
    assert_eq!(hub.play_instances().await.unwrap().len(), 1);
}
#[tokio::test]
async fn audit_failure_rolls_back_registration() {
    let (temp, hub, installation) = setup().await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::raw_sql("CREATE TRIGGER refuse_instance_audit BEFORE INSERT ON operation_log WHEN NEW.action='instance.register' BEGIN SELECT RAISE(ABORT,'test'); END;").execute(&pool).await.unwrap();
    assert_eq!(
        hub.register_play_instance(input(&temp, &installation, "instance"))
            .await
            .unwrap_err()
            .code,
        "DATABASE_ERROR"
    );
    assert!(hub.play_instances().await.unwrap().is_empty());
    assert_eq!(
        std::fs::read_dir(temp.path().join("instance"))
            .unwrap()
            .count(),
        0
    );
}
#[tokio::test]
async fn installation_import_cannot_take_over_an_instance_scope() {
    let (temp, hub, installation) = setup().await;
    hub.register_play_instance(input(&temp, &installation, "instance"))
        .await
        .unwrap();
    let executable = temp.path().join("instance/another.exe");
    std::fs::copy(temp.path().join("game/test.exe"), &executable).unwrap();
    assert_eq!(
        hub.import_installation(executable.to_str().unwrap(), "another")
            .await
            .unwrap_err()
            .code,
        "INSTANCE_SCOPE_OVERLAP"
    );
    assert_eq!(hub.dashboard().await.unwrap().installations.len(), 1);
}
#[tokio::test]
async fn schema_six_upgrade_keeps_a_snapshot_and_records() {
    let (temp, hub, _) = setup().await;
    drop(hub);
    let db = temp.path().join("metadata/metadata.sqlite");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    sqlx::raw_sql("DROP TABLE play_instance; PRAGMA user_version=6;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let reopened = Hub::open(&temp.path().join("metadata")).await.unwrap();
    assert_eq!(reopened.dashboard().await.unwrap().installations.len(), 1);
    assert!(reopened.play_instances().await.unwrap().is_empty());
    assert!(
        std::fs::read_dir(temp.path().join("metadata"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("metadata-v6-"))
    );
}
#[tokio::test]
async fn two_installations_can_register_distinct_instances_but_not_share_roots() {
    let (temp, hub, installation) = setup().await;
    std::fs::create_dir(temp.path().join("game-b")).unwrap();
    let executable = temp.path().join("game-b/test.exe");
    std::fs::copy(temp.path().join("game/test.exe"), &executable).unwrap();
    let second = hub
        .import_installation(executable.to_str().unwrap(), "Version B")
        .await
        .unwrap();
    hub.register_play_instance(input(&temp, &installation, "instance"))
        .await
        .unwrap();
    hub.register_play_instance(input(&temp, &second.id, "other"))
        .await
        .unwrap();
    assert_eq!(
        hub.register_play_instance(input(&temp, &second.id, "instance"))
            .await
            .unwrap_err()
            .code,
        "INSTANCE_ROOT_IN_USE"
    );
    assert_eq!(hub.play_instances().await.unwrap().len(), 2);
}
#[tokio::test]
async fn failed_schema_seven_upgrade_preserves_schema_six() {
    let (temp, hub, _) = setup().await;
    drop(hub);
    let db = temp.path().join("metadata/metadata.sqlite");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    sqlx::raw_sql("PRAGMA user_version=6;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert!(Hub::open(&temp.path().join("metadata")).await.is_err());
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap(),
        6
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM installation")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[test]
fn caller_cannot_supply_binding_verification() {
    assert!(serde_json::from_value::<PlayInstanceInput>(serde_json::json!({"installation_id":"game", "name":"test", "data_root":"root", "binding_status":"verified"})).is_err());
}
