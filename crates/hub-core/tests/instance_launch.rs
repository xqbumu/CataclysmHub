use hub_core::{Hub, PlayInstanceInput, WorkspaceInput};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;
const PROFILE: &str = "cdda-0h-source-trial-v1";
struct Fixture {
    temp: TempDir,
    hub: Arc<Hub>,
    instance: String,
    installation: String,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::Builder::new()
            .prefix("hub trial 空格 ")
            .tempdir()
            .unwrap();
        for dir in ["save", "backups", "content", "game", "instance"] {
            std::fs::create_dir(temp.path().join(dir)).unwrap();
        }
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let text = |name: &str| temp.path().join(name).to_str().unwrap().to_owned();
        hub.initialize(WorkspaceInput {
            save_dir: text("save"),
            backup_dir: text("backups"),
            content_dir: text("content"),
        })
        .await
        .unwrap();
        let executable = temp.path().join("game/test.exe");
        std::fs::write(&executable, b"#!/bin/sh\nwhile [ $# -gt 0 ]; do\n case \"$1\" in\n --userdir) user=$2;;\n --savedir) save=$2;;\n --configdir) config=$2;;\n esac\n shift 2\ndone\ntest -d \"$save\" && test -d \"$config\" || exit 13\nprintf 'synthetic save' > \"$save/world.dat\"\nprintf 'synthetic config' > \"$config/options.json\"\nsleep 0.3\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let installation = hub
            .import_installation(executable.to_str().unwrap(), "synthetic")
            .await
            .unwrap()
            .id;
        let instance = hub
            .register_play_instance(PlayInstanceInput {
                installation_id: installation.clone(),
                name: "trial".into(),
                data_root: text("instance"),
            })
            .await
            .unwrap()
            .id;
        Self {
            temp,
            hub,
            instance,
            installation,
        }
    }
    fn root(&self) -> PathBuf {
        self.temp.path().join("instance")
    }
}
#[tokio::test]
async fn preview_is_read_only_explicit_and_never_verified() {
    let f = Fixture::new().await;
    let before = f.hub.dashboard().await.unwrap().operations.len();
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instance, "invented")
            .await
            .unwrap_err()
            .code,
        "PROFILE_UNSUPPORTED"
    );
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    assert_eq!(plan.play_instance_id.as_deref(), Some(f.instance.as_str()));
    assert!(!plan.save_binding_verified);
    assert_eq!(
        plan.args,
        vec![
            "--userdir".into(),
            format!("{}/", plan.instance_data_root.unwrap()),
            "--savedir".into(),
            format!("{}/save/", f.root().canonicalize().unwrap().display()),
            "--configdir".into(),
            format!("{}/config/", f.root().canonicalize().unwrap().display())
        ]
    );
    assert!(
        plan.warnings
            .iter()
            .any(|warning| warning.contains("安装基础目录"))
    );
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0);
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), before);
}
#[tokio::test]
async fn directory_replacement_after_preview_is_rejected_before_spawning() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    std::fs::rename(f.root(), f.temp.path().join("old-instance")).unwrap();
    std::fs::create_dir(f.root()).unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "INSTANCE_PATH_CHANGED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0);
}
#[tokio::test]
async fn installation_directory_replacement_with_identical_program_is_rejected() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    std::fs::rename(f.temp.path().join("game"), f.temp.path().join("old-game")).unwrap();
    std::fs::create_dir(f.temp.path().join("game")).unwrap();
    std::fs::copy(
        f.temp.path().join("old-game/test.exe"),
        f.temp.path().join("game/test.exe"),
    )
    .unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "INSTANCE_PATH_CHANGED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn added_symlink_after_preview_is_rejected() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    std::os::unix::fs::symlink(f.temp.path().join("save"), f.root().join("save")).unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "INSTANCE_ROLE_INVALID"
    );
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("save"))
            .unwrap()
            .count(),
        0
    );
}
#[cfg(unix)]
#[tokio::test]
async fn trial_runs_real_child_records_instance_and_keeps_global_gate() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, false).await.unwrap_err().code,
        "CONFIRMATION_REQUIRED"
    );
    let session = f.hub.launch(&plan.id, true).await.unwrap();
    assert_eq!(
        session.play_instance_id.as_deref(),
        Some(f.instance.as_str())
    );
    assert_eq!(session.launch_profile_id.as_deref(), Some(PROFILE));
    assert_eq!(
        f.hub
            .launch_plan(&f.installation, vec![])
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instance, PROFILE)
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    for _ in 0..100 {
        if f.hub.dashboard().await.unwrap().sessions[0].state == "exited_unverified" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let recorded = f.hub.dashboard().await.unwrap().sessions.remove(0);
    assert_eq!(recorded.state, "exited_unverified");
    assert_eq!(recorded.play_instance_id, session.play_instance_id);
    assert_eq!(
        std::fs::read(f.root().join("save/world.dat")).unwrap(),
        b"synthetic save"
    );
    assert_eq!(
        std::fs::read(f.root().join("config/options.json")).unwrap(),
        b"synthetic config"
    );
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("save"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "PLAN_STALE"
    );
    let next = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    assert!(!next.save_binding_verified);
}
#[tokio::test]
async fn restart_preserves_unknown_instance_context_without_replay() {
    let f = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("INSERT INTO session(id,installation_id,state,started_at,play_instance_id,launch_profile_id,instance_data_root) VALUES ('test',?,'running','2026-10-28T00:00:00Z',?,?,?)")
        .bind(&f.installation).bind(&f.instance).bind(PROFILE).bind(f.root().to_str().unwrap()).execute(&pool).await.unwrap();
    pool.close().await;
    drop(f.hub);
    let hub = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    let data = hub.dashboard().await.unwrap();
    assert_eq!(data.sessions[0].state, "interrupted_unknown");
    assert_eq!(
        data.sessions[0].play_instance_id.as_deref(),
        Some(f.instance.as_str())
    );
    assert_eq!(
        hub.instance_launch_plan(&f.instance, PROFILE)
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("instance"))
            .unwrap()
            .count(),
        0
    );
}
#[tokio::test]
async fn schema_seven_upgrade_preserves_old_sessions_and_snapshots() {
    let f = Fixture::new().await;
    let db = f.temp.path().join("metadata/metadata.sqlite");
    drop(f.hub);
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE session DROP COLUMN play_instance_id; ALTER TABLE session DROP COLUMN launch_profile_id; ALTER TABLE session DROP COLUMN instance_data_root; PRAGMA user_version=7;").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO session(id,installation_id,state,started_at) VALUES ('old',?,'exited_unverified','2026-10-28T00:00:00Z')").bind(&f.installation).execute(&pool).await.unwrap();
    pool.close().await;
    let reopened = Hub::open(&f.temp.path().join("metadata")).await.unwrap();
    let data = reopened.dashboard().await.unwrap();
    assert_eq!(data.sessions[0].id, "old");
    assert!(data.sessions[0].play_instance_id.is_none());
    assert_eq!(data.play_instances.len(), 1);
    assert!(
        std::fs::read_dir(f.temp.path().join("metadata"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("metadata-v7-"))
    );
}
#[tokio::test]
async fn failed_schema_eight_upgrade_does_not_partially_alter_sessions() {
    let f = Fixture::new().await;
    let db = f.temp.path().join("metadata/metadata.sqlite");
    drop(f.hub);
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    sqlx::raw_sql("ALTER TABLE session DROP COLUMN play_instance_id; ALTER TABLE session DROP COLUMN instance_data_root; PRAGMA user_version=7;").execute(&pool).await.unwrap();
    pool.close().await;
    assert!(Hub::open(&f.temp.path().join("metadata")).await.is_err());
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.display()))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap(),
        7
    );
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('session')")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(!columns.iter().any(|name| name == "play_instance_id"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM play_instance")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn macos_profiles_do_not_silently_apply_to_other_platforms_or_layouts() {
    let f = Fixture::new().await;
    let error = f
        .hub
        .instance_launch_plan(&f.instance, "cdda-0i-1-macos-trial-v1")
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        if cfg!(target_os = "macos") {
            "BUNDLE_LAYOUT_UNSUPPORTED"
        } else {
            "PROFILE_PLATFORM_UNSUPPORTED"
        }
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_bundle_trial_freezes_local_loader_paths_and_rechecks_frameworks() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new().await;
    let resources = f.temp.path().join("Game.app/Contents/Resources");
    std::fs::create_dir_all(&resources).unwrap();
    let executable = resources.join("cataclysm-tiles");
    std::fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["SDL2", "SDL2_image", "SDL2_ttf", "SDL2_mixer"] {
        let folder = resources.join(format!("{name}.framework/Versions/A"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), b"synthetic library, never loaded").unwrap();
    }
    let root = f.temp.path().join("mac-instance");
    std::fs::create_dir(&root).unwrap();
    let installation = f
        .hub
        .import_installation(executable.to_str().unwrap(), "bundle fixture")
        .await
        .unwrap();
    let instance = f
        .hub
        .register_play_instance(PlayInstanceInput {
            installation_id: installation.id,
            name: "mac trial".into(),
            data_root: root.to_str().unwrap().into(),
        })
        .await
        .unwrap();
    let plan = f
        .hub
        .instance_launch_plan(&instance.id, "cdda-0h-release-macos-trial-v1")
        .await
        .unwrap();
    let canonical = resources
        .canonicalize()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        plan.environment,
        std::collections::BTreeMap::from([
            ("DYLD_FRAMEWORK_PATH".into(), canonical.clone()),
            ("DYLD_LIBRARY_PATH".into(), canonical)
        ])
    );
    assert!(!plan.save_binding_verified);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    std::fs::remove_file(resources.join("SDL2.framework/Versions/A/SDL2")).unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "BUNDLE_LAYOUT_UNSUPPORTED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert!(
        f.hub
            .launch_plan(&f.installation, vec![])
            .await
            .unwrap()
            .environment
            .is_empty()
    );
}
#[tokio::test]
async fn failed_launch_audit_does_not_prepare_directories_or_run_program() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::raw_sql("CREATE TRIGGER refuse_launch BEFORE INSERT ON operation_log WHEN NEW.action='game.launch' BEGIN SELECT RAISE(ABORT,'test'); END;").execute(&pool).await.unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "DATABASE_ERROR"
    );
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0);
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}
#[tokio::test]
async fn occupied_role_path_is_rejected_without_overwriting_original_file() {
    let f = Fixture::new().await;
    std::fs::write(f.root().join("config"), b"keep this file").unwrap();
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instance, PROFILE)
            .await
            .unwrap_err()
            .code,
        "INSTANCE_ROLE_INVALID"
    );
    assert_eq!(
        std::fs::read(f.root().join("config")).unwrap(),
        b"keep this file"
    );
    assert!(!f.root().join("save").exists());
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}

#[tokio::test]
async fn resource_preview_is_readonly_and_rechecks_owners_without_granting_concurrency() {
    let f = Fixture::new().await;
    let before = f.hub.dashboard().await.unwrap().operations.len();
    let preview = f.hub.preview_instance_resources(&f.instance).await.unwrap();
    assert!(preview.conflicts.is_empty());
    assert!(preview.scope_issues.is_empty());
    assert!(!preview.binding_verified && !preview.concurrency_available);
    assert_eq!(preview.scopes.len(), 2);
    assert_eq!(std::fs::read_dir(f.root()).unwrap().count(), 0);
    assert_eq!(f.hub.dashboard().await.unwrap().operations.len(), before);
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::query("INSERT INTO session (id,installation_id,state,started_at) VALUES ('unknown-resource',?,'interrupted_unknown','now')")
        .bind(&f.installation).execute(&pool).await.unwrap();
    let preview = f.hub.preview_instance_resources(&f.instance).await.unwrap();
    assert!(
        preview
            .conflicts
            .iter()
            .any(|c| c.reason == "unknown_game_writes")
    );
    assert!(
        preview
            .conflicts
            .iter()
            .any(|c| c.reason == "directory_overlap"
                && c.protected_path.as_deref() == Some(preview.scopes[0].path.as_str()))
    );
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instance, PROFILE)
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    f.hub
        .acknowledge_interrupted("unknown-resource")
        .await
        .unwrap();
    let preview = f.hub.preview_instance_resources(&f.instance).await.unwrap();
    assert!(preview.conflicts.is_empty());
    assert!(!preview.binding_verified && !preview.concurrency_available);
    pool.close().await;
}

#[tokio::test]
async fn resource_preview_reports_missing_instance_instead_of_implying_clearance() {
    let f = Fixture::new().await;
    std::fs::remove_dir(f.root()).unwrap();
    let preview = f.hub.preview_instance_resources(&f.instance).await.unwrap();
    assert_eq!(preview.scope_issues.len(), 1);
    assert_eq!(preview.scope_issues[0].code, "RESOURCE_SCOPE_UNAVAILABLE");
    assert!(!preview.concurrency_available);
    assert_eq!(
        f.hub
            .preview_instance_resources("missing-id")
            .await
            .unwrap_err()
            .code,
        "INSTANCE_NOT_FOUND"
    );
    assert!(!f.root().exists());
}
