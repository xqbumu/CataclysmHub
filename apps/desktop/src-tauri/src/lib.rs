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
    for path in [&input.save_dir, &input.backup_dir, &input.content_dir] {
        authorized(&state, path)?;
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
            app.manage(AppState {
                hub,
                selected: Mutex::new(HashSet::new()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
