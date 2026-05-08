mod portable;

use portable::{
    bootstrap_kit, check_health, get_dashboard, login_tool as open_tool_login, run_tool,
    tool_action, AppError, AppState, Dashboard, HealthReport, ToolActionRequest, ToolCommandResult,
};

#[tauri::command]
fn bootstrap() -> Result<Dashboard, AppError> {
    let app = AppState::discover()?;
    bootstrap_kit(&app)?;
    get_dashboard(&app)
}

#[tauri::command]
fn dashboard() -> Result<Dashboard, AppError> {
    let app = AppState::discover()?;
    get_dashboard(&app)
}

#[tauri::command]
fn health() -> Result<HealthReport, AppError> {
    let app = AppState::discover()?;
    check_health(&app)
}

#[tauri::command]
async fn install_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    run_tool_action(tool_id, "install").await
}

#[tauri::command]
async fn uninstall_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    run_tool_action(tool_id, "uninstall").await
}

#[tauri::command]
async fn update_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    run_tool_action(tool_id, "update").await
}

#[tauri::command]
fn launch_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    let app = AppState::discover()?;
    run_tool(&app, &tool_id)
}

#[tauri::command]
fn login_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    let app = AppState::discover()?;
    open_tool_login(&app, &tool_id)
}

async fn run_tool_action(
    tool_id: String,
    action: &'static str,
) -> Result<ToolCommandResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = AppState::discover()?;
        tool_action(&app, ToolActionRequest::new(tool_id, action))
    })
    .await
    .map_err(|error| AppError::Message(format!("后台任务执行失败：{}", error)))?
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            dashboard,
            health,
            install_tool,
            uninstall_tool,
            update_tool,
            launch_tool,
            login_tool
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Portable AI Dev Kit");
}
