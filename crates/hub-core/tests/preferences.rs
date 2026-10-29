use hub_core::{Hub, PlayInstanceInput, WorkspaceInput};
use std::sync::Arc;
use tempfile::TempDir;

async fn setup() -> (TempDir, Arc<Hub>, String) {
    let temp = tempfile::tempdir().unwrap();
    for dir in ["saves", "backups", "content", "game", "instance"] {
        std::fs::create_dir(temp.path().join(dir)).unwrap();
    }
    let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
    hub.initialize(WorkspaceInput {
        save_dir: temp.path().join("saves").display().to_string(),
        backup_dir: temp.path().join("backups").display().to_string(),
        content_dir: temp.path().join("content").display().to_string(),
    })
    .await
    .unwrap();
    let exe = temp.path().join("game/game.exe");
    std::fs::write(&exe, b"fixture never executed").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let game = hub
        .import_installation(exe.to_str().unwrap(), "fixture")
        .await
        .unwrap();
    let instance = hub
        .register_play_instance(PlayInstanceInput {
            installation_id: game.id,
            name: "A".into(),
            data_root: temp.path().join("instance").display().to_string(),
        })
        .await
        .unwrap();
    (temp, hub, instance.id)
}

#[tokio::test]
async fn reset_backs_up_previous_preferences_without_changing_workspace_or_game_data() {
    let (temp, hub, instance) = setup().await;
    std::fs::write(temp.path().join("instance/sentinel"), b"keep").unwrap();
    let workspace = hub.dashboard().await.unwrap().workspace.unwrap();
    let saved = hub
        .save_instance_profile(&instance, "cdda-0h-source-trial-v1")
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    let result = hub.reset_preferences(saved.revision, true).await.unwrap();
    assert!(result.preferences.instance_profiles.is_empty());
    assert_eq!(result.preferences.revision, 2);
    let backup: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result.backup_path).unwrap()).unwrap();
    assert_eq!(
        backup["preferences"]["instance_profiles"][&instance],
        "cdda-0h-source-trial-v1"
    );
    assert_eq!(backup["workspace_reference_only"]["id"], workspace.id);
    let after = hub.dashboard().await.unwrap();
    assert_eq!(after.workspace.unwrap().backup_dir, workspace.backup_dir);
    assert_eq!(after.installations.len(), 1);
    assert_eq!(after.play_instances.len(), 1);
    assert_eq!(
        std::fs::read(temp.path().join("instance/sentinel")).unwrap(),
        b"keep"
    );
    drop(hub);
    let reopened = Hub::open(&temp.path().join("metadata")).await.unwrap();
    assert_eq!(reopened.preferences().await.unwrap().revision, 2);
    assert!(
        reopened
            .preferences()
            .await
            .unwrap()
            .instance_profiles
            .is_empty()
    );
}

#[tokio::test]
async fn confirmation_and_stale_revision_are_rejected_before_backup_or_reset() {
    let (temp, hub, instance) = setup().await;
    hub.save_instance_profile(&instance, "cdda-0h-source-trial-v1")
        .await
        .unwrap();
    assert_eq!(
        hub.reset_preferences(1, false).await.unwrap_err().code,
        "CONFIRMATION_REQUIRED"
    );
    assert_eq!(
        hub.reset_preferences(0, true).await.unwrap_err().code,
        "PREFERENCES_CHANGED"
    );
    assert!(!temp.path().join("metadata/config-backups").exists());
    assert_eq!(hub.preferences().await.unwrap().revision, 1);
}

#[tokio::test]
async fn backup_failure_does_not_reset_preferences() {
    let (temp, hub, instance) = setup().await;
    hub.save_instance_profile(&instance, "cdda-0h-source-trial-v1")
        .await
        .unwrap();
    std::fs::write(
        temp.path().join("metadata/config-backups"),
        b"not a directory",
    )
    .unwrap();
    assert_eq!(
        hub.reset_preferences(1, true).await.unwrap_err().code,
        "CONFIG_BACKUP_FAILED"
    );
    assert_eq!(
        hub.preferences().await.unwrap().instance_profiles[&instance],
        "cdda-0h-source-trial-v1"
    );
}

#[tokio::test]
async fn invalid_instance_or_profile_cannot_create_preferences() {
    let (_temp, hub, instance) = setup().await;
    assert_eq!(
        hub.save_instance_profile("missing", "cdda-0h-source-trial-v1")
            .await
            .unwrap_err()
            .code,
        "INSTANCE_NOT_FOUND"
    );
    assert_eq!(
        hub.save_instance_profile(&instance, "arbitrary-shell")
            .await
            .unwrap_err()
            .code,
        "PROFILE_UNSUPPORTED"
    );
    assert_eq!(hub.preferences().await.unwrap().revision, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_backup_directory_is_not_followed() {
    let (temp, hub, instance) = setup().await;
    hub.save_instance_profile(&instance, "cdda-0h-source-trial-v1")
        .await
        .unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, temp.path().join("metadata/config-backups")).unwrap();
    assert_eq!(
        hub.reset_preferences(1, true).await.unwrap_err().code,
        "CONFIG_BACKUP_FAILED"
    );
    assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
    assert_eq!(hub.preferences().await.unwrap().revision, 1);
}

#[tokio::test]
async fn failed_reset_audit_keeps_preferences_and_the_completed_backup() {
    let (temp, hub, instance) = setup().await;
    hub.save_instance_profile(&instance, "cdda-0h-source-trial-v1")
        .await
        .unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER reject_reset BEFORE INSERT ON operation_log WHEN NEW.action='preferences.reset' BEGIN SELECT RAISE(ABORT,'injected'); END").execute(&pool).await.unwrap();
    assert!(hub.reset_preferences(1, true).await.is_err());
    assert_eq!(hub.preferences().await.unwrap().revision, 1);
    assert_eq!(
        hub.preferences().await.unwrap().instance_profiles[&instance],
        "cdda-0h-source-trial-v1"
    );
    assert_eq!(
        std::fs::read_dir(temp.path().join("metadata/config-backups"))
            .unwrap()
            .count(),
        1
    );
}

#[tokio::test]
async fn schema14_upgrade_keeps_workspace_and_creates_a_metadata_backup() {
    let (temp, hub, _) = setup().await;
    let workspace = hub.dashboard().await.unwrap().workspace.unwrap();
    drop(hub);
    let data = temp.path().join("metadata");
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        data.join("metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::raw_sql("ALTER TABLE workspace DROP COLUMN root_dir; DROP TABLE hub_preferences; PRAGMA user_version=14;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let hub = Hub::open(&data).await.unwrap();
    assert_eq!(
        hub.dashboard().await.unwrap().workspace.unwrap().id,
        workspace.id
    );
    assert_eq!(hub.preferences().await.unwrap().revision, 0);
    assert!(std::fs::read_dir(data).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("metadata-v14-")
    }));
}
