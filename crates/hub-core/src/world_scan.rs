use crate::{
    Hub, HubError, Result, ScanEntry, ScanIssue, ScanReport, ScanTask, WorldCandidate,
    database::{id, now},
    paths,
};
use cap_std::fs::Dir;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const TASK_COLUMNS: &str = "id,request_key,workspace_id,operation,state,phase,processed_entries,revision,error_code,created_at,finished_at";
#[derive(Clone, Copy)]
struct ScanLimits {
    entries: u32,
    depth: usize,
}
const LIMITS: ScanLimits = ScanLimits {
    entries: 20_000,
    depth: 32,
};

struct Inventory<'a> {
    report: ScanReport,
    cancelled: &'a AtomicBool,
    progress: &'a AtomicU64,
    limits: ScanLimits,
}
impl Inventory<'_> {
    fn issue(&mut self, code: &str, relative_path: &str, incomplete: bool) {
        self.report.issues.push(ScanIssue {
            code: code.into(),
            relative_path: relative_path.into(),
        });
        if incomplete {
            self.report.enumeration_complete = false;
        }
    }
    fn visit(
        &mut self,
        directory: &Dir,
        parent: &str,
        candidate: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(HubError::new(
                "TASK_CANCELLED",
                "扫描已取消，没有发布新扫描结果。",
            ));
        }
        let entries = match directory.entries() {
            Ok(entries) => entries,
            Err(_) => {
                self.issue("DIRECTORY_UNREADABLE", parent, true);
                return Ok(());
            }
        };
        for entry in entries {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(HubError::new(
                    "TASK_CANCELLED",
                    "扫描已取消，没有发布新扫描结果。",
                ));
            }
            if self.report.visited_entries >= self.limits.entries {
                if !self
                    .report
                    .issues
                    .iter()
                    .any(|issue| issue.code == "ENTRY_LIMIT")
                {
                    self.issue("ENTRY_LIMIT", parent, true);
                }
                return Ok(());
            }
            self.report.visited_entries += 1;
            self.progress
                .store(u64::from(self.report.visited_entries), Ordering::Release);
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.issue("ENTRY_UNREADABLE", parent, true);
                    continue;
                }
            };
            let native_name = entry.file_name();
            let Some(name) = native_name.to_str() else {
                self.issue("NON_UTF8_NAME", parent, true);
                continue;
            };
            // Windows can contain literal backslashes only as separators; Unix can contain them
            // in a name. Do not produce a misleading portable path for such names.
            if name.contains('\\') || name.chars().any(char::is_control) {
                self.issue("UNSUPPORTED_NAME", parent, true);
                continue;
            }
            let relative = if parent.is_empty() {
                name.to_owned()
            } else {
                format!("{parent}/{name}")
            };
            let metadata = match directory.symlink_metadata(&native_name) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.issue("ENTRY_UNREADABLE", &relative, true);
                    continue;
                }
            };
            let file_type = metadata.file_type();
            let kind = if file_type.is_symlink() {
                "symlink"
            } else if file_type.is_dir() {
                "directory"
            } else if file_type.is_file() {
                "file"
            } else {
                "unsupported"
            };
            let owner = if parent.is_empty() && kind == "directory" {
                Some(relative.as_str())
            } else {
                candidate
            };
            let length = if kind == "file" { metadata.len() } else { 0 };
            self.report.entries.push(ScanEntry {
                relative_path: relative.clone(),
                file_type: kind.into(),
                size: length.to_string(),
                candidate_root: owner.map(str::to_owned),
            });
            if parent.is_empty() && kind == "directory" {
                self.report.candidates.push(WorldCandidate {
                    id: String::new(),
                    relative_root: relative.clone(),
                    file_count: 0,
                    entry_count: 0,
                    total_bytes: "0".into(),
                    identity_confidence: "unknown".into(),
                    boundary_verified: false,
                });
            }
            match kind {
                "directory" => {
                    if depth >= self.limits.depth {
                        self.issue("DEPTH_LIMIT", &relative, true);
                        continue;
                    }
                    // Resolve through an open directory capability, never through an ambient
                    // joined absolute path. Raced symlinks cannot escape this directory scope.
                    match directory.open_dir(&native_name) {
                        Ok(child) => self.visit(&child, &relative, owner, depth + 1)?,
                        Err(_) => self.issue("DIRECTORY_UNREADABLE", &relative, true),
                    }
                }
                "symlink" => self.issue("LINK_NOT_FOLLOWED", &relative, false),
                "unsupported" => self.issue("UNSUPPORTED_FILE_TYPE", &relative, false),
                _ => {}
            }
        }
        Ok(())
    }
}
fn inventory(
    root: &Path,
    task_id: &str,
    cancelled: &AtomicBool,
    progress: &AtomicU64,
    limits: ScanLimits,
) -> Result<ScanReport> {
    let directory = Dir::open_ambient_dir(root, cap_std::ambient_authority()).map_err(|_| {
        HubError::new(
            "SAVE_ROOT_UNAVAILABLE",
            "存档目录不可访问，请检查目录、磁盘和权限。",
        )
    })?;
    inventory_directory(directory, task_id, cancelled, progress, limits)
}
fn empty_report(task_id: &str) -> ScanReport {
    ScanReport {
        task_id: task_id.into(),
        scanner_version: "opaque-directory-v1".into(),
        created_at: now(),
        enumeration_complete: true,
        visited_entries: 0,
        total_bytes: "0".into(),
        candidates: vec![],
        entries: vec![],
        issues: vec![],
    }
}
fn inventory_directory(
    directory: Dir,
    task_id: &str,
    cancelled: &AtomicBool,
    progress: &AtomicU64,
    limits: ScanLimits,
) -> Result<ScanReport> {
    let report = empty_report(task_id);
    let mut scan = Inventory {
        report,
        cancelled,
        progress,
        limits,
    };
    scan.visit(&directory, "", None, 0)?;
    scan.report
        .entries
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    scan.report
        .candidates
        .sort_by(|a, b| a.relative_root.cmp(&b.relative_root));
    scan.report
        .issues
        .sort_by(|a, b| (&a.relative_path, &a.code).cmp(&(&b.relative_path, &b.code)));
    scan.report.total_bytes = scan
        .report
        .entries
        .iter()
        .filter_map(|entry| entry.size.parse::<u128>().ok())
        .sum::<u128>()
        .to_string();
    let mut counts = std::collections::HashMap::<String, (u32, u32, u128)>::new();
    for entry in &scan.report.entries {
        if let Some(root) = &entry.candidate_root {
            let total = counts.entry(root.clone()).or_default();
            total.0 += 1;
            if entry.file_type == "file" {
                total.1 += 1;
                total.2 += entry.size.parse::<u128>().unwrap_or(0);
            }
        }
    }
    for candidate in &mut scan.report.candidates {
        if let Some((entries, files, bytes)) = counts.get(&candidate.relative_root) {
            candidate.entry_count = *entries;
            candidate.file_count = *files;
            candidate.total_bytes = bytes.to_string();
        }
    }
    Ok(scan.report)
}
pub(crate) fn inspect_source(root: &Path, cancelled: &AtomicBool) -> Result<ScanReport> {
    inventory(root, "preflight", cancelled, &AtomicU64::new(0), LIMITS)
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct InstanceSaveReport {
    pub instance_id: String,
    pub data_root: String,
    pub save_dir: String,
    pub report: ScanReport,
}

impl Hub {
    /// Read only, scoped to a registered instance; never merges workspace history.
    pub async fn inspect_instance_saves(&self, instance_id: &str) -> Result<InstanceSaveReport> {
        let _guard = self.mutation.lock().await;
        self.check_resource_claims().await?;
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        let identity = self
            .instance_identity(
                &instance,
                &installation.working_dir,
                &installation.executable,
            )
            .await?;
        let root = std::path::PathBuf::from(&instance.data_root);
        let save = root.join("save");
        let save_dir = paths::text(&save)?;
        let report = tokio::task::spawn_blocking(move || {
            let directory = Dir::open_ambient_dir(&root, cap_std::ambient_authority())
                .map_err(|_| HubError::new("PATH_INVALID", "实例目录不可访问。"))?;
            use cap_fs_ext::DirExt;
            let save_directory = match directory.open_dir_nofollow("save") {
                Ok(dir) => Some(dir),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return Err(HubError::new(
                        "SAVE_ROOT_UNAVAILABLE",
                        "save 必须是可访问的普通目录，不能是链接。",
                    ));
                }
            };
            let report = match save_directory {
                Some(dir) => inventory_directory(
                    dir,
                    "instance",
                    &AtomicBool::new(false),
                    &AtomicU64::new(0),
                    LIMITS,
                )?,
                None => empty_report("instance"),
            };
            if same_file::Handle::from_path(&root).ok().as_ref() != Some(&identity.0) {
                return Err(HubError::new(
                    "INSTANCE_PATH_CHANGED",
                    "实例目录在扫描期间已变化。",
                ));
            }
            Ok(report)
        })
        .await
        .map_err(|_| HubError::new("SCAN_FAILED", "扫描未完成。"))??;
        Ok(InstanceSaveReport {
            instance_id: instance.id,
            data_root: instance.data_root,
            save_dir,
            report,
        })
    }
    pub async fn start_world_scan(self: &Arc<Self>, request_key: &str) -> Result<ScanTask> {
        let _guard = self.mutation.lock().await;
        if uuid::Uuid::parse_str(request_key).is_err() {
            return Err(HubError::new(
                "REQUEST_KEY_INVALID",
                "扫描请求标识无效，请重新发起扫描。",
            ));
        }
        let query = format!("SELECT {TASK_COLUMNS} FROM task WHERE request_key=?");
        if let Some(existing) = sqlx::query_as(&query)
            .bind(request_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
        {
            return Ok(existing);
        }
        self.check_resource_claims().await?;
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let root = paths::canonical(&workspace.save_dir, true)?;
        if paths::text(&root)? != workspace.save_dir {
            return Err(HubError::new(
                "SAVE_ROOT_CHANGED",
                "存档目录的实际位置发生变化，扫描已停止。",
            ));
        }
        let task_id = id();
        let created_at = now();
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO task (id,request_key,workspace_id,operation,state,phase,created_at) VALUES (?,?,?,'world.scan','running','enumerating',?)")
            .bind(&task_id).bind(request_key).bind(&workspace.id).bind(&created_at).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&task_id)
            .bind("world.scan")
            .bind("running")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.scan_controls
            .lock()
            .await
            .insert(task_id.clone(), Arc::clone(&cancelled));
        let hub = Arc::clone(self);
        let worker_id = task_id.clone();
        let workspace_id = workspace.id.clone();
        tokio::spawn(async move {
            let progress = Arc::new(AtomicU64::new(0));
            let scan_progress = Arc::clone(&progress);
            let worker_cancelled = Arc::clone(&cancelled);
            let scan_id = worker_id.clone();
            let mut worker = tokio::task::spawn_blocking(move || {
                inventory(&root, &scan_id, &worker_cancelled, &scan_progress, LIMITS)
            });
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut progress_failed = false;
            let result = loop {
                tokio::select! {
                    result = &mut worker => break result.unwrap_or_else(|_| Err(HubError::new("SCAN_WORKER_FAILED", "扫描工作线程意外退出。"))),
                    _ = ticker.tick() => {
                        let processed = progress.load(Ordering::Acquire) as i64;
                        if sqlx::query("UPDATE task SET processed_entries=?,revision=revision+1 WHERE id=? AND state='running' AND processed_entries<>?")
                            .bind(processed).bind(&worker_id).bind(processed).execute(&hub.pool).await.is_err() {
                            progress_failed = true;
                        }
                    }
                }
            };
            let result = if progress_failed {
                Err(HubError::new(
                    "TASK_PROGRESS_PERSIST_FAILED",
                    "扫描进度记录失败，请重新扫描。",
                ))
            } else {
                result
            };
            if hub
                .finish_world_scan(&worker_id, &workspace_id, result, &cancelled)
                .await
                .is_err()
            {
                // No success report was published; startup reconciliation will expose the intent.
                hub.mark_task_recovery(crate::task_recovery::TaskKind::Scan, &worker_id)
                    .await;
            }
            hub.scan_controls.lock().await.remove(&worker_id);
        });
        Ok(ScanTask {
            id: task_id,
            request_key: request_key.into(),
            workspace_id: workspace.id,
            operation: "world.scan".into(),
            state: "running".into(),
            phase: "enumerating".into(),
            processed_entries: 0,
            revision: 1,
            error_code: None,
            created_at,
            finished_at: None,
        })
    }

    async fn finish_world_scan(
        &self,
        task_id: &str,
        workspace_id: &str,
        result: Result<ScanReport>,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let _guard = self.mutation.lock().await;
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        let result = if cancelled.load(Ordering::Acquire) {
            Err(HubError::new("TASK_CANCELLED", "扫描已取消。"))
        } else {
            result
        };
        let (state, error, report_json, visited) = match result {
            Ok(mut report) => {
                for candidate in &mut report.candidates {
                    let candidate_id: String = sqlx::query_scalar("INSERT INTO world_candidate (id,workspace_id,relative_root,last_seen_task_id) VALUES (?,?,?,?) ON CONFLICT(workspace_id,relative_root) DO UPDATE SET last_seen_task_id=excluded.last_seen_task_id RETURNING id")
                        .bind(id()).bind(workspace_id).bind(&candidate.relative_root).bind(task_id).fetch_one(&mut *tx).await.map_err(HubError::db)?;
                    candidate.id = candidate_id;
                }
                let count = i64::from(report.visited_entries);
                let serialized = serde_json::to_string(&report).map_err(|_| {
                    HubError::new("SCAN_SERIALIZATION_FAILED", "无法保存扫描结果。")
                })?;
                ("succeeded", None, Some(serialized), count)
            }
            Err(error) => (
                if error.code == "TASK_CANCELLED" {
                    "cancelled"
                } else {
                    "failed"
                },
                Some(error.code),
                None,
                0,
            ),
        };
        sqlx::query("UPDATE task SET state=?,phase='finished',result_json=?,error_code=?,processed_entries=MAX(processed_entries,?),finished_at=?,revision=revision+1 WHERE id=?")
            .bind(state).bind(report_json).bind(error).bind(visited).bind(now()).bind(task_id).execute(&mut *tx).await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(task_id)
            .bind("world.scan")
            .bind(state)
            .bind(error)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(())
    }
    pub async fn cancel_scan(&self, task_id: &str) -> Result<ScanTask> {
        let _guard = self.mutation.lock().await;
        let task = self.scan_task(task_id).await?;
        if task.state != "running" && task.state != "cancelling" {
            return Ok(task);
        }
        let controls = self.scan_controls.lock().await;
        let flag = controls.get(task_id).ok_or(HubError::new(
            "TASK_STATE_UNKNOWN",
            "扫描任务没有活动工作线程，请重启后检查任务状态。",
        ))?;
        sqlx::query(
            "UPDATE task SET state='cancelling',revision=revision+1 WHERE id=? AND state='running'",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(HubError::db)?;
        flag.store(true, Ordering::Release);
        self.scan_task(task_id).await
    }
    pub async fn scan_task(&self, task_id: &str) -> Result<ScanTask> {
        let query = format!("SELECT {TASK_COLUMNS} FROM task WHERE id=?");
        let mut task = sqlx::query_as(&query)
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new("TASK_NOT_FOUND", "找不到该扫描任务。"))?;
        self.overlay_scan(&mut task).await;
        Ok(task)
    }
    pub async fn scan_report(&self, task_id: &str) -> Result<ScanReport> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT result_json FROM task WHERE id=? AND state='succeeded'")
                .bind(task_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(HubError::db)?
                .flatten();
        serde_json::from_str(&value.ok_or(HubError::new(
            "SCAN_RESULT_UNAVAILABLE",
            "此扫描尚无可用结果，请检查任务状态。",
        ))?)
        .map_err(|_| HubError::new("SCAN_RESULT_INVALID", "已保存的扫描报告损坏，请重新扫描。"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_inventory_is_incomplete_and_never_claims_verified_worlds() {
        let temp = tempfile::tempdir().unwrap();
        for i in 0..8 {
            std::fs::write(temp.path().join(format!("file-{i}")), "data").unwrap();
        }
        let report = inventory(
            temp.path(),
            "test",
            &AtomicBool::new(false),
            &AtomicU64::new(0),
            ScanLimits {
                entries: 3,
                depth: 2,
            },
        )
        .unwrap();
        assert!(!report.enumeration_complete);
        assert_eq!(report.visited_entries, 3);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "ENTRY_LIMIT")
        );
    }
    #[test]
    fn precancelled_inventory_never_publishes_a_report() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            inventory(
                temp.path(),
                "test",
                &AtomicBool::new(true),
                &AtomicU64::new(0),
                LIMITS
            )
            .unwrap_err()
            .code,
            "TASK_CANCELLED"
        );
    }
}
