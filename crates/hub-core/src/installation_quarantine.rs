//! Explicit installation-scoped quarantine handling; never automatic authorization.
use crate::{Hub, HubError, Result};
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct InstallationQuarantinePlan {
    pub id: String,
    pub installation_id: String,
    pub scope: String,
    pub executable_sha256: String,
    pub quarantine_entries: u32,
    pub backup_directory: String,
    pub expires_at: String,
}
#[derive(Debug, Serialize, TS)]
pub struct InstallationQuarantineResult {
    pub backup_directory: String,
    pub changed_entries: u32,
}
#[cfg(not(target_os = "macos"))]
impl Hub {
    pub async fn prepare_installation_quarantine(
        &self,
        _id: &str,
    ) -> Result<InstallationQuarantinePlan> {
        Err(HubError::new(
            "PLATFORM_UNSUPPORTED",
            "仅 macOS 支持此操作。",
        ))
    }
    pub async fn apply_installation_quarantine(
        &self,
        _id: &str,
        _confirmed: bool,
    ) -> Result<InstallationQuarantineResult> {
        Err(HubError::new(
            "PLATFORM_UNSUPPORTED",
            "仅 macOS 支持此操作。",
        ))
    }
}
#[cfg(target_os = "macos")]
pub(crate) use macos::Prepared;

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::{
        backup_format as format,
        database::{id, now},
        paths,
    };
    use cap_fs_ext::DirExt;
    use cap_std::fs::{Dir, DirBuilder, DirBuilderExt, OpenOptions, OpenOptionsExt};
    use std::{
        fs::File, io::Write, os::unix::fs::MetadataExt, path::Path, sync::atomic::AtomicBool,
    };
    use xattr::FileExt;
    const ATTRIBUTE: &str = "com.apple.quarantine";
    const AUDIT: &str = "installation-security";
    const MAX_RECORD: usize = 8 * 1024 * 1024;

    fn changed() -> HubError {
        HubError::new(
            "QUARANTINE_PLAN_STALE",
            "安装内容、身份或隔离状态已变化，请重新核对。",
        )
    }
    fn io(_: std::io::Error) -> HubError {
        HubError::new(
            "QUARANTINE_IO_FAILED",
            "无法检查或处理隔离属性；请保留属性备份与操作记录。",
        )
    }
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct Attribute {
        path: String,
        device: u64,
        inode: u64,
        links: u64,
        mode: u32,
        before: Option<Vec<u8>>,
    }
    struct Snapshot {
        attributes: Vec<Attribute>,
        content: String,
    }
    pub(crate) struct Prepared {
        pub plan: InstallationQuarantinePlan,
        root: Dir,
        snapshot: Snapshot,
    }
    fn open(root: &Dir, path: &str) -> Result<File> {
        if path.is_empty() {
            return Ok(root.try_clone().map_err(io)?.into_std_file());
        }
        let (parent, name) = format::parent(root, path)?;
        let meta = parent.symlink_metadata(&name).map_err(io)?;
        if meta.is_dir() {
            Ok(parent.open_dir_nofollow(name).map_err(io)?.into_std_file())
        } else {
            Ok(format::open_file(root, path)?.into_std())
        }
    }
    fn attribute(root: &Dir, path: &str) -> Result<Attribute> {
        let f = open(root, path)?;
        let m = f.metadata().map_err(io)?;
        if !m.is_dir() && (!m.is_file() || m.nlink() != 1) {
            return Err(changed());
        }
        let before = f.get_xattr(ATTRIBUTE).map_err(io)?;
        if before.as_ref().is_some_and(|v| v.len() > 4096) {
            return Err(changed());
        }
        Ok(Attribute {
            path: path.into(),
            device: m.dev(),
            inode: m.ino(),
            links: m.nlink(),
            mode: m.mode(),
            before,
        })
    }
    fn snapshot(root: &Dir) -> Result<Snapshot> {
        // Reuse the existing bounded, no-follow inventory and internal-link checks.
        let entries =
            crate::installation_copy::inventory(root, true, &AtomicBool::new(false), true)?;
        let content = format::sha(&serde_json::to_vec(&entries).map_err(|_| changed())?);
        let mut attributes = vec![attribute(root, "")?];
        for entry in entries {
            if entry.kind != "symlink" {
                attributes.push(attribute(root, &entry.relative_path)?);
            }
        }
        if serde_json::to_vec(&attributes)
            .map_err(|_| changed())?
            .len()
            > MAX_RECORD
        {
            return Err(changed());
        }
        Ok(Snapshot {
            attributes,
            content,
        })
    }
    fn identity_matches(f: &File, expected: &Attribute) -> Result<bool> {
        let m = f.metadata().map_err(io)?;
        Ok(m.dev() == expected.device
            && m.ino() == expected.inode
            && m.nlink() == expected.links
            && m.mode() == expected.mode)
    }
    fn rollback(root: &Dir, entries: &[&Attribute]) -> bool {
        let mut restored = true;
        for entry in entries.iter().rev() {
            let result = (|| -> Result<()> {
                let f = open(root, &entry.path)?;
                if !identity_matches(&f, entry)? {
                    return Err(changed());
                }
                let current = f.get_xattr(ATTRIBUTE).map_err(io)?;
                if current == entry.before {
                    return Ok(());
                }
                if current.is_some() {
                    return Err(changed());
                }
                f.set_xattr(ATTRIBUTE, entry.before.as_ref().ok_or_else(changed)?)
                    .map_err(io)?;
                if f.get_xattr(ATTRIBUTE).map_err(io)? != entry.before {
                    return Err(changed());
                }
                Ok(())
            })();
            restored &= result.is_ok();
        }
        restored
    }
    fn root_path_matches(root: &Dir, path: &Path) -> Result<()> {
        if std::fs::canonicalize(path).map_err(io)? != path {
            return Err(changed());
        }
        let current = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(io)?;
        let original = open(root, "")?.metadata().map_err(io)?;
        let at_path = open(&current, "")?.metadata().map_err(io)?;
        if original.dev() != at_path.dev() || original.ino() != at_path.ino() {
            return Err(changed());
        }
        Ok(())
    }
    fn revalidate(p: &Prepared) -> Result<()> {
        root_path_matches(&p.root, Path::new(&p.plan.scope))?;
        let check = snapshot(&p.root)?;
        if check.content != p.snapshot.content || check.attributes != p.snapshot.attributes {
            return Err(changed());
        }
        Ok(())
    }
    fn write_json(dir: &Dir, name: &str, value: &impl Serialize) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| changed())?;
        if bytes.len() > MAX_RECORD {
            return Err(changed());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut f = dir.open_with(name, &options).map_err(io)?;
        f.write_all(&bytes).map_err(io)?;
        f.sync_all().map_err(io)?;
        dir.try_clone()
            .map_err(io)?
            .into_std_file()
            .sync_all()
            .map_err(io)
    }
    fn apply(
        root: &Dir,
        expected: &Snapshot,
        remove: impl Fn(&File) -> std::io::Result<()>,
    ) -> Result<u32> {
        let mut attempted = Vec::new();
        let outcome = (|| -> Result<()> {
            for entry in &expected.attributes {
                if entry.before.is_none() {
                    continue;
                }
                let f = open(root, &entry.path)?;
                if !identity_matches(&f, entry)?
                    || f.get_xattr(ATTRIBUTE).map_err(io)? != entry.before
                {
                    return Err(changed());
                }
                attempted.push(entry);
                remove(&f).map_err(io)?;
                if f.get_xattr(ATTRIBUTE).map_err(io)?.is_some() {
                    return Err(changed());
                }
            }
            let after = snapshot(root)?;
            let cleared: Vec<_> = expected
                .attributes
                .iter()
                .cloned()
                .map(|mut a| {
                    a.before = None;
                    a
                })
                .collect();
            if after.content != expected.content || after.attributes != cleared {
                return Err(changed());
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            if !rollback(root, &attempted) {
                return Err(HubError::new(
                    "QUARANTINE_ROLLBACK_INCOMPLETE",
                    "隔离属性可能仅部分恢复。请保留属性备份，停止重试并核对安装。",
                ));
            }
            return Err(error);
        }
        Ok(attempted.len() as u32)
    }
    impl Hub {
        async fn check_quarantine_scope(&self, scope: &Path) -> Result<()> {
            let data = self.dashboard().await?;
            let mut protected = vec![self.data_dir.clone()];
            if let Some(workspace) = data.workspace {
                protected.extend(
                    [
                        workspace.save_dir,
                        workspace.backup_dir,
                        workspace.content_dir,
                    ]
                    .map(std::path::PathBuf::from),
                );
            }
            protected.extend(
                data.play_instances
                    .into_iter()
                    .map(|i| std::path::PathBuf::from(i.data_root)),
            );
            for path in protected {
                if crate::play_instances::overlaps(scope, &path)? {
                    return Err(HubError::new(
                        "QUARANTINE_SCOPE_OVERLAP",
                        "安装须与存档、备份、资源、实例及 Hub 元数据目录分离。",
                    ));
                }
            }
            Ok(())
        }
        pub async fn prepare_installation_quarantine(
            &self,
            installation_id: &str,
        ) -> Result<InstallationQuarantinePlan> {
            let _gate = self.mutation.lock().await;
            self.check_resource_claims().await?;
            let installation = self.installation(installation_id).await?;
            let recognition = self
                .inspect_installation_recognition(installation_id.into())
                .await?;
            if recognition.recommended_profile.is_none() {
                return Err(HubError::new(
                    "QUARANTINE_BUILD_UNSUPPORTED",
                    "仅已识别的 macOS 游戏样本支持此操作；请先核对完整下载来源。",
                ));
            }
            let scope = paths::bundle_root(Path::new(&installation.executable)).ok_or(
                HubError::new("APPLICATION_REQUIRED", "此操作仅处理已登记的 .app 安装。"),
            )?;
            let key = id();
            self.check_quarantine_scope(&scope).await?;
            let backup_directory = paths::text(&self.data_dir.join(AUDIT).join(&key))?;
            let prepared = tokio::task::spawn_blocking(move || -> Result<Prepared> {
                if std::fs::canonicalize(&scope).map_err(io)? != scope {
                    return Err(changed());
                }
                let root =
                    Dir::open_ambient_dir(&scope, cap_std::ambient_authority()).map_err(io)?;
                let snapshot = snapshot(&root)?;
                let relative = paths::text(
                    Path::new(&installation.executable)
                        .strip_prefix(&scope)
                        .map_err(|_| changed())?,
                )?;
                if format::hash_reader(
                    &mut format::open_file(&root, &relative)?,
                    128 * 1024 * 1024,
                    &AtomicBool::new(false),
                )?
                .1 != installation.sha256
                {
                    return Err(changed());
                }
                crate::application_entry::validate_bundle_program(&scope.join(relative))?;
                let count = snapshot
                    .attributes
                    .iter()
                    .filter(|a| a.before.is_some())
                    .count();
                if count == 0 {
                    return Err(HubError::new(
                        "QUARANTINE_NOT_PRESENT",
                        "此安装没有待移除的隔离属性；请检查其他启动原因。",
                    ));
                }
                Ok(Prepared {
                    plan: InstallationQuarantinePlan {
                        id: key,
                        installation_id: installation.id,
                        scope: paths::text(&scope)?,
                        executable_sha256: installation.sha256,
                        quarantine_entries: count as u32,
                        backup_directory,
                        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(5))
                            .to_rfc3339(),
                    },
                    root,
                    snapshot,
                })
            })
            .await
            .map_err(|_| changed())??;
            let mut plans = self.quarantine_plans.lock().await;
            plans.retain(|_, p| p.plan.expires_at > now());
            if plans.len() >= 4 {
                return Err(HubError::new(
                    "PLAN_LIMIT",
                    "待确认的安装范围过多，请稍后重试。",
                ));
            }
            let plan = prepared.plan.clone();
            plans.insert(plan.id.clone(), prepared);
            Ok(plan)
        }
        pub async fn apply_installation_quarantine(
            &self,
            plan_id: &str,
            confirmed: bool,
        ) -> Result<InstallationQuarantineResult> {
            let _gate = self.mutation.lock().await;
            if !confirmed {
                return Err(HubError::new(
                    "CONFIRMATION_REQUIRED",
                    "请单独确认仅移除此安装的隔离属性。",
                ));
            }
            self.check_resource_claims().await?;
            let p = self
                .quarantine_plans
                .lock()
                .await
                .remove(plan_id)
                .ok_or_else(changed)?;
            if p.plan.expires_at <= now() {
                return Err(changed());
            }
            let installation = self.installation(&p.plan.installation_id).await?;
            if installation.sha256 != p.plan.executable_sha256
                || paths::bundle_root(Path::new(&installation.executable)).as_ref()
                    != Some(Path::new(&p.plan.scope).to_path_buf()).as_ref()
            {
                return Err(changed());
            }
            let metadata = self.data_dir.clone();
            self.check_quarantine_scope(Path::new(&p.plan.scope))
                .await?;
            let key = p.plan.id.clone();
            self.quarantine_log(&key, "running", None).await?;
            let outcome = tokio::task::spawn_blocking(move || -> Result<InstallationQuarantineResult> {
                revalidate(&p)?;
                let base = Dir::open_ambient_dir(metadata, cap_std::ambient_authority()).map_err(io)?;
                let mut private = DirBuilder::new();
                private.mode(0o700);
                match base.create_dir_with(AUDIT, &private) {
                    Ok(()) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(e) => return Err(io(e)),
                }
                let journal = base.open_dir_nofollow(AUDIT).map_err(io)?;
                journal.create_dir_with(&p.plan.id, &private).map_err(io)?;
                // Persist the directory links as well as the preimage itself
                // before the first security attribute is touched.
                open(&base, "")?.sync_all().map_err(io)?;
                open(&journal, "")?.sync_all().map_err(io)?;
                let journal = journal.open_dir_nofollow(&p.plan.id).map_err(io)?;
                write_json(&journal, "before.json", &serde_json::json!({
                    "plan": p.plan,
                    "content_sha256": p.snapshot.content,
                    "attributes": p.snapshot.attributes,
                }))?;
                let outcome = apply(&p.root, &p.snapshot, |f| {
                    root_path_matches(&p.root, Path::new(&p.plan.scope))
                        .map_err(std::io::Error::other)?;
                    f.remove_xattr(ATTRIBUTE)
                });
                let status = match &outcome {
                    Ok(_) => "succeeded",
                    Err(e) if e.code == "QUARANTINE_ROLLBACK_INCOMPLETE" => "partial_unknown",
                    Err(_) => "failed_rollback_attempted",
                };
                if write_json(&journal, "result.json", &serde_json::json!({
                    "status": status, "finished_at": now(),
                    "error": outcome.as_ref().err().map(|e| e.code),
                    "changed_entries": outcome.as_ref().ok(),
                })).is_err() {
                    return Err(HubError::new("QUARANTINE_AUDIT_FAILED", "属性可能已变更，但结果记录失败；请核对预览中的属性备份目录，勿自动重试。"));
                }
                Ok(InstallationQuarantineResult {
                    backup_directory: p.plan.backup_directory,
                    changed_entries: outcome?,
                })
            }).await.unwrap_or_else(|_| Err(HubError::new("QUARANTINE_STATUS_UNKNOWN", "属性处理结果未知，请核对属性备份和操作记录。")));
            self.quarantine_log(
                &key,
                if outcome.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                },
                outcome.as_ref().err().map(|e| e.code),
            )
            .await?;
            outcome
        }
        async fn quarantine_log(&self, key: &str, status: &str, error: Option<&str>) -> Result<()> {
            sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
                .bind(id())
                .bind(key)
                .bind("installation.quarantine")
                .bind(status)
                .bind(error)
                .bind(now())
                .execute(&self.pool)
                .await
                .map_err(|_| {
                    HubError::new(
                        "QUARANTINE_AUDIT_FAILED",
                        "无法保存安全属性操作记录；请检查属性备份目录。",
                    )
                })?;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn fixture() -> (tempfile::TempDir, Dir) {
            let t = tempfile::tempdir().unwrap();
            std::fs::write(t.path().join("game"), b"test data, never executed").unwrap();
            let d = Dir::open_ambient_dir(t.path(), cap_std::ambient_authority()).unwrap();
            for p in ["", "game"] {
                open(&d, p)
                    .unwrap()
                    .set_xattr(ATTRIBUTE, b"0081;fixture")
                    .unwrap();
            }
            (t, d)
        }
        #[test]
        fn changes_only_selected_attribute_and_preserves_file_bytes() {
            let (_t, d) = fixture();
            open(&d, "game")
                .unwrap()
                .set_xattr("org.cataclysmhub.test", b"keep")
                .unwrap();
            let before = snapshot(&d).unwrap();
            assert_eq!(
                apply(&d, &before, |f| f.remove_xattr(ATTRIBUTE)).unwrap(),
                2
            );
            assert_eq!(snapshot(&d).unwrap().content, before.content);
            assert_eq!(
                open(&d, "game")
                    .unwrap()
                    .get_xattr("org.cataclysmhub.test")
                    .unwrap(),
                Some(b"keep".to_vec())
            );
        }
        #[test]
        fn failed_removal_restores_attributes_already_changed() {
            let (_t, d) = fixture();
            let before = snapshot(&d).unwrap();
            let count = std::cell::Cell::new(0);
            assert!(
                apply(&d, &before, |f| {
                    count.set(count.get() + 1);
                    if count.get() == 2 {
                        Err(std::io::Error::other("injected"))
                    } else {
                        f.remove_xattr(ATTRIBUTE)
                    }
                })
                .is_err()
            );
            assert_eq!(snapshot(&d).unwrap().attributes, before.attributes);
        }
        #[test]
        fn rejects_outside_links_and_shared_regular_files() {
            let (t, d) = fixture();
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("secret"), b"outside").unwrap();
            std::os::unix::fs::symlink(outside.path().join("secret"), t.path().join("escape"))
                .unwrap();
            assert!(snapshot(&d).is_err());
            std::fs::remove_file(t.path().join("escape")).unwrap();
            std::fs::hard_link(t.path().join("game"), outside.path().join("shared")).unwrap();
            assert!(snapshot(&d).is_err());
        }
        fn prepared(t: &tempfile::TempDir, d: &Dir) -> Prepared {
            Prepared {
                plan: InstallationQuarantinePlan {
                    id: "preview".into(),
                    installation_id: "fixture".into(),
                    scope: std::fs::canonicalize(t.path())
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .into(),
                    executable_sha256: "hash".into(),
                    quarantine_entries: 2,
                    backup_directory: "/not-created".into(),
                    expires_at: "2000-01-01T00:00:00Z".into(),
                },
                root: d.try_clone().unwrap(),
                snapshot: snapshot(d).unwrap(),
            }
        }
        #[test]
        fn stale_content_attributes_and_root_identity_are_rejected() {
            for change in ["content", "attribute", "root"] {
                let (t, d) = fixture();
                let p = prepared(&t, &d);
                revalidate(&p).unwrap();
                match change {
                    "content" => {
                        std::fs::write(t.path().join("game"), b"different content").unwrap()
                    }
                    "attribute" => open(&d, "game")
                        .unwrap()
                        .set_xattr(ATTRIBUTE, b"different")
                        .unwrap(),
                    _ => {
                        let moved = t.path().with_extension("moved");
                        std::fs::rename(t.path(), &moved).unwrap();
                        std::fs::create_dir(t.path()).unwrap();
                        assert!(revalidate(&p).is_err());
                        std::fs::remove_dir(t.path()).unwrap();
                        std::fs::rename(moved, t.path()).unwrap();
                        continue;
                    }
                }
                assert_eq!(revalidate(&p).unwrap_err().code, "QUARANTINE_PLAN_STALE");
            }
        }
        #[tokio::test]
        async fn confirmation_and_expiry_do_not_create_attribute_backups_or_mutate() {
            let (t, d) = fixture();
            let metadata = tempfile::tempdir().unwrap();
            let hub = Hub::open(metadata.path()).await.unwrap();
            let p = prepared(&t, &d);
            let before = p.snapshot.attributes.clone();
            hub.quarantine_plans
                .lock()
                .await
                .insert("preview".into(), p);
            assert_eq!(
                hub.apply_installation_quarantine("preview", false)
                    .await
                    .unwrap_err()
                    .code,
                "CONFIRMATION_REQUIRED"
            );
            assert_eq!(
                hub.apply_installation_quarantine("preview", true)
                    .await
                    .unwrap_err()
                    .code,
                "QUARANTINE_PLAN_STALE"
            );
            assert_eq!(snapshot(&d).unwrap().attributes, before);
            assert!(!metadata.path().join(AUDIT).exists());
        }
        #[tokio::test]
        async fn unsupported_builds_and_active_or_unknown_games_are_blocked() {
            let (t, d) = fixture();
            let metadata = tempfile::tempdir().unwrap();
            let hub = Hub::open(metadata.path()).await.unwrap();
            let hash = format::sha(b"test data, never executed");
            sqlx::query("INSERT INTO installation VALUES ('fixture','fixture',?,?,?,NULL,NULL,'macos','aarch64',?)")
                .bind(t.path().join("game").to_str().unwrap()).bind(t.path().to_str().unwrap()).bind(hash).bind(now()).execute(&hub.pool).await.unwrap();
            assert_eq!(
                hub.prepare_installation_quarantine("fixture")
                    .await
                    .unwrap_err()
                    .code,
                "QUARANTINE_BUILD_UNSUPPORTED"
            );
            for state in ["running", "interrupted_unknown"] {
                sqlx::query("INSERT INTO session(id,installation_id,state,started_at) VALUES (?,'fixture',?,?)").bind(state).bind(state).bind(now()).execute(&hub.pool).await.unwrap();
                assert_eq!(
                    hub.prepare_installation_quarantine("fixture")
                        .await
                        .unwrap_err()
                        .code,
                    "GAME_RUNNING_OR_UNKNOWN"
                );
                assert_eq!(
                    hub.apply_installation_quarantine("missing", true)
                        .await
                        .unwrap_err()
                        .code,
                    "GAME_RUNNING_OR_UNKNOWN"
                );
            }
            assert!(!metadata.path().join(AUDIT).exists());
            assert!(attribute(&d, "game").unwrap().before.is_some());
        }
        #[tokio::test]
        async fn metadata_and_attribute_journal_are_never_in_the_installation_scope() {
            let metadata = tempfile::tempdir().unwrap();
            let hub = Hub::open(metadata.path()).await.unwrap();
            let root = std::fs::canonicalize(metadata.path()).unwrap();
            assert_eq!(
                hub.check_quarantine_scope(&root).await.unwrap_err().code,
                "QUARANTINE_SCOPE_OVERLAP"
            );
            assert_eq!(
                hub.check_quarantine_scope(root.parent().unwrap())
                    .await
                    .unwrap_err()
                    .code,
                "QUARANTINE_SCOPE_OVERLAP"
            );
            assert!(!root.join(AUDIT).exists());
        }
    }
}
