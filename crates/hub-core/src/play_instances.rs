use crate::{
    Hub, HubError, Result,
    database::{id, now},
    paths,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, sqlx::FromRow, TS)]
pub struct PlayInstance {
    pub id: String,
    pub installation_id: String,
    pub installation_sha256: String,
    pub name: String,
    pub data_root: String,
    pub binding_status: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PlayInstanceInput {
    pub installation_id: String,
    pub name: String,
    pub data_root: String,
}

// Compare physical directory identities as well as normalized names, including
// case-insensitive filesystems and symlink aliases. Failure is conservative.
pub(crate) fn overlaps(a: &Path, b: &Path) -> Result<bool> {
    let identity = |path: &Path| {
        same_file::Handle::from_path(path)
            .map_err(|_| HubError::new("PATH_INVALID", "隔离范围不可访问，请检查目录。"))
    };
    let a_handle = identity(a)?;
    let b_handle = identity(b)?;
    for ancestor in b.ancestors() {
        if a_handle == identity(ancestor)? {
            return Ok(true);
        }
    }
    for ancestor in a.ancestors() {
        if b_handle == identity(ancestor)? {
            return Ok(true);
        }
    }
    Ok(false)
}
impl Hub {
    pub async fn play_instances(&self) -> Result<Vec<PlayInstance>> {
        sqlx::query_as("SELECT * FROM play_instance ORDER BY created_at DESC, id")
            .fetch_all(&self.pool)
            .await
            .map_err(HubError::db)
    }
    /// Only the trusted desktop layer supplies this platform-derived managed root.
    pub async fn register_managed_instance(
        &self,
        managed_root: &Path,
        installation_id: String,
        name: String,
    ) -> Result<PlayInstance> {
        use cap_fs_ext::DirExt;
        use cap_std::fs::Dir;
        let _guard = self.mutation.lock().await;
        self.check_resource_claims().await?;
        if name.chars().any(char::is_control)
            || name.trim().is_empty()
            || name.trim().chars().count() > 100
        {
            return Err(HubError::new(
                "NAME_INVALID",
                "实例名称须为 1～100 个可显示字符。",
            ));
        }
        let installation = self.installation(&installation_id).await?;
        if paths::digest(PathBuf::from(&installation.executable)).await? != installation.sha256 {
            return Err(HubError::new(
                "SOURCE_CHANGED",
                "游戏程序已变化，请重新检查安装。",
            ));
        }
        let data = self.dashboard().await?;
        let workspace = data
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let mut protected = vec![
            self.data_dir.clone(),
            PathBuf::from(workspace.save_dir),
            PathBuf::from(workspace.backup_dir),
            PathBuf::from(workspace.content_dir),
        ];
        protected.extend(
            data.installations
                .iter()
                .map(|i| paths::installation_scope(&i.working_dir)),
        );
        protected.extend(
            data.play_instances
                .iter()
                .map(|i| PathBuf::from(&i.data_root)),
        );
        let parent = managed_root
            .parent()
            .ok_or(HubError::new("PATH_INVALID", "系统目录没有父目录。"))?;
        let leaf = managed_root
            .file_name()
            .ok_or(HubError::new("PATH_INVALID", "系统目录没有名称。"))?;
        let target = managed_root.join("instances").join(id());
        // Before any mkdir, reject if the nearest existing ancestor is inside a
        // registered source. Comparing directory identities also catches aliases.
        let mut ancestor = target.as_path();
        while !ancestor
            .try_exists()
            .map_err(|_| HubError::new("PATH_INVALID", "系统目录不可访问。"))?
        {
            ancestor = ancestor
                .parent()
                .ok_or(HubError::new("PATH_INVALID", "系统目录不可访问。"))?;
        }
        for protected in protected {
            for existing in ancestor.ancestors() {
                if same_file::is_same_file(existing, &protected)
                    .map_err(|_| HubError::new("PATH_INVALID", "受保护目录不可访问。"))?
                {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_OVERLAP",
                        "推荐实例目录位于已有受保护数据内，请选择自定义目录。",
                    ));
                }
            }
        }
        let err = |_| HubError::new("DEFAULT_DIRECTORY_FAILED", "无法创建系统推荐实例目录。");
        let parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority()).map_err(err)?;
        match parent.create_dir(leaf) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(err(e)),
        }
        let root = parent.open_dir_nofollow(leaf).map_err(err)?;
        match root.create_dir("instances") {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(err(e)),
        }
        let instances = root.open_dir_nofollow("instances").map_err(err)?;
        instances
            .create_dir(target.file_name().unwrap())
            .map_err(err)?;
        self.register_play_instance_locked(PlayInstanceInput {
            installation_id,
            name,
            data_root: paths::text(&target)?,
        })
        .await
    }
    /// Derive scope and game from a completed durable restore, never arbitrary UI paths.
    pub async fn register_restored_instance(
        &self,
        restore_task_id: &str,
        name: &str,
    ) -> Result<PlayInstance> {
        let _guard = self.mutation.lock().await;
        self.check_resource_claims().await?;
        let task = self.restore_task(restore_task_id).await?;
        if task.state != "succeeded" {
            return Err(HubError::new(
                "RESTORE_NOT_READY",
                "仅完成的恢复可创建独立环境。",
            ));
        }
        let backup: crate::BackupRecord = sqlx::query_as("SELECT * FROM backup_archive WHERE id=?")
            .bind(&task.backup_id)
            .fetch_one(&self.pool)
            .await
            .map_err(HubError::db)?;
        let original = self
            .instance(backup.play_instance_id.as_deref().ok_or(HubError::new(
                "RESTORED_INSTALLATION_MISMATCH",
                "此副本没有原实例关联，请手动核对游戏。",
            ))?)
            .await?;
        let data_root = paths::text(&Path::new(&task.destination).join("data"))?;
        if let Some(existing) = self
            .play_instances()
            .await?
            .into_iter()
            .find(|i| i.data_root == data_root)
        {
            if existing.installation_id == original.installation_id
                && existing.installation_sha256 == original.installation_sha256
            {
                return Ok(existing);
            }
            return Err(HubError::new(
                "INSTANCE_ROOT_IN_USE",
                "恢复目录已关联其他游戏。",
            ));
        }
        if backup.source_root.as_deref() != Some(original.data_root.as_str()) {
            return Err(HubError::new(
                "RESTORED_INSTALLATION_MISMATCH",
                "副本来源目录与原实例不一致。",
            ));
        }
        self.register_play_instance_locked(PlayInstanceInput {
            installation_id: original.installation_id,
            name: name.into(),
            data_root,
        })
        .await
    }
    pub async fn register_play_instance(&self, input: PlayInstanceInput) -> Result<PlayInstance> {
        let _guard = self.mutation.lock().await;
        self.register_play_instance_locked(input).await
    }
    async fn register_play_instance_locked(
        &self,
        input: PlayInstanceInput,
    ) -> Result<PlayInstance> {
        self.check_resource_claims().await?;
        if input.name.chars().any(char::is_control)
            || input.name.trim().is_empty()
            || input.name.trim().chars().count() > 100
        {
            return Err(HubError::new(
                "NAME_INVALID",
                "实例名称须为 1～100 个可显示字符。",
            ));
        }
        let installation = self.installation(&input.installation_id).await?;
        if paths::digest(PathBuf::from(&installation.executable)).await? != installation.sha256 {
            return Err(HubError::new(
                "SOURCE_CHANGED",
                "游戏程序已变化，请重新检查安装。",
            ));
        }
        let data = self.dashboard().await?;
        let workspace = data
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let existing = self.play_instances().await?;
        let installations: Vec<crate::Installation> = sqlx::query_as("SELECT * FROM installation")
            .fetch_all(&self.pool)
            .await
            .map_err(HubError::db)?;
        let backup_root = workspace.backup_dir.clone();
        let mut protected = vec![
            self.data_dir.clone(),
            PathBuf::from(workspace.save_dir),
            PathBuf::from(workspace.backup_dir),
            PathBuf::from(workspace.content_dir),
        ];
        protected.extend(
            installations
                .into_iter()
                .map(|item| paths::installation_scope(&item.working_dir)),
        );
        let root_input = input.data_root.clone();
        // Look up the durable task directly: dashboard history is intentionally
        // capped and must not decide whether an older restore is usable.
        let canonical_root = paths::canonical(&root_input, true)?;
        let restored_record = if canonical_root.file_name().is_some_and(|n| n == "data") {
            let destination = paths::text(
                canonical_root
                    .parent()
                    .ok_or(HubError::new("PATH_INVALID", "实例目录没有父目录。"))?,
            )?;
            let task: Option<crate::RestoreTask> = sqlx::query_as(&format!(
                "SELECT {} FROM restore_task WHERE destination=? AND state='succeeded' ORDER BY created_at DESC,id LIMIT 1", crate::managed_restore::COLUMNS,
            )).bind(destination).fetch_optional(&self.pool).await.map_err(HubError::db)?;
            if let Some(task) = task {
                let record: Option<crate::BackupRecord> =
                    sqlx::query_as("SELECT * FROM backup_archive WHERE id=?")
                        .bind(&task.backup_id)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(HubError::db)?;
                record.map(|record| (task, record))
            } else {
                None
            }
        } else {
            None
        };
        let prior = existing.clone();
        let install_identity = (installation.id.clone(), installation.sha256.clone());
        let root = tokio::task::spawn_blocking(move || {
            let root = paths::canonical(&root_input, true)?;
            for item in &prior {
                let old = paths::canonical(&item.data_root, true)?;
                if overlaps(&root, &old)? {
                    if same_file::is_same_file(&root, &old).unwrap_or(false) {
                        return Ok((root, Some(item.clone())));
                    }
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_OVERLAP",
                        "实例数据目录不能重叠。",
                    ));
                }
            }
            for item in protected {
                let item = std::fs::canonicalize(item)
                    .map_err(|_| HubError::new("PATH_INVALID", "现有受保护目录不可访问。"))?;
                if overlaps(&root, &item)? {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_OVERLAP",
                        "实例目录须与游戏安装、工作区及元数据目录分离。",
                    ));
                }
            }
            if restored_record.is_some()
                || std::fs::read_dir(&root)
                    .map_err(|_| HubError::new("PATH_INVALID", "无法检查实例目录。"))?
                    .next()
                    .is_some()
            {
                // Only a completed, registered instance restore can admit a
                // nonempty root. Re-read its bytes now, not a stale inspection.
                let restored = restored_record.as_ref().ok_or(HubError::new(
                    "INSTANCE_ROOT_NOT_EMPTY",
                    "请选择空目录或由 Hub 成功恢复且可重新核验的实例 data 目录。",
                ))?;
                if Path::new(&restored.0.destination).join("data") != root {
                    return Err(HubError::new(
                        "PATH_CHANGED",
                        "恢复目录路径已变化，请重新选择。",
                    ));
                }
                let original = prior
                    .iter()
                    .find(|item| Some(&item.id) == restored.1.play_instance_id.as_ref())
                    .ok_or(HubError::new(
                        "RESTORED_INSTALLATION_MISMATCH",
                        "恢复来源没有可核对的原实例。",
                    ))?;
                if original.installation_id != install_identity.0
                    || original.installation_sha256 != install_identity.1
                    || restored.1.source_root.as_deref() != Some(original.data_root.as_str())
                {
                    return Err(HubError::new(
                        "RESTORED_INSTALLATION_MISMATCH",
                        "恢复数据须关联原实例的同一游戏安装及程序摘要。",
                    ));
                }
                let checked = crate::managed_inspection::inspect_registered(
                    &restored.1,
                    &restored.0,
                    &backup_root,
                    &std::sync::atomic::AtomicBool::new(false),
                    &std::sync::atomic::AtomicU8::new(0),
                )?;
                if checked.incomplete_marker_present {
                    return Err(HubError::new(
                        "RESTORE_NOT_READY",
                        "恢复目录含未完成标记，请先检查，不能登记为实例。",
                    ));
                }
            }
            Ok((root, None))
        })
        .await
        .map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "实例目录检查未完成。"))??;
        if let Some(old) = root.1 {
            if old.installation_id == installation.id
                && old.name == input.name.trim()
                && old.installation_sha256 == installation.sha256
            {
                return Ok(old);
            }
            return Err(HubError::new(
                "INSTANCE_ROOT_IN_USE",
                "该目录已登记为其他实例。",
            ));
        }
        if existing.len() >= 200 {
            return Err(HubError::new("INSTANCE_LIMIT", "最多登记 200 个实例。"));
        }
        let instance = PlayInstance {
            id: id(),
            installation_id: installation.id,
            installation_sha256: installation.sha256,
            name: input.name.trim().to_owned(),
            data_root: paths::text(&root.0)?,
            binding_status: "unverified".into(),
            created_at: now(),
        };
        let mut tx = self.pool.begin().await.map_err(HubError::db)?;
        sqlx::query("INSERT INTO play_instance VALUES (?,?,?,?,?,?,?)")
            .bind(&instance.id)
            .bind(&instance.installation_id)
            .bind(&instance.installation_sha256)
            .bind(&instance.name)
            .bind(&instance.data_root)
            .bind(&instance.binding_status)
            .bind(&instance.created_at)
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        sqlx::query("INSERT INTO operation_log VALUES (?,?,?,?,?,?)")
            .bind(id())
            .bind(&instance.id)
            .bind("instance.register")
            .bind("succeeded")
            .bind(Option::<String>::None)
            .bind(now())
            .execute(&mut *tx)
            .await
            .map_err(HubError::db)?;
        tx.commit().await.map_err(HubError::db)?;
        Ok(instance)
    }
}
