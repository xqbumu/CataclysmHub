use crate::{Hub, HubError, LaunchPlan, PlayInstance, Result, paths, process::PreparedLaunch};
use cap_fs_ext::DirExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PROFILE: &str = "cdda-0h-source-trial-v1";
const MAC_H: &str = "cdda-0h-release-macos-trial-v1";
const MAC_I: &str = "cdda-0i-1-macos-trial-v1";
const MAC_H_V2: &str = "cdda-0h-release-macos-trial-v2";
const MAC_I_V2: &str = "cdda-0i-1-macos-trial-v2";
const MAC_H_NATIVE: &str = "cdda-0h-release-macos-native-trial-v1";
const MAC_I_NATIVE: &str = "cdda-0i-1-macos-native-trial-v1";
const RUNTIME_DIR: &str = ".hub-runtime";
const RUNTIME_ROLES: [&str; 6] = ["home", "tmp", "config", "cache", "data", "state"];

fn has_runtime(profile: &str) -> bool {
    let profile = crate::candidate_launch::base_profile(profile);
    matches!(profile, MAC_H_V2 | MAC_I_V2 | MAC_H_NATIVE | MAC_I_NATIVE)
}
fn native_entry(profile: &str) -> bool {
    let profile = crate::candidate_launch::base_profile(profile);
    matches!(profile, MAC_H_NATIVE | MAC_I_NATIVE)
}
fn layout_error() -> HubError {
    HubError::new(
        "BUNDLE_LAYOUT_UNSUPPORTED",
        "请选择所选方案对应的包内游戏本体；原生入口须位于 .app/Contents/MacOS，不使用启动脚本。",
    )
}
fn working_directory(profile: &str, executable: &str, original: &str) -> Result<String> {
    if !native_entry(profile) {
        return Ok(original.into());
    }
    let parent = Path::new(executable).parent().ok_or_else(layout_error)?;
    let contents = parent.parent().ok_or_else(layout_error)?;
    if parent.file_name().and_then(|n| n.to_str()) != Some("MacOS")
        || parent != Path::new(original)
        || contents.file_name().and_then(|n| n.to_str()) != Some("Contents")
        || !contents
            .parent()
            .and_then(Path::extension)
            .is_some_and(|e| e.eq_ignore_ascii_case("app"))
    {
        return Err(layout_error());
    }
    let resources = contents.join("Resources");
    let canonical = paths::canonical(&paths::text(&resources)?, true)?;
    if canonical != resources {
        return Err(layout_error());
    }
    paths::text(&canonical)
}

fn check_role(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
            Err(HubError::new(
                "INSTANCE_ROLE_INVALID",
                "实例运行环境路径须为普通目录，不能是文件或链接。",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(HubError::new(
            "INSTANCE_SCOPE_UNREADABLE",
            "无法检查实例运行环境目录。",
        )),
    }
}

pub(crate) fn validate_profile(profile: &str) -> Result<()> {
    crate::candidate_launch::ensure_enabled(profile)?;
    let profile = crate::candidate_launch::base_profile(profile);
    if ![
        PROFILE,
        MAC_H,
        MAC_I,
        MAC_H_V2,
        MAC_I_V2,
        MAC_H_NATIVE,
        MAC_I_NATIVE,
    ]
    .contains(&profile)
    {
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
    data_root: &str,
) -> Result<std::collections::BTreeMap<String, String>> {
    validate_profile(profile)?;
    if crate::candidate_launch::base_profile(profile) == PROFILE {
        return Ok(Default::default());
    }
    let mut environment = std::collections::BTreeMap::new();
    if has_runtime(profile) {
        let runtime = Path::new(data_root).join(RUNTIME_DIR);
        check_role(&runtime)?;
        for role in RUNTIME_ROLES {
            check_role(&runtime.join(role))?;
        }
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
            environment.insert(key.into(), paths::text(&runtime.join(role))?);
        }
    }
    if crate::candidate_launch::base_profile(profile) != profile {
        environment.insert(
            "CFFIXED_USER_HOME".into(),
            paths::text(&Path::new(data_root).join(RUNTIME_DIR).join("home"))?,
        );
        environment.insert("PATH".into(), "/usr/bin:/bin".into());
    }
    let native = native_entry(profile);
    let executable = executable.to_owned();
    let working = working_dir.to_owned();
    tokio::task::spawn_blocking(move || {
        let root = paths::canonical(&working, true)?;
        let path = Path::new(&executable);
        if root.file_name().and_then(|name| name.to_str()) != Some("Resources")
            || root
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                != Some("Contents")
        {
            return Err(layout_error());
        }
        if native {
            use std::io::Read;
            let parent = path.parent().ok_or_else(layout_error)?;
            if parent.file_name().and_then(|name| name.to_str()) != Some("MacOS")
                || parent.parent() != root.parent()
                || !root
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::extension)
                    .is_some_and(|e| e.eq_ignore_ascii_case("app"))
            {
                return Err(layout_error());
            }
            let mut magic = [0_u8; 4];
            std::fs::File::open(path)
                .and_then(|mut file| file.read_exact(&mut magic))
                .map_err(|_| layout_error())?;
            if !matches!(
                u32::from_be_bytes(magic),
                0xfeedface
                    | 0xcefaedfe
                    | 0xfeedfacf
                    | 0xcffaedfe
                    | 0xcafebabe
                    | 0xbebafeca
                    | 0xcafebabf
                    | 0xbfbafeca
            ) {
                return Err(HubError::new(
                    "BUNDLE_NATIVE_ENTRY_REQUIRED",
                    "所选入口不是 Mach-O 本体；不能使用脚本或其他包装程序冒充原生入口。",
                ));
            }
        } else if path.file_name().and_then(|name| name.to_str()) != Some("cataclysm-tiles")
            || path.parent() != Some(root.as_path())
        {
            return Err(layout_error());
        }
        for name in ["SDL2", "SDL2_image", "SDL2_ttf", "SDL2_mixer"] {
            let file = root.join(format!("{name}.framework/Versions/A/{name}"));
            let canonical = std::fs::canonicalize(file).map_err(|_| {
                HubError::new(
                    "BUNDLE_LAYOUT_UNSUPPORTED",
                    "发布包内所需框架缺失或不可访问。",
                )
            })?;
            if !canonical.starts_with(&root) || !canonical.is_file() {
                return Err(HubError::new(
                    "BUNDLE_LAYOUT_UNSUPPORTED",
                    "发布包框架不能引用安装范围以外的路径。",
                ));
            }
        }
        if native {
            check_bundle_tree(&paths::installation_scope(&working))?;
        }
        let text = paths::text(&root)?;
        environment.insert("DYLD_FRAMEWORK_PATH".into(), text.clone());
        environment.insert("DYLD_LIBRARY_PATH".into(), text);
        Ok(environment)
    })
    .await
    .map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "发布包布局检查未完成。"))?
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

// Bundle frameworks need internal relative symlinks. Resolve them through the
// package capability, never through the ambient filesystem or by following a
// directory link during traversal. Mutable caches are checked, not hashed.
fn check_bundle_tree(path: &Path) -> Result<()> {
    let unreadable = |_| HubError::new("BUNDLE_SCOPE_UNREADABLE", "无法完整检查安装包范围。");
    let unsafe_scope = || {
        HubError::new(
            "BUNDLE_SCOPE_UNSAFE",
            "安装包含外部、失效或循环链接、硬链接或特殊文件，不能使用原生试运行方案。",
        )
    };
    let root = cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())
        .map_err(unreadable)?;
    let mut pending = vec![(PathBuf::new(), 0)];
    let mut count = 0;
    while let Some((prefix, depth)) = pending.pop() {
        let dir = if prefix.as_os_str().is_empty() {
            root.try_clone().map_err(unreadable)?
        } else {
            // Open each component without following a replaced directory link.
            let mut dir = root.try_clone().map_err(unreadable)?;
            for component in prefix.components() {
                dir = dir
                    .open_dir_nofollow(component.as_os_str())
                    .map_err(unreadable)?;
            }
            dir
        };
        for entry in dir.entries().map_err(unreadable)? {
            count += 1;
            if count > 20_000 {
                return Err(HubError::new(
                    "BUNDLE_SCOPE_LIMIT",
                    "安装包范围超过 20000 项。",
                ));
            }
            let entry = entry.map_err(unreadable)?;
            let name = entry.file_name();
            let relative = prefix.join(&name);
            let metadata = dir.symlink_metadata(&name).map_err(unreadable)?;
            if metadata.file_type().is_symlink() {
                let target = dir.read_link_contents(&name).map_err(unreadable)?;
                if target.is_absolute() || target.as_os_str().is_empty() {
                    return Err(unsafe_scope());
                }
                let resolved = root.canonicalize(&relative).map_err(|_| unsafe_scope())?;
                let target = root.metadata(resolved).map_err(unreadable)?;
                if !target.is_file() && !target.is_dir() {
                    return Err(unsafe_scope());
                }
            } else if metadata.is_dir() {
                if depth >= 32 {
                    return Err(HubError::new(
                        "BUNDLE_SCOPE_LIMIT",
                        "安装包层级超过 32 层。",
                    ));
                }
                pending.push((relative, depth + 1));
            } else if metadata.is_file() {
                #[cfg(unix)]
                {
                    use cap_std::fs::MetadataExt;
                    if metadata.nlink() != 1 {
                        return Err(unsafe_scope());
                    }
                }
            } else {
                return Err(unsafe_scope());
            }
        }
    }
    Ok(())
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
            #[cfg(unix)]
            if kind.is_file() {
                use std::os::unix::fs::MetadataExt;
                let metadata = std::fs::symlink_metadata(entry.path()).map_err(|_| {
                    HubError::new("INSTANCE_SCOPE_UNREADABLE", "无法核实实例文件的链接数。")
                })?;
                if !metadata.is_file() || metadata.nlink() != 1 {
                    return Err(HubError::new(
                        "INSTANCE_SCOPE_UNSAFE",
                        "实例含共享硬链接或检查期间变化的文件，不能使用此试运行方案。",
                    ));
                }
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
    pub(crate) async fn instance(&self, instance_id: &str) -> Result<PlayInstance> {
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
    pub(crate) async fn instance_identity(
        &self,
        instance: &PlayInstance,
        working_dir: &str,
        executable: &str,
    ) -> Result<(same_file::Handle, same_file::Handle, same_file::Handle)> {
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
                .map(|item| paths::installation_scope(&item.working_dir)),
        );
        other.extend(
            self.play_instances()
                .await?
                .into_iter()
                .filter(|item| item.id != instance.id)
                .map(|item| PathBuf::from(item.data_root)),
        );
        let root = instance.data_root.clone();
        let executable_parent = Path::new(executable)
            .parent()
            .ok_or(HubError::new("PATH_INVALID", "游戏程序缺少父目录。"))?
            .to_owned();
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
            // Native entries live in MacOS while cwd is Resources. Pin both directories.
            let executable_parent_handle = same_file::Handle::from_path(executable_parent)
                .map_err(|_| HubError::new("PATH_INVALID", "无法打开游戏程序父目录身份。"))?;
            Ok((root_handle, working_handle, executable_parent_handle))
        })
        .await
        .map_err(|_| HubError::new("INSTANCE_CHECK_FAILED", "实例检查未完成。"))?
    }
    /// Explicit trial or feature-gated candidate. Neither grants verified binding.
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
        self.check_instance_resources(instance_id).await?;
        let working_dir = working_directory(
            profile_id,
            &installation.executable,
            &installation.working_dir,
        )?;
        let environment = profile_environment(
            profile_id,
            &installation.executable,
            &working_dir,
            &instance.data_root,
        )
        .await?;
        let identity = self
            .instance_identity(&instance, &working_dir, &installation.executable)
            .await?;
        let candidate = crate::candidate_launch::prepare(
            profile_id,
            &installation.executable,
            &working_dir,
            &instance.data_root,
        )
        .await?;
        let mut plan = self
            .launch_plan_inner(
                &instance.installation_id,
                profile_args(&instance.data_root),
                candidate.as_ref().map(|_| instance_id),
            )
            .await?;
        plan.working_dir = working_dir;
        plan.play_instance_id = Some(instance.id);
        plan.launch_profile_id = Some(profile_id.into());
        plan.environment = environment;
        plan.instance_data_root = Some(instance.data_root.clone());
        plan.save_dir = paths::text(&PathBuf::from(&instance.data_root).join("save"))?;
        plan.warnings = vec![
            format!(
                "所选启动方案：{profile_id}。程序样本识别与游戏验收分开记录，实际目录绑定和资源兼容性仍需在游戏中验证。"
            ),
            "确认后将准备实例 save/config 子目录并传递目录参数；实际绑定仍未验证，不会复制或迁移原存档。".into(),
            "游戏还可能写入安装基础目录和数据目录的缓存；本方案保持全局互斥，不允许并行游玩。"
                .into(),
            "只监测直接进程；退出不代表子进程已关闭或存档稳定。实例数据可单独备份，安装缓存不在其中。".into(),
        ];
        if candidate.is_some() {
            plan.warnings = vec![
                "并发实验候选：仅适用于固定 QA 程序和静态资源组合，未完成真实双开验收，不代表正式版本支持。".into(),
                "完整安装包与完整实例根参与写入冲突判定；同包、同实例及未知会话仍会阻止启动。文件任务继续串行。".into(),
                "仅监测直接进程；异常退出、结果记录失败或 Hub 重启后恢复全局保护。进程退出不证明存档一致。".into(),
                "进程环境仅使用预览中的变量，不继承其他环境；目录检查不是操作系统沙箱。请仅用可丢弃测试数据。".into(),
            ];
        }
        if has_runtime(profile_id) && candidate.is_none() {
            plan.warnings.push("新版方案还将在实例 .hub-runtime 内准备 HOME、临时及 XDG 配置/缓存/数据/状态目录；其他环境仍继承，系统 API 或绝对路径仍可能写到其他位置，这不是沙箱或完整隔离证明。".into());
        }
        if native_entry(profile_id) {
            plan.warnings.push("原生本体入口方案仅用于已含 Contents/MacOS 下 Mach-O 本体的包；不会创建入口、更改 Info.plist 或签名，不使用启动脚本。原生窗口可识别不代表游戏版本、存档或隔离已验证。".into());
        }
        self.plans.lock().await.insert(
            plan.id.clone(),
            PreparedLaunch {
                plan: plan.clone(),
                instance_identity: Some(Arc::new(identity)),
                candidate,
            },
        );
        Ok(plan)
    }
    pub(crate) async fn revalidate_instance_launch(
        &self,
        plan: &LaunchPlan,
        identity: &(same_file::Handle, same_file::Handle, same_file::Handle),
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
        let environment = profile_environment(
            profile,
            &plan.executable,
            &plan.working_dir,
            &instance.data_root,
        )
        .await?;
        if plan.environment != environment
            || plan.instance_data_root.as_deref() != Some(&instance.data_root)
            || plan.installation_id != instance.installation_id
            || plan.executable_sha256 != instance.installation_sha256
            || plan.args != profile_args(&instance.data_root)
        {
            return Err(HubError::new("PLAN_STALE", "实例预览已变化，请重新生成。"));
        }
        let current = self
            .instance_identity(&instance, &plan.working_dir, &plan.executable)
            .await?;
        if current.0 != identity.0 || current.1 != identity.1 || current.2 != identity.2 {
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
    identity: Arc<(same_file::Handle, same_file::Handle, same_file::Handle)>,
) -> Result<()> {
    let path = plan
        .instance_data_root
        .clone()
        .ok_or(HubError::new("PLAN_STALE", "实例目录缺失。"))?;
    let runtime = has_runtime(plan.launch_profile_id.as_deref().unwrap_or(""));
    tokio::task::spawn_blocking(move || {
        let error = |_| {
            HubError::new(
                "INSTANCE_PREPARE_FAILED",
                "无法准备实例数据或运行环境目录，程序未启动。已有文件未被覆盖。",
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
        if runtime {
            let prepare_dir = |parent: &cap_std::fs::Dir, name: &str| {
                #[cfg(unix)]
                let mut builder = cap_std::fs::DirBuilder::new();
                #[cfg(not(unix))]
                let builder = cap_std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use cap_std::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match parent.create_dir_with(name, &builder) {
                    Ok(()) => {}
                    Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(cause) => return Err(error(cause)),
                }
                parent.open_dir_nofollow(name).map_err(error)
            };
            let runtime_root = prepare_dir(&root, RUNTIME_DIR)?;
            for role in RUNTIME_ROLES {
                prepare_dir(&runtime_root, role)?;
            }
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
