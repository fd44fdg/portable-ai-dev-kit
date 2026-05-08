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
fn install_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    let app = AppState::discover()?;
    tool_action(&app, ToolActionRequest::new(tool_id, "install"))
}

#[tauri::command]
fn uninstall_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    let app = AppState::discover()?;
    tool_action(&app, ToolActionRequest::new(tool_id, "uninstall"))
}

#[tauri::command]
fn update_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    let app = AppState::discover()?;
    tool_action(&app, ToolActionRequest::new(tool_id, "update"))
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
