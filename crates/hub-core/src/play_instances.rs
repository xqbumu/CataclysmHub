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
    pub async fn register_play_instance(&self, input: PlayInstanceInput) -> Result<PlayInstance> {
        let _guard = self.mutation.lock().await;
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
        let workspace = self
            .dashboard()
            .await?
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let existing = self.play_instances().await?;
        let installations: Vec<crate::Installation> = sqlx::query_as("SELECT * FROM installation")
            .fetch_all(&self.pool)
            .await
            .map_err(HubError::db)?;
        let mut protected = vec![
            self.data_dir.clone(),
            PathBuf::from(workspace.save_dir),
            PathBuf::from(workspace.backup_dir),
            PathBuf::from(workspace.content_dir),
        ];
        protected.extend(
            installations
                .into_iter()
                .map(|item| PathBuf::from(item.working_dir)),
        );
        let root_input = input.data_root.clone();
        let prior = existing.clone();
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
            if std::fs::read_dir(&root)
                .map_err(|_| HubError::new("PATH_INVALID", "无法检查实例目录。"))?
                .next()
                .is_some()
            {
                return Err(HubError::new(
                    "INSTANCE_ROOT_NOT_EMPTY",
                    "请选择现有的空目录；不会迁移或覆盖文件。",
                ));
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
