mod portable;

use portable::{
    bootstrap_kit, check_health, get_dashboard, get_marketplace_tools,
    login_tool as open_tool_login, run_tool, tool_action, AppError, AppState, Dashboard,
    HealthReport, MarketplaceTool, ToolActionRequest, ToolCommandResult,
};

#[tauri::command]
async fn bootstrap(force: Option<bool>) -> Result<Dashboard, AppError> {
    tokio::task::spawn_blocking(move || -> Result<Dashboard, AppError> {
        let app = AppState::discover()?;
        bootstrap_kit(&app)?;
        get_dashboard(&app, force.unwrap_or(false))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn dashboard(force: Option<bool>) -> Result<Dashboard, AppError> {
    tokio::task::spawn_blocking(move || -> Result<Dashboard, AppError> {
        let app = AppState::discover()?;
        get_dashboard(&app, force.unwrap_or(false))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn health() -> Result<HealthReport, AppError> {
    tokio::task::spawn_blocking(|| -> Result<HealthReport, AppError> {
        let app = AppState::discover()?;
        check_health(&app)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn install_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        tool_action(&app, ToolActionRequest::new(tool_id, "install"))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn uninstall_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        tool_action(&app, ToolActionRequest::new(tool_id, "uninstall"))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn update_tool(tool_id: String) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        tool_action(&app, ToolActionRequest::new(tool_id, "update"))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn launch_tool(
    tool_id: String,
    workspace_dir: Option<String>,
) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        run_tool(&app, &tool_id, workspace_dir)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn login_tool(
    tool_id: String,
    workspace_dir: Option<String>,
) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        open_tool_login(&app, &tool_id, workspace_dir)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn select_workspace_dialog() -> Result<Option<String>, AppError> {
    tokio::task::spawn_blocking(|| -> Result<Option<String>, AppError> {
        Ok(rfd::FileDialog::new()
            .set_title("选择 AI CLI 工作目录")
            .pick_folder()
            .map(|path| path.display().to_string()))
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn add_custom_tool(
    name: String,
    install_type: String,
    package_name: Option<String>,
    script_url: Option<String>,
    bin_name: Option<String>,
) -> Result<Dashboard, AppError> {
    tokio::task::spawn_blocking(move || -> Result<Dashboard, AppError> {
        let app = AppState::discover()?;
        portable::add_custom_tool(
            &app,
            name,
            &install_type,
            package_name,
            script_url,
            bin_name,
        )?;
        get_dashboard(&app, false)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn delete_custom_tool(tool_id: String) -> Result<Dashboard, AppError> {
    tokio::task::spawn_blocking(move || -> Result<Dashboard, AppError> {
        let app = AppState::discover()?;
        portable::delete_custom_tool(&app, tool_id)?;
        get_dashboard(&app, false)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn marketplace_tools() -> Result<Vec<MarketplaceTool>, AppError> {
    tokio::task::spawn_blocking(|| -> Result<Vec<MarketplaceTool>, AppError> {
        let app = AppState::discover()?;
        get_marketplace_tools(&app)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}

#[tauri::command]
async fn install_marketplace_tool(
    id: String,
    name: String,
    package_name: String,
) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        portable::install_marketplace_tool(&app, id, name, package_name)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
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
            select_workspace_dialog,
            add_custom_tool,
            delete_custom_tool,
            marketplace_tools,
            install_marketplace_tool
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Portable AI Dev Kit");
}
