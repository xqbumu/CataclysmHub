use crate::{Hub, HubError, LaunchPlan, PlayInstance, Result, paths, process::PreparedLaunch};
use std::path::{Path, PathBuf};

const PROFILE: &str = "cdda-0h-source-trial-v1";

fn profile_args(root: &str) -> Vec<String> {
    let child = |name: &str| format!("{}/{name}/", root.trim_end_matches(['/', '\\']));
    vec![
        "--userdir".into(),
        format!("{}/", root.trim_end_matches(['/', '\\'])),
        "--savedir".into(),
        child("save"),
        "--configdir".into(),
        child("config"),
    ]
}

// A read-only preflight, not a sandbox or proof about what a program will write.
fn check_tree(root: &Path) -> Result<()> {
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut count = 0;
    while let Some((dir, depth)) = pending.pop() {
        for entry in std::fs::read_dir(dir)
            .map_err(|_| HubError::new("INSTANCE_SCOPE_UNREADABLE", "无法完整检查实例目录。"))?
        {
            count += 1;
            if count > 20_000 {
                return Err(HubError::new(
                    "INSTANCE_SCOPE_LIMIT",
                    "实例预检超过 20000 项，尚未支持此范围。",
                ));
            }
            let entry = entry.map_err(|_| {
                HubError::new("INSTANCE_SCOPE_UNREADABLE", "无法完整检查实例目录。")
            })?;
            let kind = entry.file_type().map_err(|_| {
                HubError::new("INSTANCE_SCOPE_UNREADABLE", "无法读取实例条目类型。")
            })?;
            if kind.is_symlink() || !(kind.is_file() || kind.is_dir()) {
                return Err(HubError::new(
                    "INSTANCE_SCOPE_UNSAFE",
                    "实例目录含链接或特殊文件，不能使用此试运行方案。",
                ));
            }
            if kind.is_dir() {
                if depth >= 32 {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_LIMIT",
                        "实例目录层级超过预检限制。",
                    ));
                }
                pending.push((entry.path(), depth + 1));
            }
        }
    }
    Ok(())
}
impl Hub {
    async fn instance(&self, instance_id: &str) -> Result<PlayInstance> {
        sqlx::query_as("SELECT * FROM play_instance WHERE id=?")
            .bind(instance_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(HubError::db)?
            .ok_or(HubError::new(
                "INSTANCE_NOT_FOUND",
                "实例不存在，请刷新游戏库。",
            ))
    }
    async fn instance_identity(
        &self,
        instance: &PlayInstance,
        working_dir: &str,
    ) -> Result<(same_file::Handle, same_file::Handle)> {
        let dashboard = self.dashboard().await?;
        let workspace = dashboard
            .workspace
            .ok_or(HubError::new("WORKSPACE_REQUIRED", "请先配置工作区。"))?;
        let installs = dashboard.installations;
        let mut other = vec![
            self.data_dir.clone(),
            PathBuf::from(workspace.save_dir),
            PathBuf::from(workspace.backup_dir),
            PathBuf::from(workspace.content_dir),
        ];
        other.extend(
            installs
                .into_iter()
                .map(|item| PathBuf::from(item.working_dir)),
        );
        other.extend(
            self.play_instances()
                .await?
                .into_iter()
                .filter(|item| item.id != instance.id)
                .map(|item| PathBuf::from(item.data_root)),
        );
        let root = instance.data_root.clone();
        let working_dir = working_dir.to_owned();
        tokio::task::spawn_blocking(move || {
            let canonical = paths::canonical(&root, true)?;
            if paths::text(&canonical)? != root {
                return Err(HubError::new(
                    "INSTANCE_PATH_CHANGED",
                    "实例目录路径已变化，请重新检查。",
                ));
            }
            for scope in other {
                let scope = std::fs::canonicalize(scope).map_err(|_| {
                    HubError::new("PATH_INVALID", "受保护范围不可访问，不能完成实例检查。")
                })?;
                if crate::play_instances::overlaps(&canonical, &scope)? {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_OVERLAP",
                        "实例与其他受保护范围重叠。",
                    ));
                }
            }
            check_tree(&canonical)?;
            let root_handle = same_file::Handle::from_path(&canonical)
                .map_err(|_| HubError::new("PATH_INVALID", "无法打开实例目录身份。"))?;
            let working = paths::canonical(&working_dir, true)?;
            if paths::text(&working)? != working_dir {
                return Err(HubError::new(
                    "INSTANCE_PATH_CHANGED",
                    "游戏工作目录已变化。",
                ));
            }
            let working_handle = same_file::Handle::from_path(working)
                .map_err(|_| HubError::new("PATH_INVALID", "无法打开游戏工作目录身份。"))?;
            Ok((root_handle, working_handle))
        })
        .await
        .map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "实例检查未完成。"))?
    }
    /// Explicitly selected source-derived trial. Never grants verified binding or concurrency.
    pub async fn instance_launch_plan(
        &self,
        instance_id: &str,
        profile_id: &str,
    ) -> Result<LaunchPlan> {
        let _guard = self.mutation.lock().await;
        if profile_id != PROFILE {
            return Err(HubError::new(
                "PROFILE_UNSUPPORTED",
                "不支持此实例参数方案。",
            ));
        }
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        if installation.sha256 != instance.installation_sha256 {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "实例关联的程序身份已变化。",
            ));
        }
        let identity = self
            .instance_identity(&instance, &installation.working_dir)
            .await?;
        let mut plan = self
            .launch_plan_inner(&instance.installation_id, profile_args(&instance.data_root))
            .await?;
        plan.play_instance_id = Some(instance.id);
        plan.launch_profile_id = Some(PROFILE.into());
        plan.instance_data_root = Some(instance.data_root.clone());
        plan.save_dir = paths::text(&PathBuf::from(&instance.data_root).join("save"))?;
        plan.warnings = vec![
            "这是显式选择的 CDDA 0.H 源码参数试运行；没有识别或验证本地程序版本，不适用于任意游戏。".into(),
            "已指定用户、存档和配置目录参数，但实际绑定仍未验证；不会复制或迁移原存档。".into(),
            "0.H 还可能写入安装基础目录和数据目录的缓存；本方案保持全局互斥，不允许并行游玩。".into(),
            "只监测直接进程；退出不代表子进程已关闭或存档稳定。实例数据尚未接入工作区备份。".into(),
        ];
        self.plans.lock().await.insert(
            plan.id.clone(),
            PreparedLaunch {
                plan: plan.clone(),
                instance_identity: Some(identity),
            },
        );
        Ok(plan)
    }
    pub(crate) async fn revalidate_instance_launch(
        &self,
        plan: &LaunchPlan,
        identity: &(same_file::Handle, same_file::Handle),
    ) -> Result<()> {
        let instance = self
            .instance(
                plan.play_instance_id
                    .as_deref()
                    .ok_or(HubError::new("PLAN_STALE", "实例预览缺少关联。"))?,
            )
            .await?;
        if plan.launch_profile_id.as_deref() != Some(PROFILE)
            || plan.instance_data_root.as_deref() != Some(&instance.data_root)
            || plan.installation_id != instance.installation_id
            || plan.executable_sha256 != instance.installation_sha256
            || plan.args != profile_args(&instance.data_root)
        {
            return Err(HubError::new("PLAN_STALE", "实例预览已变化，请重新生成。"));
        }
        let current = self.instance_identity(&instance, &plan.working_dir).await?;
        if current.0 != identity.0 || current.1 != identity.1 {
            return Err(HubError::new(
                "INSTANCE_PATH_CHANGED",
                "预览后实例或安装目录被替换，启动已取消。",
            ));
        }
        Ok(())
    }
}
