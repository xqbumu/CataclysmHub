use crate::{Hub, HubError, LaunchPlan, PlayInstance, Result, paths, process::PreparedLaunch};
use cap_fs_ext::DirExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PROFILE: &str = "cdda-0h-source-trial-v1";
const MAC_H: &str = "cdda-0h-release-macos-trial-v1";
const MAC_I: &str = "cdda-0i-1-macos-trial-v1";

fn validate_profile(profile: &str) -> Result<()> {
    if ![PROFILE, MAC_H, MAC_I].contains(&profile) {
        return Err(HubError::new(
            "PROFILE_UNSUPPORTED",
            "不支持此实例参数方案。",
        ));
    }
    if profile != PROFILE && !cfg!(target_os = "macos") {
        return Err(HubError::new(
            "PROFILE_PLATFORM_UNSUPPORTED",
            "此发布包试运行方案仅适用于 macOS。",
        ));
    }
    Ok(())
}

async fn profile_environment(
    profile: &str,
    executable: &str,
    working_dir: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    validate_profile(profile)?;
    if profile == PROFILE {
        return Ok(Default::default());
    }
    let executable = executable.to_owned();
    let working = working_dir.to_owned();
    tokio::task::spawn_blocking(move || {
        let root = paths::canonical(&working, true)?;
        let path = Path::new(&executable);
        if path.file_name().and_then(|name| name.to_str()) != Some("cataclysm-tiles")
            || path.parent() != Some(root.as_path())
            || root.file_name().and_then(|name| name.to_str()) != Some("Resources")
            || root.parent().and_then(Path::file_name).and_then(|name| name.to_str()) != Some("Contents") {
            return Err(HubError::new("BUNDLE_LAYOUT_UNSUPPORTED", "请选择 macOS 包内 Contents/Resources/cataclysm-tiles 本体；不使用 bundle 启动脚本。"));
        }
        for name in ["SDL2", "SDL2_image", "SDL2_ttf", "SDL2_mixer"] {
            let file = root.join(format!("{name}.framework/Versions/A/{name}"));
            let canonical = std::fs::canonicalize(file).map_err(|_| HubError::new("BUNDLE_LAYOUT_UNSUPPORTED", "发布包内所需框架缺失或不可访问。"))?;
            if !canonical.starts_with(&root) || !canonical.is_file() {
                return Err(HubError::new("BUNDLE_LAYOUT_UNSUPPORTED", "发布包框架不能引用安装范围以外的路径。"));
            }
        }
        let text = paths::text(&root)?;
        Ok(std::collections::BTreeMap::from([("DYLD_FRAMEWORK_PATH".into(), text.clone()), ("DYLD_LIBRARY_PATH".into(), text)]))
    }).await.map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "发布包布局检查未完成。"))?
}

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
    for name in ["save", "config"] {
        match std::fs::symlink_metadata(root.join(name)) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err(HubError::new(
                    "INSTANCE_ROLE_INVALID",
                    "实例 save/config 路径须为普通目录，不能是文件或链接。",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(HubError::new(
                    "INSTANCE_SCOPE_UNREADABLE",
                    "无法检查实例 save/config 目录。",
                ));
            }
        }
    }
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
        validate_profile(profile_id)?;
        let instance = self.instance(instance_id).await?;
        let installation = self.installation(&instance.installation_id).await?;
        if installation.sha256 != instance.installation_sha256 {
            return Err(HubError::new(
                "EXECUTABLE_CHANGED",
                "实例关联的程序身份已变化。",
            ));
        }
        self.check_resource_claims().await?;
        let environment = profile_environment(
            profile_id,
            &installation.executable,
            &installation.working_dir,
        )
        .await?;
        let identity = self
            .instance_identity(&instance, &installation.working_dir)
            .await?;
        let mut plan = self
            .launch_plan_inner(&instance.installation_id, profile_args(&instance.data_root))
            .await?;
        plan.play_instance_id = Some(instance.id);
        plan.launch_profile_id = Some(profile_id.into());
        plan.environment = environment;
        plan.instance_data_root = Some(instance.data_root.clone());
        plan.save_dir = paths::text(&PathBuf::from(&instance.data_root).join("save"))?;
        plan.warnings = vec![
            format!(
                "这是显式选择的 {profile_id} 源码参数试运行；没有识别或验证本地程序版本，不适用于任意游戏。"
            ),
            "确认后将准备实例 save/config 子目录并传递目录参数；实际绑定仍未验证，不会复制或迁移原存档。".into(),
            "游戏还可能写入安装基础目录和数据目录的缓存；本方案保持全局互斥，不允许并行游玩。"
                .into(),
            "只监测直接进程；退出不代表子进程已关闭或存档稳定。实例数据尚未接入工作区备份。".into(),
        ];
        self.plans.lock().await.insert(
            plan.id.clone(),
            PreparedLaunch {
                plan: plan.clone(),
                instance_identity: Some(Arc::new(identity)),
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
        let profile = plan
            .launch_profile_id
            .as_deref()
            .ok_or(HubError::new("PLAN_STALE", "预览缺少参数方案。"))?;
        let environment = profile_environment(profile, &plan.executable, &plan.working_dir).await?;
        if plan.environment != environment
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

pub(crate) async fn prepare_instance_directories(
    plan: &LaunchPlan,
    identity: Arc<(same_file::Handle, same_file::Handle)>,
) -> Result<()> {
    let path = plan
        .instance_data_root
        .clone()
        .ok_or(HubError::new("PLAN_STALE", "实例目录缺失。"))?;
    tokio::task::spawn_blocking(move || {
        let error = |_| {
            HubError::new(
                "INSTANCE_PREPARE_FAILED",
                "无法准备实例 save/config 目录，程序未启动。已有文件未被覆盖。",
            )
        };
        let root = cap_std::fs::Dir::open_ambient_dir(&path, cap_std::ambient_authority())
            .map_err(error)?;
        let current =
            same_file::Handle::from_file(root.try_clone().map_err(error)?.into_std_file())
                .map_err(error)?;
        if current != identity.0 {
            return Err(HubError::new(
                "INSTANCE_PATH_CHANGED",
                "实例目录已被替换，程序未启动。",
            ));
        }
        for name in ["save", "config"] {
            match root.create_dir(name) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(cause) => return Err(error(cause)),
            }
            root.open_dir_nofollow(name).map_err(error)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| {
        HubError::new(
            "INSTANCE_PREPARE_FAILED",
            "实例目录准备未完成，程序未启动。",
        )
    })?
}
