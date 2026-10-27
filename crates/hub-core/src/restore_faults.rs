//! Unit-test-only subprocess rendezvous. This module and its call sites do not exist in application builds.
pub(crate) fn checkpoint(point: &str) {
    if std::env::var("HUB_TEST_RESTORE_CRASH_POINT").as_deref() != Ok(point) {
        return;
    }
    let path = std::env::var_os("HUB_TEST_RESTORE_RENDEZVOUS").expect("test rendezvous path");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    std::io::Write::write_all(&mut file, point.as_bytes()).unwrap();
    file.sync_all().unwrap();
    // The parent terminates this process at the observed durable boundary, never resumes the operation.
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use crate::{Hub, WorkspaceInput};
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
        sync::Arc,
        time::{Duration, Instant},
    };

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn tree(root: &Path) -> BTreeMap<PathBuf, String> {
        let mut values = BTreeMap::new();
        let mut pending = vec![root.to_owned()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
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
    #[test]
    fn restore_child_process() {
        let Some(root) = std::env::var_os("HUB_TEST_RESTORE_CHILD_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        runtime().block_on(async {
            let hub = Hub::open(&root.join("metadata")).await.unwrap();
            let backup_id = std::env::var("HUB_TEST_RESTORE_BACKUP_ID").unwrap();
            let plan = hub.prepare_restore(&backup_id, root.join("recovery").to_str().unwrap()).await.unwrap();
            let key = uuid::Uuid::new_v4().to_string();
            let request = serde_json::json!({"plan_id":plan.id,"digest":plan.plan_digest,"key":key,"destination":plan.destination});
            std::fs::write(root.join("request.json"), serde_json::to_vec(&request).unwrap()).unwrap();
            hub.start_restore(&plan.id, &plan.plan_digest, &key, true).await.unwrap();
            std::future::pending::<()>().await;
        });
    }
    fn crash_at(point: &str, committed: bool, report_present: bool, incomplete: bool) {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        for name in ["saves", "backups", "content", "recovery"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        std::fs::create_dir(root.join("saves/empty")).unwrap();
        std::fs::write(root.join("saves/alpha"), b"first\0unknown").unwrap();
        std::fs::write(root.join("saves/beta"), b"second\0unknown").unwrap();
        let source_before = tree(&root.join("saves"));
        let backup_id = runtime().block_on(async {
            let hub = Hub::open(&root.join("metadata")).await.unwrap();
            hub.initialize(WorkspaceInput {
                save_dir: root.join("saves").to_str().unwrap().into(),
                backup_dir: root.join("backups").to_str().unwrap().into(),
                content_dir: root.join("content").to_str().unwrap().into(),
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
                    "fixture backup failed or timed out: {current:?}"
                );
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            task.backup_id
        });
        let rendezvous = root.join("rendezvous");
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "restore_faults::tests::restore_child_process",
                    "--nocapture",
                ])
                .env("HUB_TEST_RESTORE_CHILD_ROOT", root)
                .env("HUB_TEST_RESTORE_BACKUP_ID", &backup_id)
                .env("HUB_TEST_RESTORE_CRASH_POINT", point)
                .env("HUB_TEST_RESTORE_RENDEZVOUS", &rendezvous)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while !rendezvous.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before {point}"
            );
            assert!(Instant::now() < deadline, "child did not reach {point}");
            std::thread::sleep(Duration::from_millis(5));
        }
        // create_new becomes visible before its short write; wait for the full rendezvous payload.
        while std::fs::read(&rendezvous).unwrap() != point.as_bytes() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        child.0.kill().unwrap();
        assert!(!child.0.wait().unwrap().success());
        let request: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
        let destination = PathBuf::from(request["destination"].as_str().unwrap());
        assert_eq!(
            destination.join("restore-verification.json").exists(),
            report_present
        );
        assert_eq!(
            destination.join("RESTORE_INCOMPLETE.json").exists(),
            incomplete
        );
        let target_before_restart = tree(&destination);
        let inspection =
            crate::verify_restored_directory(&root.join("backups").join(&backup_id), &destination);
        if ["after_incomplete_marker", "after_payload_file"].contains(&point) {
            assert_eq!(inspection.unwrap_err().code, "RESTORED_DATA_MISMATCH");
        } else {
            let inspection = inspection.unwrap();
            assert_eq!(inspection.verification.state, "verified");
            assert_eq!(inspection.incomplete_marker_present, incomplete);
            assert_eq!(inspection.historical_report_present, report_present);
        }
        assert_eq!(
            tree(&destination),
            target_before_restart,
            "read-only inspection changed the output"
        );
        if point == "after_payload_file" {
            assert!(destination.join("data/alpha").is_file());
            assert!(!destination.join("data/beta").exists());
        }
        runtime().block_on(async {
            let hub = Hub::open(&root.join("metadata")).await.unwrap();
            let task = hub.dashboard().await.unwrap().restore_tasks.remove(0);
            assert_eq!(
                task.state,
                if committed {
                    "succeeded"
                } else {
                    "recovery_required"
                }
            );
            assert_eq!(
                task.error_code.as_deref(),
                if committed {
                    None
                } else {
                    Some("TASK_INTERRUPTED")
                }
            );
            let replay = hub
                .start_restore(
                    request["plan_id"].as_str().unwrap(),
                    request["digest"].as_str().unwrap(),
                    request["key"].as_str().unwrap(),
                    true,
                )
                .await
                .unwrap();
            assert_eq!(replay.id, task.id);
            assert_eq!(replay.state, task.state);
            assert_eq!(
                hub.cancel_restore(&task.id).await.unwrap().state,
                task.state
            );
            hub.prepare_backup()
                .await
                .expect("no phantom worker may block the workspace");
            let audit = hub
                .dashboard()
                .await
                .unwrap()
                .operations
                .iter()
                .filter(|event| event.task_id == task.id)
                .count();
            assert_eq!(audit, 2); // intent + reconciliation, or intent + committed success
            drop(hub);
            let hub = Hub::open(&root.join("metadata")).await.unwrap();
            assert_eq!(
                hub.dashboard()
                    .await
                    .unwrap()
                    .operations
                    .iter()
                    .filter(|event| event.task_id == task.id)
                    .count(),
                audit
            );
        });
        assert_eq!(
            tree(&destination),
            target_before_restart,
            "restart or replay changed the recovery directory"
        );
        assert_eq!(
            tree(&root.join("saves")),
            source_before,
            "source bytes changed"
        );
        assert_eq!(std::fs::read_dir(root.join("recovery")).unwrap().count(), 1);
        crate::verify_backup_directory(&root.join("backups").join(backup_id)).unwrap();
    }
    #[test]
    fn kill_after_incomplete_marker() {
        crash_at("after_incomplete_marker", false, false, true);
    }
    #[test]
    fn kill_after_one_payload_file() {
        crash_at("after_payload_file", false, false, true);
    }
    #[test]
    fn kill_after_readback() {
        crash_at("after_readback", false, false, true);
    }
    #[test]
    fn kill_after_report() {
        crash_at("after_report", false, true, true);
    }
    #[test]
    fn kill_after_completion_marker() {
        crash_at("after_completion_marker", false, true, false);
    }
    #[test]
    fn kill_after_task_commit() {
        crash_at("after_task_commit", true, true, false);
    }
}
