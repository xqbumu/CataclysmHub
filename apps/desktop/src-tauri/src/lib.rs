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
            initialize_workspace,
            import_installation,
            create_launch_plan,
            launch_game,
            acknowledge_interrupted,
            start_world_scan,
            cancel_world_scan,
            get_scan_report,
            prepare_backup,
            create_backup,
            verify_backup,
            cancel_backup
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        // Never print the full framework error: it can include a user's private path.
        eprintln!("APPLICATION_START_FAILED");
        let _ = error;
        std::process::exit(1);
    }
}
