mod portable;

use portable::{
    add_custom_command_tool as add_custom_command_tool_impl,
    add_custom_npm_tool as add_custom_npm_tool_impl, bootstrap_kit, check_health, get_dashboard,
    inspect_npm_package as inspect_npm_package_impl, login_tool as open_tool_login, run_tool,
    search_npm_packages as search_npm_packages_impl, tool_action, AddCommandToolRequest,
    AddNpmToolRequest, AppError, AppState, Dashboard, HealthReport, NpmPackageCandidate,
    NpmPackageSuggestion, ToolActionRequest, ToolCommandResult,
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

#[tauri::command]
async fn add_custom_npm_tool(request: AddNpmToolRequest) -> Result<ToolCommandResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = AppState::discover()?;
        add_custom_npm_tool_impl(&app, request)
    })
    .await
    .map_err(|error| AppError::Message(format!("后台任务执行失败：{}", error)))?
}

#[tauri::command]
async fn add_custom_command_tool(
    request: AddCommandToolRequest,
) -> Result<ToolCommandResult, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = AppState::discover()?;
        add_custom_command_tool_impl(&app, request)
    })
    .await
    .map_err(|error| AppError::Message(format!("后台任务执行失败：{}", error)))?
}

#[tauri::command]
async fn search_npm_packages(query: String) -> Result<Vec<NpmPackageCandidate>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = AppState::discover()?;
        search_npm_packages_impl(&app, &query)
    })
    .await
    .map_err(|error| AppError::Message(format!("后台任务执行失败：{}", error)))?
}

#[tauri::command]
async fn inspect_npm_package(package_name: String) -> Result<NpmPackageSuggestion, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let app = AppState::discover()?;
        inspect_npm_package_impl(&app, &package_name)
    })
    .await
    .map_err(|error| AppError::Message(format!("后台任务执行失败：{}", error)))?
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
            login_tool,
            add_custom_npm_tool,
            add_custom_command_tool,
            search_npm_packages,
            inspect_npm_package
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Portable AI Dev Kit");
}
