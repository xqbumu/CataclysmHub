use hub_core::{
    Dashboard, Hub, HubError, Installation, LaunchPlan, ScanReport, ScanTask, Session, Workspace,
    WorkspaceInput,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

struct AppState {
    hub: Arc<Hub>,
    selected: Mutex<HashSet<PathBuf>>,
    managed_root: PathBuf,
}
fn selection_error() -> HubError {
    HubError::new("PATH_NOT_SELECTED", "请通过本机文件选择器重新选择路径。")
}
fn authorized(state: &AppState, path: &str) -> Result<(), HubError> {
    let path = std::fs::canonicalize(path).map_err(|_| selection_error())?;
    if !state
        .selected
        .lock()
        .map_err(|_| selection_error())?
        .contains(&path)
    {
        return Err(selection_error());
    }
    Ok(())
}
fn default_workspace(state: &AppState) -> Result<WorkspaceInput, HubError> {
    let text = |name: &str| {
        state
            .managed_root
            .join(name)
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| HubError::new("PATH_UNSUPPORTED", "系统目录无法表示为 UTF-8。"))
    };
    Ok(WorkspaceInput {
        save_dir: text("saves")?,
        backup_dir: text("backups")?,
        content_dir: text("content")?,
    })
}
fn prepare_default_directory(state: &AppState, name: &str) -> Result<(), HubError> {
    use cap_fs_ext::DirExt;
    let err = |_| {
        HubError::new(
            "DEFAULT_DIRECTORY_FAILED",
            "无法创建系统推荐目录，请检查权限或使用自定义目录。",
        )
    };
    let parent = state.managed_root.parent().ok_or_else(selection_error)?;
    std::fs::create_dir_all(parent).map_err(err)?;
    let parent =
        cap_std::fs::Dir::open_ambient_dir(parent, cap_std::ambient_authority()).map_err(err)?;
    let leaf = state.managed_root.file_name().ok_or_else(selection_error)?;
    match parent.create_dir(leaf) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(err(e)),
    }
    let root = parent.open_dir_nofollow(leaf).map_err(err)?;
    match root.create_dir(name) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(err(e)),
    }
    root.open_dir_nofollow(name).map_err(err)?;
    Ok(())
}
#[tauri::command]
async fn register_managed_instance(
    state: tauri::State<'_, AppState>,
    installation_id: String,
    name: String,
) -> Result<hub_core::PlayInstance, HubError> {
    state
        .hub
        .register_managed_instance(&state.managed_root, installation_id, name)
        .await
}
#[tauri::command]
async fn workspace_defaults(state: tauri::State<'_, AppState>) -> Result<WorkspaceInput, HubError> {
    default_workspace(&state)
}
#[tauri::command]
async fn hub_preferences(
    state: tauri::State<'_, AppState>,
) -> Result<hub_core::HubPreferences, HubError> {
    state.hub.preferences().await
}
#[tauri::command]
async fn save_instance_profile(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    profile: String,
) -> Result<hub_core::HubPreferences, HubError> {
    state
        .hub
        .save_instance_profile(&instance_id, &profile)
        .await
}
#[tauri::command]
async fn reset_hub_preferences(
    state: tauri::State<'_, AppState>,
    revision: i64,
    confirmed: bool,
) -> Result<hub_core::PreferencesReset, HubError> {
    state.hub.reset_preferences(revision, confirmed).await
}
#[tauri::command]
async fn pick_path(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    kind: String,
) -> Result<Option<String>, HubError> {
    let executable = match kind.as_str() {
        "directory" => false,
        "executable" => true,
        _ => {
            return Err(HubError::new(
                "PICKER_KIND_INVALID",
                "不支持此路径选择类型。",
            ));
        }
    };
    let selected = tauri::async_runtime::spawn_blocking(move || {
        let picker = app.dialog().file().set_title(if executable {
            "选择游戏可执行文件"
        } else {
            "选择现有目录"
        });
        if executable {
            picker.blocking_pick_file()
        } else {
            picker.blocking_pick_folder()
        }
    })
    .await
    .map_err(|_| selection_error())?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|_| selection_error())?;
    let path = std::fs::canonicalize(path).map_err(|_| selection_error())?;
    let text = path
        .to_str()
        .ok_or(HubError::new("PATH_UNSUPPORTED", "暂不支持非 UTF-8 路径。"))?
        .to_owned();
    state
        .selected
        .lock()
        .map_err(|_| selection_error())?
        .insert(path);
    Ok(Some(text))
}
#[tauri::command]
async fn prepare_content_install(
    state: tauri::State<'_, AppState>,
    entry_id: String,
    parent_dir: String,
) -> Result<hub_core::ContentInstallPlan, HubError> {
    authorized(&state, &parent_dir)?;
    state
        .hub
        .prepare_content_install(&entry_id, &parent_dir)
        .await
}
#[tauri::command]
async fn start_content_install(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    digest: String,
    key: String,
    confirmed: bool,
) -> Result<String, HubError> {
    state
        .hub
        .start_content_install(&plan_id, &digest, &key, confirmed)
        .await
}
#[tauri::command]
async fn content_install_tasks(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<hub_core::ContentInstallTask>, HubError> {
    state.hub.content_install_tasks().await
}
#[tauri::command]
async fn cancel_content_install(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<(), HubError> {
    state.hub.cancel_content_install(&id).await
}
#[tauri::command]
async fn prepare_config_rollback(
    state: tauri::State<'_, AppState>,
    change_id: String,
) -> Result<hub_core::ConfigChangePlan, HubError> {
    state.hub.prepare_config_rollback(&change_id).await
}
#[tauri::command]
async fn prepare_config_change(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    input: hub_core::ContentConfigInput,
    backup_id: String,
) -> Result<hub_core::ConfigChangePlan, HubError> {
    state
        .hub
        .prepare_config_change(&instance_id, input, &backup_id)
        .await
}
#[tauri::command]
async fn apply_config_change(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    digest: String,
    request_key: String,
    confirmed: bool,
) -> Result<hub_core::ConfigChange, HubError> {
    state
        .hub
        .apply_config_change(&plan_id, &digest, &request_key, confirmed)
        .await
}
#[tauri::command]
async fn config_changes(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<hub_core::ConfigChange>, HubError> {
    state.hub.config_changes().await
}
#[tauri::command]
async fn inspect_config_change(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<hub_core::ConfigChangeObservation, HubError> {
    state.hub.inspect_config_change(&id).await
}
#[tauri::command]
async fn resolve_config_change(
    state: tauri::State<'_, AppState>,
    id: String,
    expected_hash: String,
    confirmed: bool,
) -> Result<hub_core::ConfigChangeObservation, HubError> {
    state
        .hub
        .resolve_config_change(&id, &expected_hash, confirmed)
        .await
}
#[tauri::command]
async fn prepare_content_config_protection(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    input: hub_core::ContentConfigInput,
) -> Result<hub_core::ContentConfigProtectionPlan, HubError> {
    state
        .hub
        .prepare_content_config_protection(&instance_id, input)
        .await
}
#[tauri::command]
async fn preview_content_config(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    input: hub_core::ContentConfigInput,
) -> Result<hub_core::ContentConfigPlan, HubError> {
    state.hub.preview_content_config(&instance_id, input).await
}
#[tauri::command]
async fn inspect_content_config(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<hub_core::ContentConfigReport, HubError> {
    state.hub.inspect_content_config(&instance_id).await
}
#[tauri::command]
async fn content_entries(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<hub_core::ContentEntry>, HubError> {
    state.hub.content_entries().await
}
#[tauri::command]
async fn register_content(
    state: tauri::State<'_, AppState>,
    root: String,
    kind: String,
) -> Result<hub_core::ContentEntry, HubError> {
    authorized(&state, &root)?;
    state.hub.register_content(&root, &kind).await
}
#[tauri::command]
async fn refresh_content(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<hub_core::ContentEntry, HubError> {
    state.hub.refresh_content(&id).await
}
#[tauri::command]
async fn forget_content(state: tauri::State<'_, AppState>, id: String) -> Result<(), HubError> {
    state.hub.forget_content(&id).await
}
#[tauri::command]
async fn prepare_installation_copy(
    state: tauri::State<'_, AppState>,
    installation_id: String,
    source_root: String,
    parent_dir: String,
) -> Result<hub_core::ManagedInstallationCopyPlan, HubError> {
    authorized(&state, &source_root)?;
    authorized(&state, &parent_dir)?;
    state
        .hub
        .prepare_managed_installation_copy(&installation_id, &source_root, &parent_dir)
        .await
}
#[tauri::command]
async fn start_installation_copy(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    plan_digest: String,
    request_key: String,
    confirmed: bool,
) -> Result<hub_core::InstallationCopyTask, HubError> {
    state
        .hub
        .start_installation_copy(&plan_id, &plan_digest, &request_key, confirmed)
        .await
}
#[tauri::command]
async fn cancel_installation_copy(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<hub_core::InstallationCopyTask, HubError> {
    state.hub.cancel_installation_copy(&task_id).await
}
#[tauri::command]
async fn register_copied_installation(
    state: tauri::State<'_, AppState>,
    task_id: String,
    name: String,
) -> Result<Installation, HubError> {
    state
        .hub
        .register_copied_installation(&task_id, &name)
        .await
}
#[tauri::command]
async fn get_dashboard(state: tauri::State<'_, AppState>) -> Result<Dashboard, HubError> {
    state.hub.dashboard().await
}
#[tauri::command]
async fn initialize_workspace(
    state: tauri::State<'_, AppState>,
    input: WorkspaceInput,
) -> Result<Workspace, HubError> {
    initialize_workspace_impl(&state, input).await
}
async fn initialize_workspace_impl(
    state: &AppState,
    input: WorkspaceInput,
) -> Result<Workspace, HubError> {
    if state.hub.dashboard().await?.workspace.is_some() {
        return Err(HubError::new(
            "WORKSPACE_EXISTS",
            "工作区已配置，不会覆盖现有路径。",
        ));
    }
    let defaults = default_workspace(state)?;
    let selections = [
        (&input.save_dir, &defaults.save_dir, "saves"),
        (&input.backup_dir, &defaults.backup_dir, "backups"),
        (&input.content_dir, &defaults.content_dir, "content"),
    ];
    // Validate all custom authorizations before creating any managed directory.
    for (path, default, _) in selections {
        if path != default {
            authorized(state, path)?;
        }
    }
    for (path, default, name) in selections {
        if path == default {
            prepare_default_directory(state, name)?;
        }
    }
    state.hub.initialize(input).await
}
#[tauri::command]
async fn import_installation(
    state: tauri::State<'_, AppState>,
    executable: String,
    name: String,
) -> Result<Installation, HubError> {
    authorized(&state, &executable)?;
    state.hub.import_installation(&executable, &name).await
}
#[tauri::command]
async fn register_play_instance(
    state: tauri::State<'_, AppState>,
    input: hub_core::PlayInstanceInput,
) -> Result<hub_core::PlayInstance, HubError> {
    authorized(&state, &input.data_root)?;
    state.hub.register_play_instance(input).await
}
#[tauri::command]
async fn preview_instance_resources(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<hub_core::InstanceResourcePreview, HubError> {
    state.hub.preview_instance_resources(&instance_id).await
}
#[tauri::command]
async fn create_instance_launch_plan(
    state: tauri::State<'_, AppState>,
    instance_id: String,
    profile_id: String,
) -> Result<LaunchPlan, HubError> {
    state
        .hub
        .instance_launch_plan(&instance_id, &profile_id)
        .await
}
#[tauri::command]
async fn create_launch_plan(
    state: tauri::State<'_, AppState>,
    installation_id: String,
    args: Vec<String>,
) -> Result<LaunchPlan, HubError> {
    state.hub.launch_plan(&installation_id, args).await
}
#[tauri::command]
async fn launch_game(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    confirmed: bool,
) -> Result<Session, HubError> {
    state.hub.launch(&plan_id, confirmed).await
}
#[tauri::command]
async fn acknowledge_interrupted(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<(), HubError> {
    state.hub.acknowledge_interrupted(&session_id).await
}

#[tauri::command]
async fn start_world_scan(
    state: tauri::State<'_, AppState>,
    request_key: String,
) -> Result<ScanTask, HubError> {
    state.hub.start_world_scan(&request_key).await
}
#[tauri::command]
async fn cancel_world_scan(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<ScanTask, HubError> {
    state.hub.cancel_scan(&task_id).await
}
#[tauri::command]
async fn get_scan_report(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<ScanReport, HubError> {
    state.hub.scan_report(&task_id).await
}

#[tauri::command]
async fn prepare_instance_backup(
    state: tauri::State<'_, AppState>,
    instance_id: String,
) -> Result<hub_core::BackupPlan, HubError> {
    state.hub.prepare_instance_backup(&instance_id).await
}
#[tauri::command]
async fn prepare_backup(
    state: tauri::State<'_, AppState>,
) -> Result<hub_core::BackupPlan, HubError> {
    state.hub.prepare_backup().await
}
#[tauri::command]
async fn create_backup(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    plan_digest: String,
    request_key: String,
    confirmed: bool,
) -> Result<hub_core::BackupTask, HubError> {
    state
        .hub
        .create_backup(&plan_id, &plan_digest, &request_key, confirmed)
        .await
}
#[tauri::command]
async fn verify_backup(
    state: tauri::State<'_, AppState>,
    backup_id: String,
    request_key: String,
) -> Result<hub_core::BackupTask, HubError> {
    state.hub.verify_backup(&backup_id, &request_key).await
}
#[tauri::command]
async fn cancel_backup(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<hub_core::BackupTask, HubError> {
    state.hub.cancel_backup(&task_id).await
}

#[tauri::command]
async fn retry_task_recovery(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<(), HubError> {
    state.hub.retry_task_recovery(&task_id).await
}

#[tauri::command]
async fn prepare_restore(
    state: tauri::State<'_, AppState>,
    backup_id: String,
    parent_dir: String,
) -> Result<hub_core::RestorePlan, HubError> {
    authorized(&state, &parent_dir)?;
    state.hub.prepare_restore(&backup_id, &parent_dir).await
}
#[tauri::command]
async fn start_restore(
    state: tauri::State<'_, AppState>,
    plan_id: String,
    plan_digest: String,
    request_key: String,
    confirmed: bool,
) -> Result<hub_core::RestoreTask, HubError> {
    state
        .hub
        .start_restore(&plan_id, &plan_digest, &request_key, confirmed)
        .await
}
#[tauri::command]
async fn cancel_restore(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<hub_core::RestoreTask, HubError> {
    state.hub.cancel_restore(&task_id).await
}

#[tauri::command]
async fn start_inspection(
    state: tauri::State<'_, AppState>,
    restore_task_id: String,
    request_key: String,
) -> Result<hub_core::InspectionTask, HubError> {
    state
        .hub
        .start_inspection(&restore_task_id, &request_key)
        .await
}
#[tauri::command]
async fn cancel_inspection(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<hub_core::InspectionTask, HubError> {
    state.hub.cancel_inspection(&task_id).await
}

#[tauri::command]
async fn update_backup_details(
    state: tauri::State<'_, AppState>,
    input: hub_core::BackupDetailsInput,
) -> Result<hub_core::BackupRecord, HubError> {
    state.hub.update_backup_details(input).await
}

#[tauri::command]
async fn preview_retention(
    state: tauri::State<'_, AppState>,
    policy: hub_core::RetentionPolicy,
) -> Result<hub_core::RetentionPreview, HubError> {
    state.hub.preview_retention(policy).await
}

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            #[cfg(debug_assertions)]
            let dir = std::env::var_os("CATACLYSM_HUB_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(dir);
            let hub = match tauri::async_runtime::block_on(Hub::open(&dir)) {
                Ok(hub) => hub,
                Err(error) => {
                    app.dialog()
                        .message(error.message)
                        .title("无法打开 Cataclysm Hub")
                        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
                        .blocking_show();
                    return Err(Box::new(error));
                }
            };
            let local = app.path().app_local_data_dir()?;
            #[cfg(debug_assertions)]
            let local = if std::env::var_os("CATACLYSM_HUB_DATA_DIR").is_some() {
                dir.clone()
            } else {
                local
            };
            if !local.is_absolute() {
                return Err("应用数据目录必须是绝对路径".into());
            }
            let namespace = local
                .file_name()
                .ok_or("应用数据目录缺少名称")?
                .to_string_lossy();
            let managed_root = local.with_file_name(format!("{namespace}.workspace"));
            app.manage(AppState {
                hub,
                selected: Mutex::new(HashSet::new()),
                managed_root,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            register_managed_instance,
            workspace_defaults,
            hub_preferences,
            save_instance_profile,
            reset_hub_preferences,
            inspect_content_config,
            preview_content_config,
            prepare_content_config_protection,
            prepare_config_change,
            prepare_config_rollback,
            apply_config_change,
            config_changes,
            inspect_config_change,
            resolve_config_change,
            prepare_content_install,
            start_content_install,
            content_install_tasks,
            cancel_content_install,
            content_entries,
            register_content,
            refresh_content,
            forget_content,
            pick_path,
            get_dashboard,
            prepare_installation_copy,
            start_installation_copy,
            cancel_installation_copy,
            register_copied_installation,
            initialize_workspace,
            import_installation,
            register_play_instance,
            create_launch_plan,
            create_instance_launch_plan,
            preview_instance_resources,
            launch_game,
            acknowledge_interrupted,
            start_world_scan,
            cancel_world_scan,
            get_scan_report,
            prepare_backup,
            prepare_instance_backup,
            create_backup,
            verify_backup,
            cancel_backup,
            retry_task_recovery,
            prepare_restore,
            start_restore,
            cancel_restore,
            start_inspection,
            cancel_inspection,
            update_backup_details,
            preview_retention
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        // Never print the full framework error: it can include a user's private path.
        eprintln!("APPLICATION_START_FAILED");
        let _ = error;
        std::process::exit(1);
    }
}

#[cfg(test)]
mod directory_tests {
    use super::*;
    async fn fixture() -> (tempfile::TempDir, AppState) {
        let temp = tempfile::tempdir().unwrap();
        let hub = Hub::open(&temp.path().join("metadata")).await.unwrap();
        let state = AppState {
            hub,
            selected: Mutex::new(HashSet::new()),
            managed_root: temp.path().join("app.workspace"),
        };
        (temp, state)
    }
    #[tokio::test]
    async fn defaults_are_read_only_until_saved_and_existing_workspace_is_preserved() {
        let (_temp, state) = fixture().await;
        let defaults = default_workspace(&state).unwrap();
        assert!(!state.managed_root.exists());
        let workspace = initialize_workspace_impl(&state, defaults.clone())
            .await
            .unwrap();
        assert!(PathBuf::from(&workspace.save_dir).is_dir());
        assert!(PathBuf::from(&workspace.backup_dir).is_dir());
        assert!(PathBuf::from(&workspace.content_dir).is_dir());
        assert_eq!(
            initialize_workspace_impl(&state, defaults)
                .await
                .unwrap_err()
                .code,
            "WORKSPACE_EXISTS"
        );
        assert_eq!(
            state.hub.dashboard().await.unwrap().workspace.unwrap().id,
            workspace.id
        );
    }
    #[tokio::test]
    async fn arbitrary_paths_still_require_picker_authorization_before_creating_defaults() {
        let (temp, state) = fixture().await;
        let mut input = default_workspace(&state).unwrap();
        let custom = temp.path().join("custom");
        std::fs::create_dir(&custom).unwrap();
        input.save_dir = custom.display().to_string();
        assert_eq!(
            initialize_workspace_impl(&state, input.clone())
                .await
                .unwrap_err()
                .code,
            "PATH_NOT_SELECTED"
        );
        assert!(!state.managed_root.exists());
        state
            .selected
            .lock()
            .unwrap()
            .insert(std::fs::canonicalize(custom).unwrap());
        assert!(initialize_workspace_impl(&state, input).await.is_ok());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn managed_directories_do_not_follow_external_symlinks() {
        let (temp, state) = fixture().await;
        std::fs::create_dir(&state.managed_root).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, state.managed_root.join("saves")).unwrap();
        let input = default_workspace(&state).unwrap();
        assert_eq!(
            initialize_workspace_impl(&state, input)
                .await
                .unwrap_err()
                .code,
            "DEFAULT_DIRECTORY_FAILED"
        );
        assert_eq!(std::fs::read_dir(outside).unwrap().count(), 0);
        assert!(state.hub.dashboard().await.unwrap().workspace.is_none());
    }
}
