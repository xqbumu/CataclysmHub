use super::*;
#[cfg(unix)]
use crate::{Hub, PlayInstanceInput, WorkspaceInput};

#[cfg(unix)]
struct Fixture {
    temp: tempfile::TempDir,
    hub: Arc<Hub>,
    instances: Vec<String>,
}
#[cfg(unix)]
impl Fixture {
    async fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        for name in ["save", "backup", "content"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
        }
        let path = |s: &str| temp.path().join(s).to_str().unwrap().to_owned();
        hub.initialize(WorkspaceInput {
            save_dir: path("save"),
            backup_dir: path("backup"),
            content_dir: path("content"),
        })
        .await
        .unwrap();
        let mut instances = Vec::new();
        for index in 0..3 {
            let game = temp.path().join(format!("game-{index}"));
            let data = temp.path().join(format!("instance-{index}"));
            std::fs::create_dir(&game).unwrap();
            std::fs::create_dir(&data).unwrap();
            let executable = game.join("game");
            std::fs::write(&executable, b"#!/bin/sh\nroot=$2\nprintf started > \"$root/started\"\ni=0\nwhile [ ! -f \"$root/release\" ] && [ $i -lt 500 ]; do /bin/sleep 0.02; i=$((i+1)); done\nif [ -f \"$root/fail\" ]; then exit 7; fi\nexit 0\n").unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(game.join("static.json"), b"fixture resources").unwrap();
            let installation = hub
                .import_installation(executable.to_str().unwrap(), "synthetic")
                .await
                .unwrap();
            let instance = hub
                .register_play_instance(PlayInstanceInput {
                    installation_id: installation.id,
                    name: format!("{index}"),
                    data_root: data.to_str().unwrap().into(),
                })
                .await
                .unwrap();
            instances.push(instance.id);
        }
        Self {
            temp,
            hub,
            instances,
        }
    }
    async fn plan(&self, index: usize) -> crate::LaunchPlan {
        self.hub
            .instance_launch_plan(&self.instances[index], "synthetic-candidate")
            .await
            .unwrap()
    }
    fn release(&self, index: usize) {
        std::fs::write(
            self.temp.path().join(format!("instance-{index}/release")),
            b"",
        )
        .unwrap();
    }
    async fn wait(&self, id: &str, state: &str) {
        for _ in 0..200 {
            if self
                .hub
                .dashboard()
                .await
                .unwrap()
                .sessions
                .iter()
                .any(|s| s.id == id && s.state == state)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("session did not reach {state}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn independent_candidates_run_together_and_exit_releases_only_its_owner() {
    let f = Fixture::new().await;
    let a = f.plan(0).await;
    let b = f.plan(1).await;
    let (a, b) = tokio::join!(f.hub.launch(&a.id, true), f.hub.launch(&b.id, true));
    let a = a.unwrap();
    let b = b.unwrap();
    let claims = f.hub.dashboard().await.unwrap().resource_claims;
    assert_eq!(claims.len(), 4);
    assert!(claims.iter().all(|c| c.scope == "directory"));
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[0], "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    // Ordinary launches and operations using the global gate stay conservative.
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[2], "cdda-0h-source-trial-v1")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub.check_resource_claims().await.unwrap_err().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    f.release(0);
    f.wait(&a.id, "exited_unverified").await;
    let claims = f.hub.dashboard().await.unwrap().resource_claims;
    assert_eq!(claims.len(), 2);
    assert!(claims.iter().all(|c| c.owner_id == b.id));
    f.plan(0).await;
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[1], "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    f.release(1);
    f.wait(&b.id, "exited_unverified").await;
    assert!(f.hub.dashboard().await.unwrap().resource_claims.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn simultaneous_same_instance_and_shared_installation_are_rejected() {
    let f = Fixture::new().await;
    let installation = f
        .hub
        .instance(&f.instances[0])
        .await
        .unwrap()
        .installation_id;
    let shared = f.temp.path().join("shared-package-instance");
    std::fs::create_dir(&shared).unwrap();
    let shared = f
        .hub
        .register_play_instance(PlayInstanceInput {
            installation_id: installation,
            name: "shared".into(),
            data_root: shared.to_str().unwrap().into(),
        })
        .await
        .unwrap();
    let a = f.plan(0).await;
    let b = f.plan(0).await;
    let (a, b) = tokio::join!(f.hub.launch(&a.id, true), f.hub.launch(&b.id, true));
    let results = [a, b];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results.iter().find_map(|r| r.as_ref().err()).unwrap().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub
            .instance_launch_plan(&shared.id, "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    let id = &results.iter().find_map(|r| r.as_ref().ok()).unwrap().id;
    f.release(0);
    f.wait(id, "exited_unverified").await;
    assert_eq!(f.hub.dashboard().await.unwrap().sessions.len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn static_changes_and_failed_intent_never_spawn() {
    let f = Fixture::new().await;
    let plan = f.plan(0).await;
    std::fs::write(
        f.temp.path().join("game-0/static.json"),
        b"changed resources",
    )
    .unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "PLAN_STALE"
    );
    let plan = f.plan(1).await;
    sqlx::query("CREATE TRIGGER refuse_candidate BEFORE INSERT ON operation_log WHEN NEW.action='game.launch' BEGIN SELECT RAISE(ABORT,'test'); END").execute(&f.hub.pool).await.unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "DATABASE_ERROR"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
    assert!(f.hub.candidate_sessions.lock().await.is_empty());
    assert!(!f.temp.path().join("instance-0/started").exists());
    assert!(!f.temp.path().join("instance-1/started").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn failed_running_record_keeps_global_protection() {
    let f = Fixture::new().await;
    let plan = f.plan(0).await;
    sqlx::query("CREATE TRIGGER refuse_running BEFORE UPDATE ON session WHEN NEW.state='running' BEGIN SELECT RAISE(ABORT,'test'); END").execute(&f.hub.pool).await.unwrap();
    assert_eq!(
        f.hub.launch(&plan.id, true).await.unwrap_err().code,
        "LAUNCH_STATUS_UNKNOWN"
    );
    let data = f.hub.dashboard().await.unwrap();
    assert!(data.resource_claims.iter().any(|c| c.scope == "global"));
    assert!(f.hub.candidate_sessions.lock().await.is_empty());
    f.release(0);
    f.wait(&data.sessions[0].id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn failed_result_reverts_to_global_without_releasing_other_session() {
    let f = Fixture::new().await;
    let a = f.hub.launch(&f.plan(0).await.id, true).await.unwrap();
    let b = f.hub.launch(&f.plan(1).await.id, true).await.unwrap();
    sqlx::query("CREATE TRIGGER refuse_result BEFORE INSERT ON operation_log WHEN NEW.action='game.process_result' BEGIN SELECT RAISE(ABORT,'test'); END").execute(&f.hub.pool).await.unwrap();
    f.release(0);
    for _ in 0..200 {
        if !f.hub.candidate_sessions.lock().await.contains_key(&a.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let data = f.hub.dashboard().await.unwrap();
    assert!(
        data.resource_claims
            .iter()
            .any(|c| c.owner_id == a.id && c.scope == "global")
    );
    assert_eq!(
        data.resource_claims
            .iter()
            .filter(|c| c.owner_id == b.id)
            .count(),
        2
    );
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[2], "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    sqlx::query("DROP TRIGGER refuse_result")
        .execute(&f.hub.pool)
        .await
        .unwrap();
    f.release(1);
    f.wait(&b.id, "exited_unverified").await;
    assert!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .all(|c| c.owner_id == a.id)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn abnormal_exit_and_reopen_cannot_recover_scoped_eligibility() {
    let f = Fixture::new().await;
    std::fs::write(f.temp.path().join("instance-0/fail"), b"").unwrap();
    let a = f.hub.launch(&f.plan(0).await.id, true).await.unwrap();
    f.release(0);
    f.wait(&a.id, "interrupted_unknown").await;
    assert!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.scope == "global")
    );
    assert!(f.hub.candidate_sessions.lock().await.is_empty());
    // Reopening durable state cannot recreate process supervision or eligibility.
    let metadata = f.temp.path().join("metadata");
    drop(f.hub);
    let hub = Hub::open(&metadata).await.unwrap();
    assert!(hub.candidate_sessions.lock().await.is_empty());
    assert!(
        hub.dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.scope == "global")
    );
    hub.acknowledge_interrupted(&a.id).await.unwrap();
    assert!(hub.dashboard().await.unwrap().resource_claims.is_empty());
}

#[cfg(unix)]
#[test]
fn fingerprint_changes_for_static_content_but_not_known_cache_content() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("Contents/Resources/data")).unwrap();
    std::fs::write(
        root.join("Contents/Resources/data/resource.json"),
        b"static",
    )
    .unwrap();
    let before = static_fingerprint(root).unwrap();
    for name in ["Contents/Resources/cache", "Contents/Resources/data/cache"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
        std::fs::write(root.join(name).join("generated.fb"), b"mutable").unwrap();
    }
    assert_eq!(static_fingerprint(root).unwrap(), before);
    std::fs::write(
        root.join("Contents/Resources/data/resource.json"),
        b"tamper",
    )
    .unwrap();
    assert_ne!(static_fingerprint(root).unwrap(), before);
}

#[tokio::test]
async fn production_candidate_never_accepts_arbitrary_package() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().to_str().unwrap();
    assert_eq!(
        prepare(H, path, path, path).await.err().unwrap().code,
        "CONCURRENCY_CANDIDATE_UNSUPPORTED"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn changed_live_root_falls_back_to_global() {
    let f = Fixture::new().await;
    let a = f.hub.launch(&f.plan(0).await.id, true).await.unwrap();
    let root = f.temp.path().join("instance-0");
    let moved = f.temp.path().join("moved-instance");
    std::fs::rename(&root, &moved).unwrap();
    std::fs::create_dir(&root).unwrap();
    assert!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.owner_id == a.id && c.scope == "global")
    );
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[1], "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    f.release(0);
    f.wait(&a.id, "exited_unverified").await;
}

#[cfg(all(
    feature = "candidate-concurrency",
    target_os = "macos",
    target_arch = "aarch64"
))]
#[tokio::test]
#[ignore = "read-only local QA package check; requires HUB_CANDIDATE_QA_ROOT"]
async fn local_qa_packages_match_pinned_candidates() {
    let root = PathBuf::from(std::env::var("HUB_CANDIDATE_QA_ROOT").unwrap());
    let f = Fixture::new().await;
    for (index, package, profile) in [(0, "H.app", H), (1, "Game.app", I)] {
        let executable = root.join(package).join("Contents/MacOS/cataclysm-hub-qa");
        let installation = f
            .hub
            .import_installation(executable.to_str().unwrap(), "local QA")
            .await
            .unwrap();
        let data = f.temp.path().join(format!("qa-instance-{index}"));
        std::fs::create_dir(&data).unwrap();
        let instance = f
            .hub
            .register_play_instance(PlayInstanceInput {
                installation_id: installation.id,
                name: "read-only QA".into(),
                data_root: data.to_str().unwrap().into(),
            })
            .await
            .unwrap();
        let plan = f
            .hub
            .instance_launch_plan(&instance.id, profile)
            .await
            .unwrap();
        assert!(!plan.save_binding_verified);
        assert!(plan.environment.contains_key("CFFIXED_USER_HOME"));
        assert_eq!(std::fs::read_dir(data).unwrap().count(), 0);
    }
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn active_file_task_is_not_bypassed_by_candidate() {
    let f = Fixture::new().await;
    sqlx::query("INSERT INTO task(id,request_key,workspace_id,operation,state,phase,created_at) SELECT 'active','active',id,'world.scan','running','scan','now' FROM workspace").execute(&f.hub.pool).await.unwrap();
    assert_eq!(
        f.hub
            .instance_launch_plan(&f.instances[0], "synthetic-candidate")
            .await
            .unwrap_err()
            .code,
        "TASK_CONFLICT"
    );
    assert!(f.hub.dashboard().await.unwrap().sessions.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn observed_exit_revokes_eligibility_even_while_mutation_gate_is_busy() {
    let f = Fixture::new().await;
    let a = f.hub.launch(&f.plan(0).await.id, true).await.unwrap();
    let gate = f.hub.mutation.lock().await;
    f.release(0);
    for _ in 0..200 {
        if !f.hub.candidate_sessions.lock().await.contains_key(&a.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(!f.hub.candidate_sessions.lock().await.contains_key(&a.id));
    assert!(
        f.hub
            .dashboard()
            .await
            .unwrap()
            .resource_claims
            .iter()
            .any(|c| c.owner_id == a.id && c.scope == "global")
    );
    drop(gate);
    f.wait(&a.id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn unrelated_instance_backup_can_finish_while_candidate_runs() {
    let f = Fixture::new().await;
    std::fs::write(f.temp.path().join("instance-1/save.txt"), b"saved world").unwrap();
    let a = f.plan(0).await;
    let a = f.hub.launch(&a.id, true).await.unwrap();
    assert_eq!(
        f.hub
            .prepare_instance_backup(&f.instances[0])
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub.prepare_backup().await.unwrap_err().code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    let plan = f
        .hub
        .prepare_instance_backup(&f.instances[1])
        .await
        .unwrap();
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
    for _ in 0..200 {
        let task = f.hub.backup_task(&task.id).await.unwrap();
        if task.state != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        f.hub.backup_task(&task.id).await.unwrap().state,
        "succeeded"
    );
    let data = f.hub.dashboard().await.unwrap();
    assert!(
        data.sessions
            .iter()
            .any(|s| s.id == a.id && s.state == "running")
    );
    assert!(data.resource_claims.iter().all(|c| c.owner_id == a.id));
    assert_eq!(
        std::fs::read(f.temp.path().join("instance-1/save.txt")).unwrap(),
        b"saved world"
    );
    f.release(0);
    f.wait(&a.id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn instance_backup_rechecks_session_and_destination_at_confirmation() {
    let f = Fixture::new().await;
    let plan = f
        .hub
        .prepare_instance_backup(&f.instances[0])
        .await
        .unwrap();
    let a = f.plan(0).await;
    let a = f.hub.launch(&a.id, true).await.unwrap();
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
    // Even a disjoint source cannot write an archive into the running game's root.
    let source = f.temp.path().join("instance-1");
    let destination = f.temp.path().join("instance-0");
    assert_eq!(
        f.hub
            .check_file_transfer_resources(source.to_str().unwrap(), destination.to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert!(f.hub.dashboard().await.unwrap().backup_tasks.is_empty());
    f.release(0);
    f.wait(&a.id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn instance_backup_blocks_unknown_candidate_and_unavailable_protected_root() {
    let f = Fixture::new().await;
    let a = f.plan(0).await;
    let a = f.hub.launch(&a.id, true).await.unwrap();
    // Wait until the shell has opened its script before moving its directory.
    let started = f.temp.path().join("instance-0/started");
    for _ in 0..200 {
        if started.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(started.exists());
    let root = f.temp.path().join("game-0");
    let moved = f.temp.path().join("moved-game");
    std::fs::rename(&root, &moved).unwrap();
    assert_eq!(
        f.hub
            .prepare_instance_backup(&f.instances[1])
            .await
            .unwrap_err()
            .code,
        "PATH_INVALID"
    );
    assert_eq!(
        f.hub
            .check_file_transfer_resources(
                f.temp.path().join("instance-1").to_str().unwrap(),
                f.temp.path().join("backup").to_str().unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    std::fs::rename(&moved, &root).unwrap();
    sqlx::query("UPDATE session SET state='interrupted_unknown' WHERE id=?")
        .bind(&a.id)
        .execute(&f.hub.pool)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .prepare_instance_backup(&f.instances[1])
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert!(f.hub.dashboard().await.unwrap().backup_tasks.is_empty());
    f.release(0);
    f.wait(&a.id, "exited_unverified").await;
}

#[cfg(unix)]
impl Fixture {
    async fn archive(&self, index: usize) -> String {
        std::fs::write(
            self.temp.path().join(format!("instance-{index}/saved.txt")),
            b"saved marker",
        )
        .unwrap();
        let plan = self
            .hub
            .prepare_instance_backup(&self.instances[index])
            .await
            .unwrap();
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
        for _ in 0..200 {
            if self.hub.backup_task(&task.id).await.unwrap().state != "running" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(
            self.hub.backup_task(&task.id).await.unwrap().state,
            "succeeded"
        );
        task.backup_id
    }
}

#[cfg(unix)]
#[tokio::test]
async fn restore_uses_actual_paths_not_archive_instance_association() {
    let f = Fixture::new().await;
    let archive = f.archive(0).await;
    let parent = f.temp.path().join("restore-parent");
    std::fs::create_dir(&parent).unwrap();
    let launch = f.plan(0).await;
    let session = f.hub.launch(&launch.id, true).await.unwrap();
    // An archive of the running instance is immutable input, not a read of its live saves.
    let plan = f
        .hub
        .prepare_restore(&archive, parent.to_str().unwrap())
        .await
        .unwrap();
    let task = f
        .hub
        .start_restore(
            &plan.id,
            &plan.plan_digest,
            &uuid::Uuid::new_v4().to_string(),
            true,
        )
        .await
        .unwrap();
    for _ in 0..200 {
        if f.hub.restore_task(&task.id).await.unwrap().state != "running" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        f.hub.restore_task(&task.id).await.unwrap().state,
        "succeeded"
    );
    assert_eq!(
        std::fs::read(Path::new(&plan.destination).join("data/saved.txt")).unwrap(),
        b"saved marker"
    );
    let data = f.hub.dashboard().await.unwrap();
    assert!(
        data.sessions
            .iter()
            .any(|s| s.id == session.id && s.state == "running")
    );
    assert!(
        data.resource_claims
            .iter()
            .all(|c| c.owner_id == session.id)
    );
    f.release(0);
    f.wait(&session.id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn restore_rechecks_running_target_and_unknown_session_at_confirmation() {
    let f = Fixture::new().await;
    let archive = f.archive(1).await;
    let parent = f.temp.path().join("instance-0");
    let plan = f
        .hub
        .prepare_restore(&archive, parent.to_str().unwrap())
        .await
        .unwrap();
    let launch = f.plan(0).await;
    let session = f.hub.launch(&launch.id, true).await.unwrap();
    assert_eq!(
        f.hub
            .prepare_restore(&archive, parent.to_str().unwrap())
            .await
            .unwrap_err()
            .code,
        "GAME_RUNNING_OR_UNKNOWN"
    );
    assert_eq!(
        f.hub
            .start_restore(
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
    assert!(!Path::new(&plan.destination).exists());
    let independent = f.temp.path().join("restore-parent");
    std::fs::create_dir(&independent).unwrap();
    let plan = f
        .hub
        .prepare_restore(&archive, independent.to_str().unwrap())
        .await
        .unwrap();
    sqlx::query("UPDATE session SET state='interrupted_unknown' WHERE id=?")
        .bind(&session.id)
        .execute(&f.hub.pool)
        .await
        .unwrap();
    assert_eq!(
        f.hub
            .start_restore(
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
    assert!(!Path::new(&plan.destination).exists());
    assert!(f.hub.dashboard().await.unwrap().restore_tasks.is_empty());
    f.release(0);
    f.wait(&session.id, "exited_unverified").await;
}

#[cfg(unix)]
#[tokio::test]
async fn restore_rejects_replaced_archive_directory_before_task_admission() {
    let f = Fixture::new().await;
    let archive = f.archive(1).await;
    let parent = f.temp.path().join("restore-parent");
    std::fs::create_dir(&parent).unwrap();
    let plan = f
        .hub
        .prepare_restore(&archive, parent.to_str().unwrap())
        .await
        .unwrap();
    let archive_path = f.temp.path().join("backup").join(&archive);
    std::fs::rename(&archive_path, f.temp.path().join("old-archive")).unwrap();
    std::fs::create_dir(&archive_path).unwrap();
    assert_eq!(
        f.hub
            .start_restore(
                &plan.id,
                &plan.plan_digest,
                &uuid::Uuid::new_v4().to_string(),
                true
            )
            .await
            .unwrap_err()
            .code,
        "PLAN_STALE"
    );
    assert!(!Path::new(&plan.destination).exists());
    assert!(f.hub.dashboard().await.unwrap().restore_tasks.is_empty());
}
