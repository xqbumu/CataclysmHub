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
        Self::with_program(None).await
    }
    async fn with_program(program: Option<&[u8]>) -> Self {
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
        std::fs::write(&executable, program.unwrap_or(b"#!/bin/sh\nwhile [ $# -gt 0 ]; do\n case \"$1\" in\n --userdir) user=$2;;\n --savedir) save=$2;;\n --configdir) config=$2;;\n esac\n shift 2\ndone\ntest -d \"$save\" && test -d \"$config\" || exit 13\nprintf 'synthetic save' > \"$save/world.dat\"\nprintf 'synthetic config' > \"$config/options.json\"\nsleep 0.3\n")).unwrap();
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
#[cfg(unix)]
#[tokio::test]
async fn simultaneous_confirmations_recheck_ownership_before_spawning() {
    // Hold the winner until both confirmations finish. Import this executable
    // unchanged so the production digest check remains part of the test.
    let f = Fixture::with_program(Some(
        b"#!/bin/sh\nprintf 'spawn\\n' >> launches\ni=0\nwhile [ ! -f release ] && [ $i -lt 500 ]; do sleep 0.02; i=$((i + 1)); done\n",
    )).await;
    let first = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    let second = f
        .hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.hub.launch(&first.id, true),
        f.hub.launch(&second.id, true)
    );
    // Release even if an assertion below fails, so the child cannot linger.
    std::fs::write(f.temp.path().join("game/release"), b"").unwrap();
    let outcomes = [a, b];
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        outcomes.iter().find_map(|r| r.as_ref().err()).unwrap().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    for _ in 0..100 {
        if f.hub.dashboard().await.unwrap().sessions[0].state == "exited_unverified" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let dashboard = f.hub.dashboard().await.unwrap();
    assert_eq!(dashboard.sessions.len(), 1);
    assert_eq!(dashboard.sessions[0].state, "exited_unverified");
    assert_eq!(
        std::fs::read_to_string(f.temp.path().join("game/launches")).unwrap(),
        "spawn\n"
    );
    // Once the actual process has exited, admission is available again.
    f.hub
        .instance_launch_plan(&f.instance, PROFILE)
        .await
        .unwrap();
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
    sqlx::raw_sql("DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; ALTER TABLE backup_archive DROP COLUMN source_root; ALTER TABLE session DROP COLUMN play_instance_id; ALTER TABLE session DROP COLUMN launch_profile_id; ALTER TABLE session DROP COLUMN instance_data_root; PRAGMA user_version=7;").execute(&pool).await.unwrap();
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
    sqlx::raw_sql("DROP TABLE installation_copy_task; ALTER TABLE backup_task DROP COLUMN play_instance_id; ALTER TABLE backup_task DROP COLUMN source_root; ALTER TABLE backup_archive DROP COLUMN play_instance_id; ALTER TABLE backup_archive DROP COLUMN source_root; ALTER TABLE session DROP COLUMN play_instance_id; ALTER TABLE session DROP COLUMN instance_data_root; PRAGMA user_version=7;").execute(&pool).await.unwrap();
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
    for profile in [
        "cdda-0i-1-macos-trial-v1",
        "cdda-0i-1-macos-native-trial-v1",
    ] {
        let error = f
            .hub
            .instance_launch_plan(&f.instance, profile)
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
    }
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

#[cfg(target_os = "macos")]
async fn runtime_fixture(f: &Fixture) -> String {
    use std::os::unix::fs::PermissionsExt;
    let resources = f.temp.path().join("Runtime.app/Contents/Resources");
    std::fs::create_dir_all(&resources).unwrap();
    let executable = resources.join("cataclysm-tiles");
    std::fs::write(&executable, b"#!/bin/sh\nfor dir in \"$HOME\" \"$TMPDIR\" \"$TMP\" \"$TEMP\" \"$XDG_CONFIG_HOME\" \"$XDG_CACHE_HOME\" \"$XDG_DATA_HOME\" \"$XDG_STATE_HOME\"; do\n test -d \"$dir\" || exit 17\n printf 'isolated child marker' > \"$dir/child-marker\" || exit 18\ndone\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["SDL2", "SDL2_image", "SDL2_ttf", "SDL2_mixer"] {
        let folder = resources.join(format!("{name}.framework/Versions/A"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), b"synthetic library, never loaded").unwrap();
    }
    let installation = f
        .hub
        .import_installation(executable.to_str().unwrap(), "runtime fixture")
        .await
        .unwrap();
    let root = f.temp.path().join("runtime-instance");
    std::fs::create_dir(&root).unwrap();
    f.hub
        .register_play_instance(PlayInstanceInput {
            installation_id: installation.id,
            name: "runtime trial".into(),
            data_root: root.to_str().unwrap().into(),
        })
        .await
        .unwrap()
        .id
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn runtime_v2_preview_is_readonly_and_child_writes_to_declared_instance_roles() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new().await;
    let instance = runtime_fixture(&f).await;
    let root = f
        .temp
        .path()
        .join("runtime-instance")
        .canonicalize()
        .unwrap();
    let runtime = root.join(".hub-runtime");
    let before_home = std::env::var_os("HOME");
    let before_tmp = std::env::var_os("TMPDIR");
    for profile in ["cdda-0h-release-macos-trial-v2", "cdda-0i-1-macos-trial-v2"] {
        let plan = f
            .hub
            .instance_launch_plan(&instance, profile)
            .await
            .unwrap();
        assert_eq!(plan.environment.len(), 10);
        for (key, role) in [
            ("HOME", "home"),
            ("TMPDIR", "tmp"),
            ("TMP", "tmp"),
            ("TEMP", "tmp"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_DATA_HOME", "data"),
            ("XDG_STATE_HOME", "state"),
        ] {
            assert_eq!(plan.environment[key], runtime.join(role).to_str().unwrap());
        }
        assert!(!plan.save_binding_verified);
        if !runtime.exists() {
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        }
        let session = f.hub.launch(&plan.id, true).await.unwrap();
        let mut ended = false;
        for _ in 0..100 {
            let data = f.hub.dashboard().await.unwrap();
            let record = data.sessions.iter().find(|s| s.id == session.id).unwrap();
            if record.state == "exited_unverified" {
                assert_eq!(record.exit_code, Some(0));
                ended = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(ended);
        for role in ["home", "tmp", "config", "cache", "data", "state"] {
            assert_eq!(
                std::fs::read(runtime.join(role).join("child-marker")).unwrap(),
                b"isolated child marker"
            );
            assert_eq!(
                std::fs::metadata(runtime.join(role))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        assert_eq!(std::env::var_os("HOME"), before_home);
        assert_eq!(std::env::var_os("TMPDIR"), before_tmp);
    }
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("save"))
            .unwrap()
            .count(),
        0
    );
    let old = f
        .hub
        .instance_launch_plan(&instance, "cdda-0h-release-macos-trial-v1")
        .await
        .unwrap();
    assert_eq!(old.environment.len(), 2);
    assert!(!old.environment.contains_key("HOME"));
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn runtime_v2_rejects_occupied_or_redirected_roles_before_launch() {
    let f = Fixture::new().await;
    let instance = runtime_fixture(&f).await;
    let root = f.temp.path().join("runtime-instance");
    let runtime = root.join(".hub-runtime");
    std::fs::write(&runtime, b"keep occupied role").unwrap();
    assert_eq!(
        f.hub
            .instance_launch_plan(&instance, "cdda-0i-1-macos-trial-v2")
            .await
            .unwrap_err()
            .code,
        "INSTANCE_ROLE_INVALID"
    );
    assert_eq!(std::fs::read(&runtime).unwrap(), b"keep occupied role");
    std::fs::remove_file(&runtime).unwrap();
    let plan = f
        .hub
        .instance_launch_plan(&instance, "cdda-0i-1-macos-trial-v2")
        .await
        .unwrap();
    std::fs::create_dir(&runtime).unwrap();
    std::os::unix::fs::symlink(f.temp.path().join("save"), runtime.join("home")).unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "INSTANCE_ROLE_INVALID"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("save"))
            .unwrap()
            .count(),
        0
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn runtime_v2_failed_intent_audit_creates_no_runtime_directories() {
    let f = Fixture::new().await;
    let instance = runtime_fixture(&f).await;
    let plan = f
        .hub
        .instance_launch_plan(&instance, "cdda-0h-release-macos-trial-v2")
        .await
        .unwrap();
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}",
        f.temp.path().join("metadata/metadata.sqlite").display()
    ))
    .await
    .unwrap();
    sqlx::raw_sql("CREATE TRIGGER refuse_runtime_launch BEFORE INSERT ON operation_log WHEN NEW.action='game.launch' BEGIN SELECT RAISE(ABORT,'test'); END;").execute(&pool).await.unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "DATABASE_ERROR"
    );
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("runtime-instance"))
            .unwrap()
            .count(),
        0
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}

#[cfg(target_os = "macos")]
async fn native_fixture(f: &Fixture) -> (String, std::path::PathBuf, std::path::PathBuf) {
    let resources = f.temp.path().join("Native.app/Contents/Resources");
    let macos = f.temp.path().join("Native.app/Contents/MacOS");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::create_dir(&macos).unwrap();
    let executable = macos.join("native-game");
    std::fs::copy("/usr/bin/true", &executable).unwrap();
    for name in ["SDL2", "SDL2_image", "SDL2_ttf", "SDL2_mixer"] {
        let folder = resources.join(format!("{name}.framework/Versions/A"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(name), b"synthetic framework, never loaded").unwrap();
    }
    let installation = f
        .hub
        .import_installation(executable.to_str().unwrap(), "native fixture")
        .await
        .unwrap();
    let root = f.temp.path().join("native-instance");
    std::fs::create_dir(&root).unwrap();
    let instance = f
        .hub
        .register_play_instance(PlayInstanceInput {
            installation_id: installation.id,
            name: "native trial".into(),
            data_root: root.to_str().unwrap().into(),
        })
        .await
        .unwrap();
    (instance.id, executable, resources)
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_entry_plan_uses_actual_body_with_resources_cwd_and_direct_session() {
    let f = Fixture::new().await;
    let (instance, executable, resources) = native_fixture(&f).await;
    let info = resources.parent().unwrap().join("Info.plist");
    std::fs::write(&info, b"fixture metadata, not interpreted or rewritten").unwrap();
    let root = f.temp.path().join("native-instance");
    let profile = "cdda-0i-1-macos-native-trial-v1";
    let plan = f
        .hub
        .instance_launch_plan(&instance, profile)
        .await
        .unwrap();
    assert_eq!(
        plan.executable,
        executable.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        plan.working_dir,
        resources.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(plan.environment.len(), 10);
    assert!(!plan.save_binding_verified);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    let session = f.hub.launch(&plan.id, true).await.unwrap();
    assert_eq!(session.play_instance_id.as_deref(), Some(instance.as_str()));
    assert_eq!(session.launch_profile_id.as_deref(), Some(profile));
    let mut exited = false;
    for _ in 0..100 {
        let data = f.hub.dashboard().await.unwrap();
        let current = data.sessions.iter().find(|s| s.id == session.id).unwrap();
        if current.state == "exited_unverified" {
            assert_eq!(current.exit_code, Some(0));
            exited = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(exited);
    assert!(root.join(".hub-runtime/home").is_dir());
    assert_eq!(
        std::fs::read(&info).unwrap(),
        b"fixture metadata, not interpreted or rewritten"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_entry_rejects_wrappers_and_body_changes_before_launch() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new().await;
    let (instance, executable, _) = native_fixture(&f).await;
    let plan = f
        .hub
        .instance_launch_plan(&instance, "cdda-0h-release-macos-native-trial-v1")
        .await
        .unwrap();
    std::fs::write(&executable, b"#!/bin/sh\nexit 0\n").unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "EXECUTABLE_CHANGED"
    );
    let wrapper = executable.with_file_name("wrapper");
    std::fs::write(&wrapper, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let installation = f
        .hub
        .import_installation(wrapper.to_str().unwrap(), "wrapper fixture")
        .await
        .unwrap();
    let root = f.temp.path().join("wrapper-instance");
    std::fs::create_dir(&root).unwrap();
    let instance = f
        .hub
        .register_play_instance(PlayInstanceInput {
            installation_id: installation.id,
            name: "wrapper".into(),
            data_root: root.to_str().unwrap().into(),
        })
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .instance_launch_plan(&instance.id, "cdda-0h-release-macos-native-trial-v1")
            .await
            .unwrap_err()
            .code,
        "BUNDLE_NATIVE_ENTRY_REQUIRED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_entry_rejects_resources_redirect_outside_its_bundle() {
    let f = Fixture::new().await;
    let (instance, _, resources) = native_fixture(&f).await;
    let external = f.temp.path().join("external-resources");
    std::fs::rename(&resources, &external).unwrap();
    std::os::unix::fs::symlink(external, &resources).unwrap();
    assert_eq!(
        f.hub
            .instance_launch_plan(&instance, "cdda-0i-1-macos-native-trial-v1")
            .await
            .unwrap_err()
            .code,
        "BUNDLE_LAYOUT_UNSUPPORTED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn native_entry_rejects_identical_body_in_replaced_macos_directory() {
    let f = Fixture::new().await;
    let (instance, executable, _) = native_fixture(&f).await;
    let plan = f
        .hub
        .instance_launch_plan(&instance, "cdda-0i-1-macos-native-trial-v1")
        .await
        .unwrap();
    let parent = executable.parent().unwrap();
    let old_parent = parent.with_file_name("MacOS-before");
    std::fs::rename(parent, &old_parent).unwrap();
    std::fs::create_dir(parent).unwrap();
    std::fs::copy(
        old_parent.join(executable.file_name().unwrap()),
        &executable,
    )
    .unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "INSTANCE_PATH_CHANGED"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert_eq!(
        std::fs::read_dir(f.temp.path().join("native-instance"))
            .unwrap()
            .count(),
        0
    );
}
