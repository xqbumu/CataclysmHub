use hub_core::{Hub, WorkspaceInput};
use std::{path::PathBuf, sync::Arc};
use tempfile::TempDir;

struct Fixture {
    _temp: TempDir,
    data: PathBuf,
    input: WorkspaceInput,
    hub: Arc<Hub>,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("metadata");
        let save = temp.path().join("saves");
        let backup = temp.path().join("backups");
        let content = temp.path().join("content");
        for dir in [&save, &backup, &content] {
            std::fs::create_dir(dir).unwrap();
        }
        let hub = Hub::open(&data).await.unwrap();
        Self {
            _temp: temp,
            data,
            input: WorkspaceInput {
                save_dir: save.to_str().unwrap().into(),
                backup_dir: backup.to_str().unwrap().into(),
                content_dir: content.to_str().unwrap().into(),
            },
            hub,
        }
    }
    #[cfg(unix)]
    fn script(&self, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self._temp.path().join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
}

#[tokio::test]
async fn initialization_is_persistent_and_cannot_overwrite_existing_paths() {
    let fixture = Fixture::new().await;
    let marker = PathBuf::from(&fixture.input.save_dir).join("world.dat");
    std::fs::write(&marker, b"original save bytes").unwrap();
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    assert_eq!(
        fixture
            .hub
            .initialize(fixture.input.clone())
            .await
            .unwrap_err()
            .code,
        "WORKSPACE_EXISTS"
    );
    let saved = fixture.hub.dashboard().await.unwrap();
    assert_eq!(saved.operations.len(), 1);
    drop(fixture.hub);
    let reopened = Hub::open(&fixture.data).await.unwrap();
    assert_eq!(
        reopened.dashboard().await.unwrap().workspace.unwrap().id,
        saved.workspace.unwrap().id
    );
    assert_eq!(std::fs::read(marker).unwrap(), b"original save bytes");
}
#[tokio::test]
async fn only_one_writer_can_open_the_metadata_directory() {
    let fixture = Fixture::new().await;
    let result = Hub::open(&fixture.data).await;
    assert!(matches!(result, Err(error) if error.code == "WORKSPACE_LOCKED"));
}
#[tokio::test]
async fn nested_paths_are_rejected_without_creating_a_workspace() {
    let mut fixture = Fixture::new().await;
    let nested = PathBuf::from(&fixture.input.save_dir).join("backups");
    std::fs::create_dir(&nested).unwrap();
    fixture.input.backup_dir = nested.to_str().unwrap().into();
    assert_eq!(
        fixture
            .hub
            .initialize(fixture.input)
            .await
            .unwrap_err()
            .code,
        "PATH_OVERLAP"
    );
    assert!(fixture.hub.dashboard().await.unwrap().workspace.is_none());
}
#[cfg(unix)]
#[tokio::test]
async fn symlink_aliases_do_not_bypass_path_separation() {
    let mut fixture = Fixture::new().await;
    let alias = fixture._temp.path().join("alias");
    std::os::unix::fs::symlink(&fixture.input.save_dir, &alias).unwrap();
    fixture.input.backup_dir = alias.to_str().unwrap().into();
    assert_eq!(
        fixture
            .hub
            .initialize(fixture.input)
            .await
            .unwrap_err()
            .code,
        "PATH_OVERLAP"
    );
}
#[cfg(unix)]
#[tokio::test]
async fn importing_is_read_only_and_duplicate_import_is_idempotent() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let script = fixture.script("cdda", "#!/bin/sh\nexit 0\n");
    let bytes = std::fs::read(&script).unwrap();
    let first = fixture
        .hub
        .import_installation(script.to_str().unwrap(), "CDDA")
        .await
        .unwrap();
    let second = fixture
        .hub
        .import_installation(script.to_str().unwrap(), "CDDA again")
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.version, None);
    assert_eq!(std::fs::read(script).unwrap(), bytes);
    assert_eq!(
        fixture.hub.dashboard().await.unwrap().installations.len(),
        1
    );
}
#[cfg(unix)]
#[tokio::test]
async fn launch_uses_literal_arguments_requires_confirmation_and_records_exit() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let script = fixture.script(
        "cdda",
        "#!/bin/sh\nprintf '%s' \"$1\" > args-recorded\nexit 7\n",
    );
    let install = fixture
        .hub
        .import_installation(script.to_str().unwrap(), "Synthetic test")
        .await
        .unwrap();
    let argument = "$(touch injected-marker); with spaces";
    let plan = fixture
        .hub
        .launch_plan(&install.id, vec![argument.into()])
        .await
        .unwrap();
    assert!(!plan.save_binding_verified);
    assert!(!fixture._temp.path().join("args-recorded").exists());
    assert_eq!(
        fixture.hub.launch(&plan.id, false).await.unwrap_err().code,
        "CONFIRMATION_REQUIRED"
    );
    let launched = fixture.hub.launch(&plan.id, true).await.unwrap();
    assert_eq!(
        fixture.hub.launch(&plan.id, true).await.unwrap_err().code,
        "PLAN_STALE"
    );
    let mut completed = None;
    for _ in 0..100 {
        let data = fixture.hub.dashboard().await.unwrap();
        if data.sessions[0].state == "exited_unverified" {
            completed = Some(data);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let data = completed.expect("child exit should be observed");
    assert_eq!(data.sessions[0].exit_code, Some(7));
    assert_eq!(
        std::fs::read_to_string(fixture._temp.path().join("args-recorded")).unwrap(),
        argument
    );
    assert!(!fixture._temp.path().join("injected-marker").exists());
    assert!(
        data.operations
            .iter()
            .any(|op| op.task_id == launched.id && op.action == "game.process_result")
    );
}
#[cfg(unix)]
#[tokio::test]
async fn changing_an_executable_after_preview_blocks_launch() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let script = fixture.script("cdda", "#!/bin/sh\nexit 0\n");
    let install = fixture
        .hub
        .import_installation(script.to_str().unwrap(), "CDDA")
        .await
        .unwrap();
    let plan = fixture.hub.launch_plan(&install.id, vec![]).await.unwrap();
    std::fs::write(&script, "#!/bin/sh\ntouch should-not-run\n").unwrap();
    assert_eq!(
        fixture.hub.launch(&plan.id, true).await.unwrap_err().code,
        "EXECUTABLE_CHANGED"
    );
    assert!(fixture.hub.dashboard().await.unwrap().sessions.is_empty());
}
#[cfg(unix)]
#[tokio::test]
async fn spawn_failure_is_audited_instead_of_left_running() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let script = fixture.script("cdda", "#!/does-not-exist/interpreter\n");
    let install = fixture
        .hub
        .import_installation(script.to_str().unwrap(), "Broken")
        .await
        .unwrap();
    let plan = fixture.hub.launch_plan(&install.id, vec![]).await.unwrap();
    assert_eq!(
        fixture.hub.launch(&plan.id, true).await.unwrap_err().code,
        "SPAWN_FAILED"
    );
    let data = fixture.hub.dashboard().await.unwrap();
    assert_eq!(data.sessions[0].state, "failed");
    assert!(
        data.operations
            .iter()
            .any(|op| op.error_code.as_deref() == Some("SPAWN_FAILED"))
    );
}
#[tokio::test]
async fn restart_does_not_misreport_an_unfinished_process_as_exited() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.data.join("metadata.sqlite")),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO installation VALUES ('game','test','synthetic','cwd','hash',NULL,NULL,'test','test','2026-01-01')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO session (id,installation_id,state,pid,started_at) VALUES ('session','game','running',1234,'2026-01-01')").execute(&pool).await.unwrap();
    pool.close().await;
    drop(fixture.hub);
    let hub = Hub::open(&fixture.data).await.unwrap();
    let data = hub.dashboard().await.unwrap();
    assert_eq!(data.sessions[0].state, "interrupted_unknown");
    assert!(data.sessions[0].finished_at.is_none());
    assert_eq!(
        hub.launch_plan("game", vec![]).await.unwrap_err().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    hub.acknowledge_interrupted("session").await.unwrap();
    assert_eq!(
        hub.dashboard().await.unwrap().sessions[0].state,
        "resolved_unknown"
    );
}
#[tokio::test]
async fn newer_database_versions_are_refused_without_downgrading() {
    let fixture = Fixture::new().await;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(fixture.data.join("metadata.sqlite")),
    )
    .await
    .unwrap();
    sqlx::query("PRAGMA user_version = 99")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    drop(fixture.hub);
    assert!(
        matches!(Hub::open(&fixture.data).await, Err(error) if error.code == "SCHEMA_UNSUPPORTED")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_running_game_blocks_other_installations_in_the_unverified_workspace() {
    let fixture = Fixture::new().await;
    fixture.hub.initialize(fixture.input.clone()).await.unwrap();
    let first = fixture.script("first-game", "#!/bin/sh\nsleep 0.3\n");
    let second = fixture.script("second-game", "#!/bin/sh\nexit 0\n");
    let a = fixture
        .hub
        .import_installation(first.to_str().unwrap(), "first")
        .await
        .unwrap();
    let b = fixture
        .hub
        .import_installation(second.to_str().unwrap(), "second")
        .await
        .unwrap();
    let pa = fixture.hub.launch_plan(&a.id, vec![]).await.unwrap();
    let pb = fixture.hub.launch_plan(&b.id, vec![]).await.unwrap();
    fixture.hub.launch(&pa.id, true).await.unwrap();
    assert_eq!(
        fixture.hub.launch(&pb.id, true).await.unwrap_err().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    // Reap the synthetic child before the fixture directory is removed.
    for _ in 0..100 {
        if fixture.hub.dashboard().await.unwrap().sessions[0].state == "exited_unverified" {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("synthetic child should exit");
}
