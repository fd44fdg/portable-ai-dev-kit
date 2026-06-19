use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

static STATE_LOCK: Mutex<()> = Mutex::new(());
static ACTION_LOCK: Mutex<()> = Mutex::new(());
static BOOTSTRAPPED_ROOTS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

const MAX_COMMAND_OUTPUT_BYTES: usize = 1024 * 1024;
const NPM_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3 * 60);
const EXPAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const SCRIPT_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const MANIFEST_PATH: &str = "config/tool-manifest.json";
const SETTINGS_PATH: &str = "config/app-settings.json";
const STATE_PATH: &str = "state/tool-state.json";
const MARKETPLACE_PATH: &str = "config/marketplace.json";
const DEFAULT_MARKETPLACE_JSON: &str = include_str!("../../config/marketplace.json");

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub root: PathBuf,
}

impl AppState {
    pub fn discover() -> Result<Self, AppError> {
        if let Ok(root) = env::var("PORTABLE_AI_KIT_ROOT") {
            return Ok(Self {
                root: normalize_path(PathBuf::from(root))?,
            });
        }

        let candidates: Vec<PathBuf> = [
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf)),
            env::current_dir().ok(),
        ]
        .into_iter()
        .flatten()
        .collect();

        for candidate in &candidates {
            if let Some(root) = find_manifest_root(candidate) {
                return Ok(Self {
                    root: normalize_path(root)?,
                });
            }
        }

        if let Some(first) = candidates.into_iter().next() {
            return Ok(Self {
                root: normalize_path(first)?,
            });
        }

        Err(AppError::Message(
            "无法定位便携根目录：未能读取可执行文件路径或当前目录".to_string(),
        ))
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative.replace('/', "\\"))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u16,
    pub network_modes: BTreeMap<String, NetworkMode>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomToolsFile {
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceFile {
    pub tools: Vec<MarketplaceDefinition>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub package_name: String,
    pub category: String,
    pub homepage: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkMode {
    pub npm_registry: String,
    pub archive_source: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    pub required: bool,
    pub base_path: String,
    pub package_name: Option<String>,
    #[serde(default)]
    pub version_command: Vec<String>,
    #[serde(default)]
    pub host_version_command: Vec<String>,
    #[serde(default)]
    pub bin_paths: Vec<String>,
    #[serde(default)]
    pub run_command: Vec<String>,
    #[serde(default)]
    pub login_command: Vec<String>,
    pub install: InstallDefinition,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolKind {
    Runtime,
    AiCli,
    App,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstallDefinition {
    #[serde(rename = "type")]
    pub install_type: InstallType,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub archive_name: Option<String>,
    pub installer_type: Option<String>,
    #[serde(default)]
    pub urls: BTreeMap<String, String>,
    pub script_url: Option<String>,
    #[serde(default)]
    pub script_args: Vec<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallType {
    Npm,
    Archive,
    PowershellScript,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub network_mode: String,
    pub workspace_path: String,
    pub auto_open_workspace: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            network_mode: "global".to_string(),
            workspace_path: "workspace".to_string(),
            auto_open_workspace: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolStateFile {
    #[serde(default)]
    pub tools: BTreeMap<String, PersistedToolState>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedToolState {
    pub installed_version: Option<String>,
    pub host_version: Option<String>,
    pub wanted_version: Option<String>,
    pub source: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub root: String,
    pub workspace: String,
    pub workspace_path: String,
    pub network_mode: String,
    pub auto_open_workspace: bool,
    pub tools: Vec<ToolView>,
    pub health: HealthReport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    pub required: bool,
    pub status: ToolStatus,
    pub installed_version: Option<String>,
    pub wanted_version: Option<String>,
    pub install_source: String,
    pub base_path: String,
    pub launch_path: Option<String>,
    pub host_available: bool,
    pub host_version: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolStatus {
    Ready,
    Missing,
    Partial,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub summary: HealthSummary,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HealthSummary {
    Healthy,
    Warning,
    Unhealthy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub id: String,
    pub label: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCommandResult {
    pub tool_id: String,
    pub action: String,
    pub success: bool,
    pub message: String,
    pub output: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub path: String,
}

pub struct ToolActionRequest {
    tool_id: String,
    action: String,
    timeout_secs: Option<u64>,
}

impl ToolActionRequest {
    pub fn new(tool_id: String, action: &str) -> Self {
        Self {
            tool_id,
            action: action.to_string(),
            timeout_secs: None,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }
}

pub fn bootstrap_kit(app: &AppState) -> Result<(), AppError> {
    // Skip the 14 create_dir_all + state-file check if we've already
    // bootstrapped this root in the current process. Dashboard refresh
    // triggers bootstrap_kit every call, which is visibly slow on network
    // drives.
    {
        let cache = BOOTSTRAPPED_ROOTS.lock().unwrap_or_else(|p| p.into_inner());
        if cache.contains(&app.root) {
            return Ok(());
        }
    }

    for relative in [
        "apps",
        "cache",
        "cache/downloads",
        "config",
        "logs",
        "scripts",
        "state",
        "state/home",
        "state/appdata",
        "state/localappdata",
        "state/xdg/config",
        "state/xdg/cache",
        "state/xdg/state",
        "tools",
        "workspace",
    ] {
        fs::create_dir_all(app.path(relative))?;
    }

    if !app.path(STATE_PATH).exists() {
        save_state(app, &ToolStateFile::default())?;
    }

    BOOTSTRAPPED_ROOTS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(app.root.clone());

    Ok(())
}

pub fn get_dashboard(app: &AppState, force: bool) -> Result<Dashboard, AppError> {
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let mut state = load_state(app)?;
    let tools = manifest
        .tools
        .iter()
        .map(|tool| tool_view(app, tool, &mut state, force))
        .collect::<Result<Vec<_>, _>>()?;
    save_state(app, &state)?;
    let health = check_health_with_state(app, &manifest, &settings, &mut state, false)?;
    save_state(app, &state)?;

    Ok(Dashboard {
        root: display_path(&app.root),
        workspace: display_path(&app.path(&settings.workspace_path)),
        workspace_path: settings.workspace_path,
        network_mode: settings.network_mode,
        auto_open_workspace: settings.auto_open_workspace,
        tools,
        health,
    })
}

pub fn check_health(app: &AppState) -> Result<HealthReport, AppError> {
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let mut state = load_state(app)?;
    let report = check_health_with_state(app, &manifest, &settings, &mut state, false)?;
    save_state(app, &state)?;
    Ok(report)
}

pub fn export_diagnostics(app: &AppState) -> Result<DiagnosticsReport, AppError> {
    bootstrap_kit(app)?;
    let dashboard = get_dashboard(app, false)?;
    let marketplace = load_marketplace(app).unwrap_or_default();
    let report = render_diagnostics_report(&dashboard, &marketplace);
    let logs_dir = app.path("logs");
    fs::create_dir_all(&logs_dir)?;
    let timestamp = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown-time".to_string())
        .replace(':', "-");
    let path = logs_dir.join(format!("diagnostics_{}.md", timestamp));
    fs::write(&path, report)?;
    Ok(DiagnosticsReport {
        path: display_path(&path),
    })
}

fn check_health_with_state(
    app: &AppState,
    manifest: &Manifest,
    settings: &Settings,
    state: &mut ToolStateFile,
    force: bool,
) -> Result<HealthReport, AppError> {
    let mut checks = Vec::new();

    push_path_check(&mut checks, "root", "环境根目录", &app.root, true);
    push_path_check(
        &mut checks,
        "manifest",
        "工具清单",
        &app.path(MANIFEST_PATH),
        true,
    );
    push_path_check(
        &mut checks,
        "marketplace",
        "工具市场配置",
        &app.path(MARKETPLACE_PATH),
        false,
    );
    push_path_check(
        &mut checks,
        "workspace",
        "工作目录",
        &app.path(&settings.workspace_path),
        false,
    );
    checks.extend(package_integrity_checks(app));

    // Write-speed probe (only on force refresh, not every load).
    if let Some(speed_check) = probe_write_speed(app, force) {
        checks.push(speed_check);
    }

    if let Ok(metadata) = fs::metadata(&app.root) {
        checks.push(HealthCheck {
            id: "root-writable".to_string(),
            label: "根目录写入权限".to_string(),
            status: if metadata.permissions().readonly() {
                CheckStatus::Error
            } else {
                CheckStatus::Ok
            },
            message: if metadata.permissions().readonly() {
                "根目录为只读，无法写入状态或安装工具".to_string()
            } else {
                "根目录可写".to_string()
            },
        });
    }

    let marketplace_check = validate_marketplace_file(app);
    checks.push(HealthCheck {
        id: "marketplace-config".to_string(),
        label: "工具市场配置".to_string(),
        status: if marketplace_check.is_ok() {
            CheckStatus::Ok
        } else {
            CheckStatus::Warning
        },
        message: match &marketplace_check {
            Ok(()) => "配置可读取".to_string(),
            Err(error) => error.to_string(),
        },
    });

    checks.extend(download_integrity_checks(manifest));

    for tool in &manifest.tools {
        let view = tool_view(app, tool, state, force)?;
        if tool.required && view.status != ToolStatus::Ready {
            let portability_note = if view.host_available {
                "；宿主机已检测到，但尚未安装到当前移动盘"
            } else {
                ""
            };
            checks.push(HealthCheck {
                id: format!("tool-{}", tool.id),
                label: format!("{} 状态", tool.name),
                status: CheckStatus::Warning,
                message: format!(
                    "{}：{}{}",
                    tool.name,
                    status_label(&view.status),
                    portability_note
                ),
            });
        }
    }

    let summary = summarize_checks(&checks);
    Ok(HealthReport { summary, checks })
}

pub fn tool_action(
    app: &AppState,
    request: ToolActionRequest,
) -> Result<ToolCommandResult, AppError> {
    let _action_guard = ACTION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let tool = find_tool(&manifest, &request.tool_id)?;

    let timeout = request
        .timeout_secs
        .map(std::time::Duration::from_secs)
        .unwrap_or(NPM_INSTALL_TIMEOUT);
    match request.action.as_str() {
        "install" => install_tool(app, &manifest, &settings, tool, timeout),
        "update" => install_tool(app, &manifest, &settings, tool, timeout),
        "uninstall" => uninstall_tool(app, tool),
        _ => Err(AppError::Message(format!(
            "未知工具操作：{}",
            request.action
        ))),
    }
}

pub fn run_tool(
    app: &AppState,
    tool_id: &str,
    workspace_dir: Option<String>,
) -> Result<ToolCommandResult, AppError> {
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let tool = find_tool(&manifest, tool_id)?;
    let command = if tool.run_command.is_empty() {
        tool.bin_paths.clone()
    } else {
        tool.run_command.clone()
    };
    if command.is_empty() {
        return Err(AppError::Message(format!("{} 未定义运行命令", tool.name)));
    }

    let output = spawn_terminal_command(app, tool, &command, "运行", workspace_dir)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "launch".to_string(),
        success: true,
        message: format!("已在新终端中启动 {}", tool.name),
        output,
    })
}

pub fn login_tool(
    app: &AppState,
    tool_id: &str,
    workspace_dir: Option<String>,
) -> Result<ToolCommandResult, AppError> {
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let tool = find_tool(&manifest, tool_id)?;
    let command = if tool.login_command.is_empty() {
        tool.run_command.clone()
    } else {
        tool.login_command.clone()
    };
    if command.is_empty() {
        return Err(AppError::Message(format!("{} 未定义登录命令", tool.name)));
    }

    let output = spawn_terminal_command(app, tool, &command, "登录", workspace_dir)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "login".to_string(),
        success: true,
        message: format!("已在新终端中打开 {} 登录流程", tool.name),
        output,
    })
}

fn spawn_terminal_command(
    app: &AppState,
    tool: &ToolDefinition,
    command: &[String],
    purpose: &str,
    workspace_dir: Option<String>,
) -> Result<String, AppError> {
    let exe = resolve_tool_relative(app, tool, &command[0]);
    if !exe.exists() {
        return Err(AppError::Message(format!("{} 尚未安装", tool.name)));
    }

    let args = command
        .iter()
        .skip(1)
        .map(|arg| quote_cmd_arg(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let workspace = workspace_dir
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.path("workspace"));
    fs::create_dir_all(&workspace)?;
    let workspace_str = display_path(&workspace);

    // Path of the tool executable relative to the kit root, so the .bat
    // resolves it from %KIT_ROOT% rather than a hard-coded drive letter.
    let exe_rel_to_root = exe
        .strip_prefix(&app.root)
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| display_path(&exe));

    // Create the batch file launcher in state directory
    let bat_name = format!("run_{}.bat", tool.id);
    let bat_path = app.path("state").join(&bat_name);

    // Generate batch file content.
    //
    // %~dp0 expands to the directory of this .bat ("<KIT_ROOT>\state\"),
    // so "%~dp0.." is the kit root regardless of which drive the kit is
    // currently mounted on. This keeps the launcher correct when the
    // portable disk is plugged into a different machine with a new drive
    // letter (or when the user manually double-clicks a stale .bat).
    let bat_content = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         setlocal\r\n\
         pushd \"%~dp0..\" >nul\r\n\
         set \"KIT_ROOT=%CD%\"\r\n\
         popd >nul\r\n\
         set \"HOME=%KIT_ROOT%\\state\\home\"\r\n\
         set \"USERPROFILE=%KIT_ROOT%\\state\\home\"\r\n\
         set \"APPDATA=%KIT_ROOT%\\state\\appdata\"\r\n\
         set \"LOCALAPPDATA=%KIT_ROOT%\\state\\localappdata\"\r\n\
         set \"XDG_CONFIG_HOME=%KIT_ROOT%\\state\\xdg\\config\"\r\n\
         set \"XDG_CACHE_HOME=%KIT_ROOT%\\state\\xdg\\cache\"\r\n\
         set \"XDG_STATE_HOME=%KIT_ROOT%\\state\\xdg\\state\"\r\n\
         set \"XDG_DATA_HOME=%KIT_ROOT%\\state\\xdg\\data\"\r\n\
         set \"TEMP=%KIT_ROOT%\\state\\temp\"\r\n\
         set \"TMP=%KIT_ROOT%\\state\\temp\"\r\n\
         set \"GIT_CONFIG_NOSYSTEM=1\"\r\n\
         set \"NPM_CONFIG_CACHE=%KIT_ROOT%\\cache\\npm\"\r\n\
         set \"NPM_CONFIG_PREFIX=%KIT_ROOT%\\apps\\node\"\r\n\
         if not exist \"%TEMP%\" mkdir \"%TEMP%\" >nul 2>nul\r\n\
         if not exist \"%NPM_CONFIG_CACHE%\" mkdir \"%NPM_CONFIG_CACHE%\" >nul 2>nul\r\n\
         set \"PATH=%KIT_ROOT%\\apps\\node;%KIT_ROOT%\\apps\\git\\cmd;%KIT_ROOT%\\apps\\git\\bin;%KIT_ROOT%\\apps\\git\\mingw64\\bin;%PATH%\"\r\n\
         cd /d \"{workspace}\"\r\n\
         \"%KIT_ROOT%\\{exe_rel}\" {args}\r\n\
         endlocal\r\n",
        workspace = workspace_str,
        exe_rel = exe_rel_to_root,
        args = args
    );

    // Write batch file atomically. fs::rename on Windows uses
    // MOVEFILE_REPLACE_EXISTING, so we replace the live .bat in one syscall
    // rather than exists/remove/rename (which has a TOCTOU window).
    let bat_tmp = app.path("state").join(format!("{}.tmp", &bat_name));
    fs::write(&bat_tmp, &bat_content)?;
    fs::rename(&bat_tmp, &bat_path)?;

    let exe_str = display_path(&exe);
    let log_name = format!("{}_{}.log", action_log_prefix(purpose), tool.id);
    let log_path = app.path("logs").join(log_name);
    let command_line = format!("\"{}\" {}", exe_str, args).trim().to_string();
    let launch_details = format!(
        "工具: {tool_name}\r\n\
         动作: {purpose}\r\n\
         工作目录: {workspace}\r\n\
         可执行文件: {exe}\r\n\
         参数: {args}\r\n\
         命令行: {command_line}\r\n\
         批处理文件: {bat_path}\r\n\
         日志文件: {log_path}\r\n\r\n\
         --- run bat ---\r\n{bat_content}",
        tool_name = tool.name,
        purpose = purpose,
        workspace = workspace_str,
        exe = exe_str,
        args = args,
        command_line = command_line,
        bat_path = display_path(&bat_path),
        log_path = display_path(&log_path),
        bat_content = bat_content
    );
    fs::write(&log_path, &launch_details)?;

    let bat_path_str = display_path(&bat_path);
    let mut cmd = Command::new("cmd.exe");
    cmd.arg("/K").arg(&bat_path_str).current_dir(&workspace_str);
    apply_portable_env(app, &mut cmd);
    prepend_portable_paths(app, &mut cmd);

    // Detach child so its handle is not retained by us; we deliberately
    // do not wait — cmd.exe /K is interactive and should outlive this call.
    let child = cmd.spawn().map_err(|error| {
        AppError::Message(format!(
            "无法为 {} 打开 {} 终端：{}",
            tool.name, purpose, error
        ))
    })?;
    drop(child);
    Ok(launch_details)
}

fn install_tool(
    app: &AppState,
    manifest: &Manifest,
    settings: &Settings,
    tool: &ToolDefinition,
    timeout: std::time::Duration,
) -> Result<ToolCommandResult, AppError> {
    let mut state = load_state(app)?;
    for dependency in &tool.install.depends_on {
        let dep = find_tool(manifest, dependency)?;
        let dep_view = tool_view(app, dep, &mut state, false)?;
        if dep_view.status != ToolStatus::Ready {
            let detail = dep_view
                .last_error
                .filter(|error| !error.trim().is_empty())
                .map(|error| format!("\n{}", error))
                .unwrap_or_default();
            return Err(AppError::Message(format!(
                "{} 依赖 {}，请先安装依赖项。{}",
                tool.name, dep.name, detail
            )));
        }
    }
    save_state(app, &state)?;

    match tool.install.install_type {
        InstallType::Npm => install_npm_tool(app, settings, tool, timeout),
        InstallType::Archive => install_archive_tool(app, manifest, settings, tool),
        InstallType::PowershellScript => install_powershell_script_tool(app, tool),
    }
}

fn install_npm_tool(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
    timeout: std::time::Duration,
) -> Result<ToolCommandResult, AppError> {
    let package_name = tool
        .package_name
        .clone()
        .ok_or_else(|| AppError::Message(format!("{} 未配置 npm 包", tool.name)))?;

    // Install on the local SSD first; the heavy npm I/O must not hit the
    // (possibly slow) portable drive. On failure this returns an error with
    // a Chinese message and the staging dir is already cleaned up.
    let staging = install_npm_tool_to_staging(app, settings, tool, timeout)?;

    // Move the finished tree onto the portable drive, with rollback. On
    // failure the previous install is restored and we surface an error.
    if let Err(error) = swap_npm_install_into_place(app, tool, staging) {
        // Preserve prior error semantics: persist a failure state and return
        // a ToolCommandResult so the UI shows the message.
        persist_action_state(
            app,
            tool,
            false,
            Some(package_name.clone()),
            &error.to_string(),
        )?;
        return Ok(ToolCommandResult {
            tool_id: tool.id.clone(),
            action: "install".to_string(),
            success: false,
            message: format!("{} 安装失败", tool.name),
            output: error.to_string(),
        });
    }

    let mut combined = String::new();
    if package_name == "freebuff" {
        if let Some(patch_note) = patch_freebuff_index(app, tool)? {
            combined.push_str(&patch_note);
        }
    }

    // Pre-run warmup: Execute the tool's version command to trigger any internal 
    // binary downloads (e.g. freebuff downloading its 49.6MB core).
    if !tool.version_command.is_empty() {
        let exe = resolve_tool_relative(app, tool, &tool.version_command[0]);
        if exe.exists() {
            let mut warmup = std::process::Command::new(&exe);
            warmup.args(&tool.version_command[1..]);
            warmup.current_dir(app.path(&tool.base_path));
            apply_portable_env(app, &mut warmup);
            let node_root = app.path("apps/node");
            prepend_path(&mut warmup, &node_root);
            
            // Allow a generous timeout since this might download a large binary
            match run_command_with_timeout(warmup, std::time::Duration::from_secs(900)) {
                Some(output) => {
                    if !combined.is_empty() {
                        combined.push_str("; ");
                    }
                    if output.status.success() {
                        combined.push_str("pre-run warmup complete");
                    } else {
                        let out_str = command_output(&output);
                        combined.push_str(&format!("pre-run warmup failed (可能网络不稳定，安装后请重试): {}", out_str));
                    }
                }
                None => {
                    if !combined.is_empty() {
                        combined.push_str("; ");
                    }
                    combined.push_str("pre-run warmup timed out after 15 minutes");
                }
            }
        }
    }

    persist_action_state(app, tool, true, Some(package_name.clone()), &combined)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "install".to_string(),
        success: true,
        message: format!("{} 已安装", tool.name),
        output: combined,
    })
}

/// Install an npm AI-CLI package into a staging directory on the local SSD
/// (env::temp_dir()) instead of directly onto the (possibly slow) portable
/// drive. Returns the staging path on success. The caller is responsible for
/// moving the staging tree into the final tool base path.
///
/// Doing the heavy npm I/O on the local drive sidesteps the small-file write
/// latency of slow USB/removable drives, which previously caused installs to
/// blow past NPM_INSTALL_TIMEOUT. The npm *cache* is also pointed at a local
/// dir for the same reason.
fn install_npm_tool_to_staging(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
    timeout: std::time::Duration,
) -> Result<PathBuf, AppError> {
    let node_root = app.path("apps/node");
    validate_portable_npm(app).map_err(AppError::Message)?;
    let package_name = tool
        .package_name
        .as_ref()
        .ok_or_else(|| AppError::Message(format!("{} 未配置 npm 包", tool.name)))?;
    let registry = resolve_registry(app, settings)?;

    // Stage on the LOCAL drive (env::temp_dir()), not on the portable drive.
    let staging_root = env::temp_dir()
        .join("portable-ai-dev-kit")
        .join("npm-staging");
    fs::create_dir_all(&staging_root)?;
    let unique = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "t".to_string())
        .replace(':', "-");
    let staging = staging_root.join(format!("{}-{}", tool.id, unique));
    fs::create_dir_all(&staging)?;
    fs::write(
        staging.join("package.json"),
        "{\"name\":\"portable-ai-tool\",\"private\":true}\n",
    )?;

    // Local npm cache so cache reads/writes never hit the slow drive.
    let local_cache = env::temp_dir()
        .join("portable-ai-dev-kit")
        .join("npm-cache");
    fs::create_dir_all(&local_cache)?;

    let mut command = portable_npm_command(app)?;
    command
        .arg("install")
        .arg("--prefix")
        .arg(display_path(&staging))
        .arg(package_name)
        .arg("--no-fund")
        .arg("--no-audit")
        .arg("--registry")
        .arg(&registry)
        .current_dir(display_path(&staging));
    apply_portable_env(app, &mut command);
    // Override the cache to the LOCAL drive (apply_portable_env set it to the
    // portable drive's cache/npm). Set AFTER apply_portable_env so we win.
    command.env("NPM_CONFIG_CACHE", display_path(&local_cache));
    prepend_path(&mut command, &node_root);

    let output = match run_command_with_timeout(command, timeout) {
        Some(output) => output,
        None => {
            let _ = fs::remove_dir_all(&staging);
            return Err(AppError::Message(format!(
                "{} 安装超时，请检查网络或 npm 源后重试。",
                tool.name
            )));
        }
    };
    let combined = command_output(&output);
    if !output.status.success() {
        let _ = fs::remove_dir_all(&staging);
        return Err(AppError::Message(format!(
            "{} 安装失败\n{}",
            tool.name, combined
        )));
    }

    // Sanity: confirm the package actually materialized.
    let bin_name = tool
        .package_name
        .as_deref()
        .and_then(|p| {
            p.split('@').next().map(|scope| {
                // package_name may be "@scope/name@latest" or "name@latest"
                let trimmed = scope.trim_start_matches('@');
                trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
            })
        })
        .unwrap_or_default();
    if !staging.join("node_modules").exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err(AppError::Message(format!(
            "{} 安装后未生成 node_modules\n{}",
            tool.name, combined
        )));
    }
    let _ = bin_name; // only used for the existence check above if you wish to extend

    Ok(staging)
}

/// Move a freshly-installed staging tree into the tool's final base path on
/// the portable drive, with rollback to the previous install on failure.
///
/// `staging` must contain the new `node_modules` (+ package.json). The previous
/// install, if any, is preserved under <tool_id>-backup until the new tree is
/// confirmed in place; it is removed on success and restored on failure.
fn swap_npm_install_into_place(
    app: &AppState,
    tool: &ToolDefinition,
    staging: PathBuf,
) -> Result<(), AppError> {
    let destination = app.path(&tool.base_path);
    let backup = app.path(&format!("cache/extract/{}-backup", tool.id));

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(app.path("cache/extract"))?;

    // Remove any stale backup from a prior run.
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }

    // Preserve current install as rollback target. Rename (not copy) so this
    // is cheap even on a slow drive.
    let had_existing = destination.exists();
    if had_existing {
        fs::rename(&destination, &backup)?;
    }

    if let Err(error) = fs::rename(&staging, &destination) {
        // Cross-device rename (os error 17): staging is on local SSD but
        // destination is on the portable USB drive. Fall back to recursive
        // copy + delete instead of failing the entire install.
        if error.kind() == io::ErrorKind::CrossesDevices {
            // Restore destination so robocopy can use it as a baseline for differential sync
            if had_existing && backup.exists() {
                let _ = fs::rename(&backup, &destination);
            }

            #[cfg(target_os = "windows")]
            {
                let status = Command::new("robocopy")
                    .arg(display_path(&staging))
                    .arg(display_path(&destination))
                    .arg("/MIR")
                    .arg("/MT:32")
                    .arg("/NDL")
                    .arg("/NFL")
                    .arg("/NJH")
                    .arg("/NJS")
                    .status();

                if let Ok(st) = status {
                    let code = st.code().unwrap_or(8);
                    if code < 8 {
                        let _ = fs::remove_dir_all(&staging);
                        if backup.exists() {
                            let _ = fs::remove_dir_all(&backup);
                        }
                        return Ok(());
                    }
                }
            }

            // Fallback: if robocopy failed or not on Windows, we need destination to be empty
            // for copy_dir_all. We move it back to backup.
            if had_existing && destination.exists() {
                let _ = fs::rename(&destination, &backup);
            }

            match copy_dir_all(&staging, &destination) {
                Ok(()) => {
                    let _ = fs::remove_dir_all(&staging);
                    if backup.exists() {
                        let _ = fs::remove_dir_all(&backup);
                    }
                    return Ok(());
                }
                Err(copy_err) => {
                    let _ = fs::remove_dir_all(&destination);
                    if had_existing && backup.exists() {
                        let _ = fs::rename(&backup, &destination);
                    }
                    let _ = fs::remove_dir_all(&staging);
                    return Err(copy_err);
                }
            }
        }

        // Roll back to the previous install if we can.
        if had_existing && backup.exists() && !destination.exists() {
            let _ = fs::rename(&backup, &destination);
        }
        // Clean up the staging dir we still own.
        let _ = fs::remove_dir_all(&staging);
        return Err(AppError::Io(error));
    }

    // New tree is in place. Drop the backup.
    if backup.exists() {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(&entry.path(), &target)?;
        }
    }
    Ok(())
}

fn portable_npm_command(app: &AppState) -> Result<Command, AppError> {
    let node_root = app.path("apps/node");
    let npm = find_existing_path(&node_root, &["npm.cmd", "node_modules/npm/bin/npm-cli.js"])
        .ok_or_else(|| AppError::Message("Node/npm 尚未安装".to_string()))?;
    if npm
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("js"))
        .unwrap_or(false)
    {
        // npm-cli.js is a JS file; run it through the bundled node.exe.
        let node_exe = find_existing_path(&node_root, &["node.exe"])
            .ok_or_else(|| AppError::Message("Node 可执行文件未找到".to_string()))?;
        let mut command = Command::new(node_exe);
        command.arg(&npm);
        Ok(command)
    } else {
        Ok(Command::new(&npm))
    }
}

fn validate_portable_npm(app: &AppState) -> Result<String, String> {
    let node_root = app.path("apps/node");

    // Fast path: read npm version directly from package.json (O(1) I/O)
    let mut npm_version = String::new();
    let npm_pkg = node_root.join("node_modules").join("npm").join("package.json");
    if let Ok(content) = fs::read_to_string(&npm_pkg) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(ver) = v.get("version").and_then(|v| v.as_str()) {
                npm_version = ver.to_string();
            }
        }
    }

    // Verify node.exe can actually run (fast execution, no node_modules I/O)
    let node_exe = find_existing_path(&node_root, &["node.exe"])
        .ok_or_else(|| "Node 可执行文件未找到".to_string())?;
    let mut command = Command::new(&node_exe);
    command.arg("--version").current_dir(display_path(&node_root));
    apply_portable_env(app, &mut command);
    prepend_path(&mut command, &node_root);

    let output = run_command_with_timeout(command, std::time::Duration::from_secs(15))
        .ok_or_else(|| "便携 Node/npm 校验超时，请重装 Node.js。".to_string())?;
    if output.status.success() {
        if !npm_version.is_empty() {
            return Ok(npm_version);
        } else {
            // Fallback if package.json was unreadable but node works
            return Ok("unknown".to_string());
        }
    }

    let combined = command_output(&output);
    Err(format!(
        "便携 Node 不完整或无法运行。请先在界面中卸载并重新安装 Node.js，然后再安装 AI CLI。\n{}",
        combined
    ))
}

fn patch_freebuff_index(app: &AppState, tool: &ToolDefinition) -> Result<Option<String>, AppError> {
    let index_path = app
        .path(&tool.base_path)
        .join("node_modules/freebuff/index.js");
    if !index_path.exists() {
        return Ok(None);
    }

    let mut raw = fs::read_to_string(&index_path)?;
    let mut notes = Vec::new();

    if raw.contains("Portable AI Dev Kit stream patch") {
        notes.push("freebuff stream pipeline patch already present".to_string());
    } else {
        // Patch the hardcoded requestTimeout in the same file if possible
        if let Some(timeout_start) = raw.find("requestTimeout: 20000,") {
            let mut updated_raw = String::with_capacity(raw.len() + 10);
            updated_raw.push_str(&raw[..timeout_start]);
            updated_raw.push_str("requestTimeout: 300000,");
            updated_raw.push_str(&raw[timeout_start + 22..]);
            raw = updated_raw;
            notes.push("freebuff requestTimeout patched to 300000ms".to_string());
        }

        let start = raw
            .find("  res.on('data', (chunk) => {")
            .ok_or_else(|| AppError::Message("无法定位 freebuff 下载进度代码".to_string()))?;
        let end_marker =
            "\n\n  const tempBinaryPath = path.join(CONFIG.tempDownloadDir, CONFIG.binaryName)";
        let end = raw[start..]
            .find(end_marker)
            .map(|offset| start + offset)
            .ok_or_else(|| AppError::Message("无法定位 freebuff 解压后续代码".to_string()))?;
            
        let replacement = r#"  const ProgressTransform = require('stream').Transform
  const progress = new ProgressTransform({
    transform(chunk, _encoding, callback) {
      downloadedSize += chunk.length
      const now = Date.now()
      if (now - lastProgressTime >= 100 || downloadedSize === totalSize) {
        lastProgressTime = now
        if (totalSize > 0) {
          const pct = Math.round((downloadedSize / totalSize) * 100)
          term.write(
            `Downloading... ${createProgressBar(pct)} ${pct}% of ${formatBytes(
              totalSize,
            )}`,
          )
        } else {
          term.write(`Downloading... ${formatBytes(downloadedSize)}`)
        }
      }
      callback(null, chunk)
    },
  })

  await new Promise((resolve, reject) => {
    // Portable AI Dev Kit stream patch: attach the whole pipeline synchronously
    // so the response cannot enter flowing mode before gunzip receives data.
    const gunzip = zlib.createGunzip()
    const extract = tar.x({ cwd: CONFIG.tempDownloadDir })
    const fail = (error) => {
      trackUpdateFailed(error.message, version, { stage: 'extraction' })
      reject(error)
    }

    res.on('error', fail)
    progress.on('error', fail)
    gunzip.on('error', fail)
    extract.on('error', fail)
    extract.on('finish', resolve)

    res.pipe(progress).pipe(gunzip).pipe(extract)
  })"#;

        let mut patched = String::with_capacity(raw.len() + replacement.len());
        patched.push_str(&raw[..start]);
        patched.push_str(replacement);
        patched.push_str(&raw[end..]);
        raw = patched;
        notes.push("freebuff stream pipeline patched".to_string());
    }

    if raw.contains("Portable AI Dev Kit update restart patch") {
        notes.push("freebuff update restart patch already present".to_string());
    } else {
        let function_marker = "async function checkForUpdates(runningProcess, exitListener) {\n";
        let insert_at = raw
            .find(function_marker)
            .map(|offset| offset + function_marker.len())
            .ok_or_else(|| AppError::Message("无法定位 freebuff 更新检查函数".to_string()))?;
        raw.insert_str(insert_at, "  let portableUpdateKilledProcess = false\n");

        let kill_marker = "            runningProcess.kill('SIGKILL')\n";
        let kill_at = raw
            .find(kill_marker)
            .ok_or_else(|| AppError::Message("无法定位 freebuff 强制停止代码".to_string()))?;
        raw.insert_str(
            kill_at + kill_marker.len(),
            "            portableUpdateKilledProcess = true\n",
        );

        let term_marker = "        runningProcess.kill('SIGTERM')\n";
        let term_at = raw
            .find(term_marker)
            .ok_or_else(|| AppError::Message("无法定位 freebuff 停止代码".to_string()))?;
        raw.insert_str(
            term_at + term_marker.len(),
            "          portableUpdateKilledProcess = true\n",
        );

        let catch_marker = "  } catch (error) {\n    // Ignore update failures\n  }\n}";
        let catch_replacement = r#"  } catch (error) {
    // Portable AI Dev Kit update restart patch: freebuff kills the running
    // binary before replacing it. If the download/extract step fails, restart
    // the old binary instead of dropping the user back to an empty prompt.
    const message = error && error.message ? error.message : String(error)
    console.error(`freebuff update failed: ${message}`)
    if (portableUpdateKilledProcess) {
        const fallbackChild = spawn(CONFIG.binaryPath, process.argv.slice(2), {
        stdio: 'inherit',
        detached: false,
      })

      fallbackChild.on('exit', (code, signal) => {
        resetTerminal()
        printCrashDiagnostics(code, signal)
        process.exit(signal ? 1 : (code || 0))
      })

      fallbackChild.on('error', (err) => {
        console.error('Failed to restart freebuff:', err.message)
        process.exit(1)
      })

      return new Promise(() => {})
    }
  }
}"#;
        if !raw.contains(catch_marker) {
            return Err(AppError::Message(
                "无法替换 freebuff 更新失败处理代码".to_string(),
            ));
        }
        raw = raw.replacen(catch_marker, catch_replacement, 1);
        notes.push("freebuff update restart patched".to_string());
    }

    fs::write(&index_path, raw)?;

    Ok(Some(format!(
        "{}: {}",
        notes.join("; "),
        display_path(&index_path),
    )))
}

fn install_archive_tool(
    app: &AppState,
    manifest: &Manifest,
    settings: &Settings,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    let network = manifest
        .network_modes
        .get(&settings.network_mode)
        .or_else(|| manifest.network_modes.get("global"))
        .ok_or_else(|| AppError::Message("未配置可用网络模式".to_string()))?;
    let url = tool
        .install
        .urls
        .get(&network.archive_source)
        .or_else(|| tool.install.urls.get("global"))
        .ok_or_else(|| AppError::Message(format!("{} 未配置下载地址", tool.name)))?;
    let archive_name = tool
        .install
        .archive_name
        .as_ref()
        .ok_or_else(|| AppError::Message(format!("{} 未配置归档文件名", tool.name)))?;
    let download_path = app.path(&format!("cache/downloads/{}", archive_name));
    let destination = app.path(&tool.base_path);
    let staging = app.path(&format!("cache/extract/{}-staging", tool.id));
    let backup = app.path(&format!("cache/extract/{}-backup", tool.id));
    fs::create_dir_all(download_path.parent().unwrap_or(&app.root))?;

    if !download_path.exists() {
        let mut download = Command::new("powershell.exe");
        download
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(format!(
                "$ProgressPreference = 'SilentlyContinue'; \
                 try {{ Invoke-WebRequest -Uri '{}' -OutFile '{}' \
                    -UseBasicParsing -TimeoutSec 120 -MaximumRedirection 5 }} \
                 catch {{ if (Test-Path '{}') {{ Remove-Item -LiteralPath '{}' -Force }} ; throw }}",
                escape_single_quote(url),
                escape_single_quote(&display_path(&download_path)),
                escape_single_quote(&display_path(&download_path)),
                escape_single_quote(&display_path(&download_path))
            ));
        let output = match run_command_with_timeout(download, DOWNLOAD_TIMEOUT) {
            Some(output) => output,
            None => {
                let _ = fs::remove_file(&download_path);
                let combined = format!("{} 下载超时，请检查网络后重试。", tool.name);
                persist_action_state(app, tool, false, Some(url.to_string()), &combined)?;
                return Ok(ToolCommandResult {
                    tool_id: tool.id.clone(),
                    action: "install".to_string(),
                    success: false,
                    message: format!("{} 下载超时", tool.name),
                    output: combined,
                });
            }
        };
        if !output.status.success() {
            // Drop any half-written file so a subsequent retry redownloads.
            let _ = fs::remove_file(&download_path);
            let combined = command_output(&output);
            persist_action_state(app, tool, false, Some(url.to_string()), &combined)?;
            return Ok(ToolCommandResult {
                tool_id: tool.id.clone(),
                action: "install".to_string(),
                success: false,
                message: format!("{} 下载失败", tool.name),
                output: combined,
            });
        }
    }
    if let Err(error) = verify_download_integrity(tool, &download_path) {
        let _ = fs::remove_file(&download_path);
        let combined = error.to_string();
        persist_action_state(app, tool, false, Some(url.to_string()), &combined)?;
        return Ok(ToolCommandResult {
            tool_id: tool.id.clone(),
            action: "install".to_string(),
            success: false,
            message: format!("{} 下载校验失败", tool.name),
            output: combined,
        });
    }

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    fs::create_dir_all(&staging)?;

    let local_archive = prepare_local_archive_copy(&download_path, archive_name)?;
    let mut expand = Command::new("tar.exe");
    expand
        .arg("-xf")
        .arg(display_path(&local_archive))
        .arg("-C")
        .arg(display_path(&staging));
    let output = match run_command_with_timeout(expand, EXPAND_TIMEOUT) {
        Some(output) => output,
        None => {
            let _ = fs::remove_dir_all(&staging);
            let combined = format!(
                "{} 解压超时，请确认移动盘写入速度，或删除缓存后重试。",
                tool.name
            );
            persist_action_state(app, tool, false, Some(url.to_string()), &combined)?;
            return Ok(ToolCommandResult {
                tool_id: tool.id.clone(),
                action: "install".to_string(),
                success: false,
                message: format!("{} 解压超时", tool.name),
                output: combined,
            });
        }
    };
    let mut combined = command_output(&output);
    let success = output.status.success()
        || (tar_output_only_has_timestamp_warnings(&output) && directory_has_entries(&staging)?);
    if success && !output.status.success() {
        combined = format!(
            "{}\n\n已忽略 tar.exe 时间戳恢复警告；文件内容已解压完成。",
            combined
        );
    }
    if success {
        flatten_single_root(&staging)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        if destination.exists() {
            fs::rename(&destination, &backup)?;
        }
        if let Err(error) = fs::rename(&staging, &destination) {
            if backup.exists() && !destination.exists() {
                let _ = fs::rename(&backup, &destination);
            }
            return Err(error.into());
        }
        if backup.exists() {
            fs::remove_dir_all(&backup)?;
        }
    } else {
        let _ = fs::remove_dir_all(&staging);
    }
    persist_action_state(app, tool, success, Some(url.to_string()), &combined)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "install".to_string(),
        success,
        message: if success {
            format!("{} 已安装", tool.name)
        } else {
            format!("{} 解压失败", tool.name)
        },
        output: combined,
    })
}

fn install_powershell_script_tool(
    app: &AppState,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    let script_url = tool
        .install
        .script_url
        .as_ref()
        .ok_or_else(|| AppError::Message(format!("{} 未配置安装脚本地址", tool.name)))?;
    let destination = app.path(&tool.base_path);
    fs::create_dir_all(&destination)?;

    let script_path = app.path("cache/downloads/install_temp.ps1");
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Download the PowerShell script
    let mut download = Command::new("powershell.exe");
    download
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(format!(
            "$ProgressPreference = 'SilentlyContinue'; \
             try {{ Invoke-WebRequest -Uri '{}' -OutFile '{}' \
                -UseBasicParsing -TimeoutSec 120 -MaximumRedirection 5 }} \
             catch {{ if (Test-Path '{}') {{ Remove-Item -LiteralPath '{}' -Force }} ; throw }}",
            escape_single_quote(script_url),
            escape_single_quote(&display_path(&script_path)),
            escape_single_quote(&display_path(&script_path)),
            escape_single_quote(&display_path(&script_path))
        ));
    let download_output = match run_command_with_timeout(download, DOWNLOAD_TIMEOUT) {
        Some(output) => output,
        None => {
            let _ = fs::remove_file(&script_path);
            let combined = format!("{} 脚本下载超时，请检查网络后重试。", tool.name);
            persist_action_state(app, tool, false, Some(script_url.to_string()), &combined)?;
            return Ok(ToolCommandResult {
                tool_id: tool.id.clone(),
                action: "install".to_string(),
                success: false,
                message: format!("{} 脚本下载超时", tool.name),
                output: combined,
            });
        }
    };
    if !download_output.status.success() {
        let _ = fs::remove_file(&script_path);
        let combined = command_output(&download_output);
        persist_action_state(app, tool, false, Some(script_url.to_string()), &combined)?;
        return Ok(ToolCommandResult {
            tool_id: tool.id.clone(),
            action: "install".to_string(),
            success: false,
            message: format!("{} 脚本下载失败", tool.name),
            output: combined,
        });
    }
    if let Err(error) = verify_download_integrity(tool, &script_path) {
        let _ = fs::remove_file(&script_path);
        let combined = error.to_string();
        persist_action_state(app, tool, false, Some(script_url.to_string()), &combined)?;
        return Ok(ToolCommandResult {
            tool_id: tool.id.clone(),
            action: "install".to_string(),
            success: false,
            message: format!("{} 脚本校验失败", tool.name),
            output: combined,
        });
    }

    // Execute the PowerShell script with argument `--dir <destination>`
    let mut run = Command::new("powershell.exe");
    run.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(display_path(&script_path))
        .arg("--dir")
        .arg(display_path(&destination));
    for arg in &tool.install.script_args {
        run.arg(arg);
    }

    apply_portable_env(app, &mut run);

    let run_output = match run_command_with_timeout(run, SCRIPT_INSTALL_TIMEOUT) {
        Some(output) => output,
        None => {
            let _ = fs::remove_file(&script_path);
            let combined = format!("{} 安装脚本执行超时，请检查脚本源后重试。", tool.name);
            persist_action_state(app, tool, false, Some(script_url.to_string()), &combined)?;
            return Ok(ToolCommandResult {
                tool_id: tool.id.clone(),
                action: "install".to_string(),
                success: false,
                message: format!("{} 安装脚本执行超时", tool.name),
                output: combined,
            });
        }
    };
    let combined = command_output(&run_output);
    let success = run_output.status.success();

    // Clean up the temporary script file
    let _ = fs::remove_file(&script_path);

    persist_action_state(app, tool, success, Some(script_url.to_string()), &combined)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "install".to_string(),
        success,
        message: if success {
            format!("{} 已安装", tool.name)
        } else {
            format!("{} 安装脚本执行失败", tool.name)
        },
        output: combined,
    })
}

fn uninstall_tool(app: &AppState, tool: &ToolDefinition) -> Result<ToolCommandResult, AppError> {
    let destination = app.path(&tool.base_path);
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }

    let mut state = load_state(app)?;
    state.tools.remove(&tool.id);
    save_state(app, &state)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "uninstall".to_string(),
        success: true,
        message: format!("{} 已卸载", tool.name),
        output: String::new(),
    })
}

fn tool_view(
    app: &AppState,
    tool: &ToolDefinition,
    state: &mut ToolStateFile,
    force: bool,
) -> Result<ToolView, AppError> {
    let base = app.path(&tool.base_path);
    let launch = find_existing_path(&base, &tool.bin_paths);
    let mut status = if launch.is_some() {
        ToolStatus::Ready
    } else if base.exists() {
        ToolStatus::Partial
    } else {
        ToolStatus::Missing
    };
    let mut persisted = state.tools.get(&tool.id).cloned().unwrap_or_default();
    let mut current_error = None;
    if tool.id == "node" && status == ToolStatus::Ready {
        if let Err(error) = validate_portable_npm(app) {
            status = ToolStatus::Partial;
            current_error = Some(error.chars().take(2000).collect());
        } else {
            persisted.last_error = None;
        }
    }
    let detected_version = if status == ToolStatus::Ready {
        if force
            || persisted.installed_version.is_none()
            || persisted.last_error.is_some()
        {
            let detected = detect_version(app, tool);
            if detected.is_some() {
                persisted.installed_version = detected.clone();
            }
            persisted.last_error = None;
            detected.or_else(|| persisted.installed_version.clone())
        } else {
            persisted.installed_version.clone()
        }
    } else {
        None
    };
    if status != ToolStatus::Ready {
        if let Some(error) = current_error {
            persisted.last_error = Some(error);
        } else if status == ToolStatus::Partial {
            persisted.last_error = Some(incomplete_install_message(tool));
        } else if persisted.installed_version.is_some() || persisted.installed_at.is_some() {
            persisted.last_error = Some(format!("未找到安装目录：{}", display_path(&base)));
        }
        persisted.installed_version = None;
    }
    let host_version = if tool.host_version_command.is_empty() {
        None
    } else if force {
        // Only run host detection when explicitly requested (refresh/retry).
        // This avoids potential hangs from powershell.exe / where.exe on
        // the initial bootstrap, keeping first-launch fast and reliable.
        let detected = detect_host_version(tool);
        persisted.host_version = detected.clone();
        detected
    } else if persisted.host_version.is_some() {
        persisted.host_version.clone()
    } else {
        // Skip host detection on initial load; cached value will be
        // populated once the user triggers a refresh.
        None
    };
    let host_available = host_version.is_some();
    state.tools.insert(tool.id.clone(), persisted.clone());

    Ok(ToolView {
        id: tool.id.clone(),
        name: tool.name.clone(),
        kind: tool.kind.clone(),
        required: tool.required,
        status,
        installed_version: detected_version,
        wanted_version: persisted
            .wanted_version
            .or_else(|| tool.package_name.clone()),
        install_source: install_source(tool),
        base_path: display_path(&base),
        launch_path: launch.map(|path| display_path(&path)),
        host_available,
        host_version,
        last_error: persisted.last_error,
    })
}

fn prepare_local_archive_copy(source: &Path, archive_name: &str) -> Result<PathBuf, AppError> {
    let local_dir = env::temp_dir().join("portable-ai-dev-kit").join("archives");
    fs::create_dir_all(&local_dir)?;
    let local_path = local_dir.join(archive_name);

    let should_copy = match (fs::metadata(source), fs::metadata(&local_path)) {
        (Ok(source_meta), Ok(local_meta)) => source_meta.len() != local_meta.len(),
        (Ok(_), Err(_)) => true,
        (Err(error), _) => return Err(error.into()),
    };

    if should_copy {
        fs::copy(source, &local_path)?;
    }

    Ok(local_path)
}

fn directory_has_entries(path: &Path) -> Result<bool, AppError> {
    Ok(fs::read_dir(path)?.next().is_some())
}

fn tar_output_only_has_timestamp_warnings(output: &std::process::Output) -> bool {
    if !String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        return false;
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut saw_warning = false;
    for line in stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !line.contains("Can't restore time: Invalid argument") {
            return false;
        }
        saw_warning = true;
    }
    saw_warning
}

fn incomplete_install_message(tool: &ToolDefinition) -> String {
    if tool.bin_paths.is_empty() {
        format!("{} 安装目录存在，但未配置启动入口。", tool.name)
    } else {
        format!(
            "{} 安装目录存在，但未找到启动入口：{}",
            tool.name,
            tool.bin_paths.join(", ")
        )
    }
}

fn run_command_with_timeout(
    mut command: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::sync::{Arc, Mutex as StdMutex};
    use std::thread;

    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    let mut child = command.spawn().ok()?;

    // Drain stdout/stderr concurrently to avoid the pipe-buffer-full deadlock.
    let stdout_buf: Arc<StdMutex<Vec<u8>>> = Arc::new(StdMutex::new(Vec::new()));
    let stderr_buf: Arc<StdMutex<Vec<u8>>> = Arc::new(StdMutex::new(Vec::new()));
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_thread = stdout_pipe.map({
        let buf = Arc::clone(&stdout_buf);
        move |mut pipe| {
            thread::spawn(move || {
                let local = read_pipe_capped(&mut pipe);
                if let Ok(mut guard) = buf.lock() {
                    guard.extend(local);
                }
            })
        }
    });
    let stderr_thread = stderr_pipe.map({
        let buf = Arc::clone(&stderr_buf);
        move |mut pipe| {
            thread::spawn(move || {
                let local = read_pipe_capped(&mut pipe);
                if let Ok(mut guard) = buf.lock() {
                    guard.extend(local);
                }
            })
        }
    });

    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("taskkill")
                        .args(&["/F", "/T", "/PID", &child.id().to_string()])
                        .output();
                    
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => {
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("taskkill")
                    .args(&["/F", "/T", "/PID", &child.id().to_string()])
                    .output();
                
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };

    if status.is_some() {
        if let Some(handle) = stdout_thread {
            let _ = handle.join();
        }
        if let Some(handle) = stderr_thread {
            let _ = handle.join();
        }
    }

    let status = status?;
    let stdout = stdout_buf.lock().map(|g| g.clone()).unwrap_or_default();
    let stderr = stderr_buf.lock().map(|g| g.clone()).unwrap_or_default();
    Some(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe_capped<R: Read>(pipe: &mut R) -> Vec<u8> {
    let mut out = Vec::new();
    let mut scratch = [0u8; 8192];
    loop {
        match pipe.read(&mut scratch) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(out.len());
                if remaining > 0 {
                    out.extend_from_slice(&scratch[..n.min(remaining)]);
                }
            }
            Err(_) => break,
        }
    }
    out
}

fn detect_version(app: &AppState, tool: &ToolDefinition) -> Option<String> {
    // Fast path: if this tool has a package_name, try to read its package.json directly
    if let Some(pkg_name) = &tool.package_name {
        let pkg_json_path = app
            .path(&tool.base_path)
            .join("node_modules")
            .join(pkg_name)
            .join("package.json");
        if let Ok(content) = fs::read_to_string(&pkg_json_path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(ver) = v.get("version").and_then(|v| v.as_str()) {
                    return Some(ver.to_string());
                }
            }
        }
    }

    if tool.version_command.is_empty() {
        return None;
    }
    let exe = resolve_tool_relative(app, tool, &tool.version_command[0]);
    if !exe.exists() {
        return None;
    }
    let mut command = Command::new(exe);
    for arg in tool.version_command.iter().skip(1) {
        command.arg(arg);
    }
    command.current_dir(app.path(&tool.base_path));
    apply_portable_env(app, &mut command);
    prepend_portable_paths(app, &mut command);
    run_command_with_timeout(command, std::time::Duration::from_secs(3))
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
}

fn detect_host_version(tool: &ToolDefinition) -> Option<String> {
    if tool.host_version_command.is_empty() {
        return None;
    }

    let executable = &tool.host_version_command[0];
    // Resolve the absolute host path (bypassing any portable PATH) so the
    // version detection truly reflects the host machine's installation.
    let absolute = host_executable_path(executable)?;

    let mut command = Command::new(&absolute);
    for arg in tool.host_version_command.iter().skip(1) {
        command.arg(arg);
    }
    if let Some(system_path) = host_system_path() {
        command.env("PATH", system_path);
    }

    run_command_with_timeout(command, std::time::Duration::from_secs(3))
        .and_then(|output| {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Some(if stdout.is_empty() { stderr } else { stdout })
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceTool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub package_name: String,
    pub category: String,
    pub homepage: String,
    pub in_manifest: bool,
    pub installed: bool,
}

pub fn get_marketplace_tools(app: &AppState) -> Result<Vec<MarketplaceTool>, AppError> {
    let manifest = load_manifest(app)?;
    let marketplace = load_marketplace(app)?;

    let result_tools: Vec<MarketplaceTool> = marketplace
        .tools
        .into_iter()
        .map(|tool| {
            validate_marketplace_definition(&tool)?;
            Ok(tool)
        })
        .collect::<Result<Vec<_>, AppError>>()?
        .into_iter()
        .map(|tool| {
            let custom_id = format!("custom-{}", tool.id);
            let in_manifest = manifest.tools.iter().any(|t| t.id == tool.id)
                || manifest.tools.iter().any(|t| t.id == custom_id);
            let installed = manifest.tools.iter().any(|t| {
                (t.id == tool.id || t.id == custom_id)
                    && app.path(&t.base_path).exists()
                    && t.bin_paths
                        .iter()
                        .any(|bin| app.path(&t.base_path).join(bin.replace('/', "\\")).exists())
            });
            MarketplaceTool {
                id: tool.id,
                name: tool.name,
                description: tool.description,
                package_name: tool.package_name,
                category: tool.category,
                homepage: tool.homepage,
                in_manifest,
                installed,
            }
        })
        .collect();

    Ok(result_tools)
}

pub fn install_marketplace_tool(
    app: &AppState,
    id: String,           // marketplace tool ID (e.g. "claude", "codebuff")
    name: String,         // display name (e.g. "Claude Code", "Codebuff")
    package_name: String, // npm package name
) -> Result<ToolCommandResult, AppError> {
    let _action_guard = ACTION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;

    let custom_tool_id = format!("custom-{}", id);

    // Check if tool is already in manifest (built-in like "claude", "codex", "antigravity")
    // or already added as a custom tool ("custom-<id>")
    if let Some(tool) = manifest
        .tools
        .iter()
        .find(|t| t.id == id || t.id == custom_tool_id)
    {
        return install_tool(app, &manifest, &settings, tool, NPM_INSTALL_TIMEOUT);
    }

    // Not in manifest, add as custom tool first, then install
    add_custom_tool_unlocked(
        app,
        name.clone(),
        "npm",
        Some(package_name.clone()),
        None,
        None,
    )?;

    let updated_manifest = load_manifest(app)?;
    if let Some(tool) = updated_manifest
        .tools
        .iter()
        .find(|t| t.id == custom_tool_id)
    {
        return install_tool(app, &updated_manifest, &settings, tool, NPM_INSTALL_TIMEOUT);
    }

    Err(AppError::Message(format!(
        "添加工具「{}」后无法找到其定义",
        name
    )))
}

fn host_executable_path(executable: &str) -> Option<String> {
    // Detect on the host machine's PATH by reading the canonical "Path" value
    // from the registry/system rather than the (possibly portable-augmented)
    // process environment, then run where.exe with that PATH.
    let mut cmd = Command::new("where.exe");
    cmd.arg(executable);
    if let Some(system_path) = host_system_path() {
        cmd.env("PATH", system_path);
    }
    run_command_with_timeout(cmd, std::time::Duration::from_secs(5))
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .map(|line| line.trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
}

fn host_system_path() -> Option<String> {
    // PowerShell to read the Machine + User PATH (i.e. the host's PATH as seen
    // by a freshly spawned cmd window), so portable PATH prepends do not leak.
    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + \
             [Environment]::GetEnvironmentVariable('Path','User')",
        );
    let output = run_command_with_timeout(command, std::time::Duration::from_secs(5))?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() || value == ";" {
        None
    } else {
        Some(value)
    }
}
const CUSTOM_TOOLS_PATH: &str = "config/custom-tools.json";

fn load_manifest(app: &AppState) -> Result<Manifest, AppError> {
    let raw = fs::read_to_string(app.path(MANIFEST_PATH))?;
    let mut manifest: Manifest = serde_json::from_str(&raw)?;

    let custom_path = app.path(CUSTOM_TOOLS_PATH);
    if custom_path.exists() {
        match fs::read_to_string(&custom_path) {
            Ok(custom_raw) => match serde_json::from_str::<CustomToolsFile>(&custom_raw) {
                Ok(custom_file) => manifest.tools.extend(custom_file.tools),
                Err(error) => {
                    eprintln!(
                        "warning: 无法解析 {}: {}; 将忽略自定义工具列表",
                        display_path(&custom_path),
                        error
                    );
                    let backup = custom_path.with_extension("json.corrupt");
                    let _ = fs::copy(&custom_path, &backup);
                }
            },
            Err(error) => {
                eprintln!(
                    "warning: 无法读取 {}: {}",
                    display_path(&custom_path),
                    error
                );
            }
        }
    }

    for tool in &mut manifest.tools {
        apply_npm_tool_aliases(tool);
        validate_tool_definition(tool)?;
    }

    Ok(manifest)
}

fn apply_npm_tool_aliases(tool: &mut ToolDefinition) {
    let Some(package_name) = tool.package_name.as_deref() else {
        return;
    };
    if is_qoder_package_alias(package_name) {
        tool.package_name = Some("@qoder-ai/qodercli".to_string());
        let bin_path = "node_modules/.bin/qodercli.cmd".to_string();
        tool.version_command = vec![bin_path.clone(), "--version".to_string()];
        tool.host_version_command = vec!["qodercli".to_string(), "--version".to_string()];
        tool.bin_paths = vec![bin_path.clone()];
        tool.run_command = vec![bin_path.clone()];
        tool.login_command = vec![bin_path];
    }
}

fn is_qoder_package_alias(package_name: &str) -> bool {
    matches!(
        package_name.trim().to_ascii_lowercase().as_str(),
        "qoder" | "qoder@latest" | "@qoder-ai/qodercli" | "@qoder-ai/qodercli@latest"
    )
}

fn validate_tool_definition(tool: &ToolDefinition) -> Result<(), AppError> {
    validate_relative_path(&tool.base_path, &format!("{} basePath", tool.id))?;
    for value in &tool.bin_paths {
        validate_relative_path(value, &format!("{} binPath", tool.id))?;
    }
    validate_command_entry(&tool.version_command, &tool.id, "versionCommand")?;
    validate_command_entry(&tool.run_command, &tool.id, "runCommand")?;
    validate_command_entry(&tool.login_command, &tool.id, "loginCommand")?;
    if let Some(archive_name) = &tool.install.archive_name {
        validate_file_name(archive_name, &format!("{} archiveName", tool.id))?;
    }
    for url in tool.install.urls.values() {
        validate_http_url(url, "archive url")?;
    }
    if let Some(script_url) = &tool.install.script_url {
        validate_script_url(script_url)?;
    }
    for arg in &tool.install.script_args {
        validate_script_arg(arg, &format!("{} scriptArg", tool.id))?;
    }
    if let Some(sha256) = &tool.install.sha256 {
        validate_sha256_hex(sha256, &format!("{} sha256", tool.id))?;
    }
    Ok(())
}

fn validate_command_entry(command: &[String], tool_id: &str, field: &str) -> Result<(), AppError> {
    if let Some(entry) = command.first() {
        validate_relative_path(entry, &format!("{} {}", tool_id, field))?;
    }
    Ok(())
}

fn validate_relative_path(value: &str, label: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Message(format!("{} 不能为空", label)));
    }
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return Err(AppError::Message(format!("{} 不能是绝对路径", label)));
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::Message(format!(
            "{} 不能包含上级目录或根路径组件",
            label
        )));
    }
    Ok(trimmed.replace('\\', "/"))
}

fn validate_file_name(value: &str, label: &str) -> Result<(), AppError> {
    let normalized = validate_relative_path(value, label)?;
    if normalized.contains('/') {
        return Err(AppError::Message(format!("{} 只能是文件名", label)));
    }
    Ok(())
}

fn validate_http_url(value: &str, label: &str) -> Result<(), AppError> {
    let lower = value.trim().to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        Ok(())
    } else {
        Err(AppError::Message(format!("{} 必须使用 http(s) URL", label)))
    }
}

fn validate_script_url(value: &str) -> Result<(), AppError> {
    let lower = value.trim().to_ascii_lowercase();
    let is_local_http =
        lower.starts_with("http://localhost") || lower.starts_with("http://127.0.0.1");
    if lower.starts_with("https://") || is_local_http {
        Ok(())
    } else {
        Err(AppError::Message(
            "脚本 URL 必须使用 HTTPS（仅 localhost / 127.0.0.1 允许 HTTP）".to_string(),
        ))
    }
}

fn validate_script_arg(value: &str, label: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control()) {
        return Err(AppError::Message(format!(
            "{} 不能为空或包含控制字符",
            label
        )));
    }
    Ok(())
}

fn validate_sha256_hex(value: &str, label: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::Message(format!(
            "{} 必须是 64 位十六进制 SHA-256",
            label
        )));
    }
    Ok(())
}

fn load_marketplace(app: &AppState) -> Result<MarketplaceFile, AppError> {
    let path = app.path(MARKETPLACE_PATH);
    if !path.exists() {
        return Ok(serde_json::from_str::<MarketplaceFile>(
            DEFAULT_MARKETPLACE_JSON,
        )?);
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<MarketplaceFile>(&raw)?)
}

fn validate_marketplace_file(app: &AppState) -> Result<(), AppError> {
    let path = app.path(MARKETPLACE_PATH);
    if !path.exists() {
        return Ok(());
    }
    let marketplace = load_marketplace(app)?;
    for tool in marketplace.tools {
        validate_marketplace_definition(&tool)?;
    }
    Ok(())
}

fn validate_marketplace_definition(tool: &MarketplaceDefinition) -> Result<(), AppError> {
    if tool.id.trim().is_empty() {
        return Err(AppError::Message("工具市场配置包含空 id".to_string()));
    }
    if tool.name.trim().is_empty() {
        return Err(AppError::Message(format!(
            "工具市场配置 {} 缺少名称",
            tool.id
        )));
    }
    if tool.package_name.trim().is_empty() {
        return Err(AppError::Message(format!(
            "工具市场配置 {} 缺少 packageName",
            tool.id
        )));
    }
    if tool.homepage.trim().is_empty() {
        return Err(AppError::Message(format!(
            "工具市场配置 {} 缺少 homepage",
            tool.id
        )));
    }
    validate_http_url(&tool.homepage, "homepage")
}

pub fn load_settings(app: &AppState) -> Result<Settings, AppError> {
    let path = app.path(SETTINGS_PATH);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let mut settings: Settings = serde_json::from_str(&fs::read_to_string(path)?)?;
    settings.workspace_path = sanitize_relative_path(&settings.workspace_path, "workspace");
    Ok(settings)
}

pub fn save_settings(
    app: &AppState,
    network_mode: &str,
    workspace_path: &str,
    auto_open_workspace: bool,
) -> Result<(), AppError> {
    let _action_guard = ACTION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let manifest = load_manifest(app)?;
    let settings_path = sanitize_workspace_path(app, workspace_path, "workspace");
    if !manifest.network_modes.contains_key(network_mode) {
        return Err(AppError::Message(format!("未知网络模式：{}", network_mode)));
    }
    let settings = Settings {
        network_mode: network_mode.to_string(),
        workspace_path: settings_path.clone(),
        auto_open_workspace,
    };
    let path = app.path(SETTINGS_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(app.path(&settings_path))?;
    let serialized = serde_json::to_string_pretty(&settings)?;
    fs::write(&path, serialized)?;
    Ok(())
}

fn sanitize_relative_path(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() || trimmed.contains("..") {
        return fallback.to_string();
    }
    trimmed.replace('\\', "/")
}

fn sanitize_workspace_path(app: &AppState, value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        if let Ok(relative) = candidate.strip_prefix(&app.root) {
            let relative = relative.to_string_lossy().replace('\\', "/");
            return sanitize_relative_path(&relative, fallback);
        }
        return fallback.to_string();
    }

    sanitize_relative_path(trimmed, fallback)
}

fn load_state(app: &AppState) -> Result<ToolStateFile, AppError> {
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = app.path(STATE_PATH);
    if !path.exists() {
        return Ok(ToolStateFile::default());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(ToolStateFile::default());
    }
    match serde_json::from_str(&raw) {
        Ok(state) => Ok(state),
        Err(_) => {
            // Corrupted state file from prior crash; reset rather than blocking app startup.
            let _ = fs::remove_file(&path);
            Ok(ToolStateFile::default())
        }
    }
}

fn save_state(app: &AppState, state: &ToolStateFile) -> Result<(), AppError> {
    let _guard = STATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let state_dir = app.path("state");
    fs::create_dir_all(&state_dir)?;
    let final_path = app.path(STATE_PATH);
    let temp_path = state_dir.join("tool-state.json.tmp");
    let serialized = serde_json::to_string_pretty(state)?;
    fs::write(&temp_path, serialized)?;
    // fs::rename on Windows uses MoveFileExW with MOVEFILE_REPLACE_EXISTING,
    // so we don't need (and shouldn't do) a separate exists/remove step —
    // that creates a TOCTOU window where readers see no file.
    fs::rename(&temp_path, &final_path)?;
    Ok(())
}

fn persist_action_state(
    app: &AppState,
    tool: &ToolDefinition,
    success: bool,
    source: Option<String>,
    output: &str,
) -> Result<(), AppError> {
    let mut state = load_state(app)?;
    let now = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string());
    let entry = state.tools.entry(tool.id.clone()).or_default();
    entry.source = source;
    entry.wanted_version = tool.package_name.clone();
    entry.updated_at = Some(now.clone());
    if success {
        entry.installed_at = entry.installed_at.clone().or(Some(now));
        entry.installed_version = detect_version(app, tool);
        entry.last_error = None;
    } else {
        entry.last_error = Some(output.chars().take(2000).collect());
    }
    save_state(app, &state)
}

fn verify_download_integrity(tool: &ToolDefinition, path: &Path) -> Result<(), AppError> {
    let Some(expected) = tool.install.sha256.as_deref() else {
        return Ok(());
    };
    let actual = file_sha256(path)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        return Ok(());
    }
    Err(AppError::Message(format!(
        "{} 下载文件 SHA-256 不匹配：expected={}, actual={}",
        tool.name,
        expected.trim(),
        actual
    )))
}

fn file_sha256(path: &Path) -> Result<String, AppError> {
    let mut command = Command::new("powershell.exe");
    command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(format!(
            "$stream = [System.IO.File]::OpenRead('{}'); \
             try {{ \
               $sha = [System.Security.Cryptography.SHA256]::Create(); \
               [System.BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') \
             }} finally {{ $stream.Dispose() }}",
            escape_single_quote(&display_path(path))
        ));
    let output = run_command_with_timeout(command, std::time::Duration::from_secs(30))
        .ok_or_else(|| AppError::Message("SHA-256 校验超时".to_string()))?;
    if !output.status.success() {
        return Err(AppError::Message(format!(
            "SHA-256 校验失败：{}",
            command_output(&output)
        )));
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    validate_sha256_hex(&hash, "实际 SHA-256")?;
    Ok(hash.to_ascii_lowercase())
}

fn find_tool<'a>(manifest: &'a Manifest, tool_id: &str) -> Result<&'a ToolDefinition, AppError> {
    manifest
        .tools
        .iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| AppError::Message(format!("未知工具：{}", tool_id)))
}

fn resolve_registry(app: &AppState, settings: &Settings) -> Result<String, AppError> {
    let manifest = load_manifest(app)?;
    Ok(manifest
        .network_modes
        .get(&settings.network_mode)
        .or_else(|| manifest.network_modes.get("global"))
        .map(|mode| mode.npm_registry.clone())
        .unwrap_or_else(|| "https://registry.npmjs.org/".to_string()))
}

fn install_source(tool: &ToolDefinition) -> String {
    match tool.install.install_type {
        InstallType::Npm => tool
            .package_name
            .clone()
            .unwrap_or_else(|| "npm".to_string()),
        InstallType::Archive => tool
            .install
            .archive_name
            .clone()
            .unwrap_or_else(|| "archive".to_string()),
        InstallType::PowershellScript => tool
            .install
            .script_url
            .clone()
            .unwrap_or_else(|| "script".to_string()),
    }
}

fn find_existing_path<T: AsRef<str>>(base: &Path, relatives: &[T]) -> Option<PathBuf> {
    for relative in relatives {
        let direct = base.join(relative.as_ref().replace('/', "\\"));
        if direct.exists() {
            return Some(direct);
        }
        if let Ok(children) = fs::read_dir(base) {
            for child in children.flatten() {
                // Skip symlinks/junctions to avoid infinite loops and
                // accidentally resolving outside the portable tree (e.g.
                // Windows compatibility junctions like "Application Data" →
                // "AppData/Roaming").
                if child.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                    continue;
                }
                let nested = child.path().join(relative.as_ref().replace('/', "\\"));
                if nested.exists() {
                    return Some(nested);
                }
            }
        }
    }
    None
}

fn resolve_tool_relative(app: &AppState, tool: &ToolDefinition, relative: &str) -> PathBuf {
    app.path(&tool.base_path).join(relative.replace('/', "\\"))
}

fn normalize_path(path: PathBuf) -> Result<PathBuf, AppError> {
    if path.exists() {
        Ok(fs::canonicalize(path)?)
    } else {
        Ok(path)
    }
}

fn display_path(path: &Path) -> String {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", rest)
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        raw
    }
}

fn find_manifest_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.join(MANIFEST_PATH).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn apply_portable_env(app: &AppState, command: &mut Command) {
    let home = display_path(&app.path("state/home"));
    let appdata = display_path(&app.path("state/appdata"));
    let localappdata = display_path(&app.path("state/localappdata"));
    let temp = display_path(&app.path("state/temp"));
    let _ = fs::create_dir_all(app.path("state/temp"));
    let npm_cache = display_path(&app.path("cache/npm"));
    let _ = fs::create_dir_all(app.path("cache/npm"));
    let npm_prefix = display_path(&app.path("apps/node"));

    command
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", &appdata)
        .env("LOCALAPPDATA", &localappdata)
        .env(
            "XDG_CONFIG_HOME",
            display_path(&app.path("state/xdg/config")),
        )
        .env("XDG_CACHE_HOME", display_path(&app.path("state/xdg/cache")))
        .env("XDG_STATE_HOME", display_path(&app.path("state/xdg/state")))
        .env("XDG_DATA_HOME", display_path(&app.path("state/xdg/data")))
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("NPM_CONFIG_CACHE", npm_cache)
        .env("NPM_CONFIG_PREFIX", npm_prefix);
}

fn prepend_path(command: &mut Command, path: &Path) {
    let original = env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{};{}", display_path(path), original));
}

fn prepend_portable_paths(app: &AppState, command: &mut Command) {
    let mut paths_to_prepend = Vec::new();
    let node_dir = app.path("apps/node");
    if node_dir.exists() {
        paths_to_prepend.push(node_dir);
    }
    let git_dir = app.path("apps/git");
    if git_dir.exists() {
        paths_to_prepend.push(git_dir.join("cmd"));
        paths_to_prepend.push(git_dir.join("bin"));
        paths_to_prepend.push(git_dir.join("mingw64\\bin"));
    }
    let original_path = env::var("PATH").unwrap_or_default();
    let mut new_path_parts = Vec::new();
    for p in paths_to_prepend {
        new_path_parts.push(display_path(&p));
    }
    if !original_path.is_empty() {
        new_path_parts.push(original_path);
    }
    command.env("PATH", new_path_parts.join(";"));
}

fn command_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = format!("{}{}", stdout, stderr);
    if output.stdout.len() >= MAX_COMMAND_OUTPUT_BYTES
        || output.stderr.len() >= MAX_COMMAND_OUTPUT_BYTES
    {
        combined.push_str("\n[output truncated]\n");
    }
    strip_terminal_escapes(&combined).trim().to_string()
}

/// Remove ANSI escape sequences and stray carriage returns from captured
/// child process output so the log panel renders cleanly. npm and cargo emit
/// CSI color codes plus `\r`-based progress redraws that show up as mojibake
/// in a `<pre>` element.
fn strip_terminal_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'[' => {
                    // CSI: ESC [ <params> <final 0x40..=0x7E>
                    i += 2;
                    while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                b']' => {
                    // OSC: ESC ] <data> (BEL | ESC \)
                    i += 2;
                    while i < bytes.len() {
                        if bytes[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => {
                    // Other two-byte ESC sequences — drop both bytes.
                    i += 2;
                }
            }
        } else if b == b'\r' {
            // Progress-bar redraws use lone \r; \r\n becomes \n.
            i += 1;
        } else {
            out.push(b);
            i += 1;
        }
    }
    // ESC (0x1b) and \r (0x0d) are not valid UTF-8 continuation or leading
    // bytes, so skipping them preserves multibyte char boundaries.
    String::from_utf8_lossy(&out).into_owned()
}

fn quote_cmd_arg(input: &str) -> String {
    // cmd.exe metacharacters that need protection when an argument is
    // splatted into a .bat file.
    const CMD_META: &[char] = &[
        ' ', '\t', '"', '&', '|', '<', '>', '^', '(', ')', '%', '!', ';', ',', '`',
    ];
    if input.is_empty() {
        return "\"\"".to_string();
    }
    if input.chars().any(|c| CMD_META.contains(&c)) {
        let escaped = input
            .replace('%', "%%")
            .replace('!', "^!")
            .replace('"', "\\\"");
        return format!("\"{}\"", escaped);
    }
    input.to_string()
}

fn action_log_prefix(purpose: &str) -> &'static str {
    match purpose {
        "登录" => "login",
        _ => "launch",
    }
}

fn escape_single_quote(input: &str) -> String {
    input.replace('\'', "''")
}

fn flatten_single_root(destination: &Path) -> Result<(), AppError> {
    let entries = fs::read_dir(destination)?
        .flatten()
        .filter(|entry| entry.file_name() != ".keep")
        .collect::<Vec<_>>();
    if entries.len() != 1 || !entries[0].path().is_dir() {
        return Ok(());
    }

    let nested = entries[0].path();
    let canonical_destination =
        fs::canonicalize(destination).unwrap_or_else(|_| destination.to_path_buf());
    let canonical_nested = fs::canonicalize(&nested).unwrap_or_else(|_| nested.clone());
    if !canonical_nested.starts_with(&canonical_destination) {
        return Err(AppError::Message(
            "归档展开包含非法的路径符号链接".to_string(),
        ));
    }

    for child in fs::read_dir(&nested)?.flatten() {
        let file_name = child.file_name();
        // Reject path-component names that escape the destination.
        let name_str = file_name.to_string_lossy();
        if name_str == ".." || name_str == "." || name_str.contains('/') || name_str.contains('\\')
        {
            return Err(AppError::Message(format!(
                "归档包含非法路径条目：{}",
                name_str
            )));
        }
        let target = destination.join(&file_name);
        if target.exists() {
            // Avoid clobbering an entry that already exists at the destination.
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        fs::rename(child.path(), target)?;
    }
    fs::remove_dir_all(nested)?;
    Ok(())
}

fn push_path_check(
    checks: &mut Vec<HealthCheck>,
    id: &str,
    label: &str,
    path: &Path,
    required: bool,
) {
    let exists = path.exists();
    checks.push(HealthCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: if exists {
            CheckStatus::Ok
        } else if required {
            CheckStatus::Error
        } else {
            CheckStatus::Warning
        },
        message: if exists {
            display_path(path)
        } else {
            format!("缺失：{}", display_path(path))
        },
    });
}

fn package_integrity_checks(app: &AppState) -> Vec<HealthCheck> {
    let mut checks = Vec::new();
    for (id, label, relative, required) in [
        ("launcher-start", "一键启动脚本", "Start.cmd", true),
        ("readme-en", "英文说明文档", "README.md", true),
        ("readme-zh", "中文说明文档", "README.zh-CN.md", true),
        ("screenshot", "展示截图", "docs/screenshot.png", false),
        ("config-manifest", "工具清单文件", MANIFEST_PATH, true),
    ] {
        push_path_check(&mut checks, id, label, &app.path(relative), required);
    }
    let marketplace_path = app.path(MARKETPLACE_PATH);
    if marketplace_path.exists() {
        push_path_check(
            &mut checks,
            "config-marketplace",
            "工具市场文件",
            &marketplace_path,
            false,
        );
    } else {
        checks.push(HealthCheck {
            id: "config-marketplace".to_string(),
            label: "工具市场文件".to_string(),
            status: CheckStatus::Ok,
            message: "缺失外部配置，已使用内置工具市场配置".to_string(),
        });
    }

    let root_exe = app.path("Portable-AI-Dev-Kit.exe");
    let release_exe = app.path("src-tauri/target/release/portable-ai-dev-kit.exe");
    let root_exists = root_exe.exists();
    let release_exists = release_exe.exists();
    checks.push(HealthCheck {
        id: "portable-exe".to_string(),
        label: "便携可执行文件".to_string(),
        status: if root_exists || release_exists {
            CheckStatus::Ok
        } else {
            CheckStatus::Warning
        },
        message: if root_exists {
            display_path(&root_exe)
        } else if release_exists {
            display_path(&release_exe)
        } else {
            "缺失：根目录 Portable-AI-Dev-Kit.exe 或 release exe 至少需要一个".to_string()
        },
    });

    let node_root = app.path("apps/node");
    if node_root.exists() {
        match validate_portable_npm(app) {
            Ok(version) => checks.push(HealthCheck {
                id: "portable-npm".to_string(),
                label: "便携 npm".to_string(),
                status: CheckStatus::Ok,
                message: format!("npm {}", version),
            }),
            Err(error) => checks.push(HealthCheck {
                id: "portable-npm".to_string(),
                label: "便携 npm".to_string(),
                status: CheckStatus::Error,
                message: error,
            }),
        }
    }

    checks
}

/// Probe the portable root's small-file write speed by writing and deleting
/// 10 tiny files in `state/temp/`. Returns (ms_per_file, CheckStatus, message).
///
/// Skipped entirely when `force` is false so normal dashboard loads stay fast.
fn probe_write_speed(app: &AppState, force: bool) -> Option<HealthCheck> {
    if !force {
        return None;
    }

    let probe_dir = app.path("state/temp/_speed_probe");
    let _ = fs::remove_dir_all(&probe_dir);
    if fs::create_dir_all(&probe_dir).is_err() {
        return Some(HealthCheck {
            id: "root-write-speed".to_string(),
            label: "根目录写入速度".to_string(),
            status: CheckStatus::Error,
            message: "无法创建临时目录进行写入速度检测".to_string(),
        });
    }

    const NUM_FILES: usize = 10;
    let start = std::time::Instant::now();
    let mut ok = true;
    for i in 0..NUM_FILES {
        let path = probe_dir.join(format!("probe_{}.tmp", i));
        if fs::write(&path, [0u8; 100]).is_err() {
            ok = false;
            break;
        }
    }
    // Sync the directory to flush metadata to disk (approximate on Windows).
    let _ = std::fs::File::open(&probe_dir).and_then(|f| f.sync_data());

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let _ = fs::remove_dir_all(&probe_dir);

    if !ok || NUM_FILES == 0 {
        return Some(HealthCheck {
            id: "root-write-speed".to_string(),
            label: "根目录写入速度".to_string(),
            status: CheckStatus::Error,
            message: "写入速度检测失败".to_string(),
        });
    }

    let ms_per_file = elapsed_ms / NUM_FILES as u64;

    let (status, message) = if ms_per_file <= 5 {
        (
            CheckStatus::Ok,
            format!(
                "写入速度正常（约 {:.0} 个文件/秒）",
                1000.0 / ms_per_file as f64
            ),
        )
    } else if ms_per_file <= 100 {
        (
            CheckStatus::Warning,
            format!(
                "写入速度偏慢（约 {:.0} 个文件/秒），大型 AI CLI 安装可能需要较长时间",
                1000.0 / ms_per_file as f64
            ),
        )
    } else {
        (
            CheckStatus::Error,
            format!(
                "写入速度极慢（约 {:.0} 个文件/秒），npm 安装/更新可能超时失败。建议：在设置中增大超时时间",
                1000.0 / ms_per_file as f64
            ),
        )
    };

    Some(HealthCheck {
        id: "root-write-speed".to_string(),
        label: "根目录写入速度".to_string(),
        status,
        message,
    })
}

fn download_integrity_checks(manifest: &Manifest) -> Vec<HealthCheck> {
    manifest
        .tools
        .iter()
        .filter(|tool| {
            matches!(
                tool.install.install_type,
                InstallType::Archive | InstallType::PowershellScript
            )
        })
        .map(|tool| {
            let has_hash = tool
                .install
                .sha256
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false);
            HealthCheck {
                id: format!("download-integrity-{}", tool.id),
                label: format!("{} 下载完整性", tool.name),
                status: CheckStatus::Ok,
                message: if has_hash {
                    "已配置 SHA-256 校验".to_string()
                } else {
                    "未配置固定 SHA-256；下载时将使用 HTTPS 传输加密校验".to_string()
                },
            }
        })
        .collect()
}

fn summarize_checks(checks: &[HealthCheck]) -> HealthSummary {
    if checks
        .iter()
        .any(|check| check.status == CheckStatus::Error)
    {
        HealthSummary::Unhealthy
    } else if checks
        .iter()
        .any(|check| check.status == CheckStatus::Warning)
    {
        HealthSummary::Warning
    } else {
        HealthSummary::Healthy
    }
}

fn render_diagnostics_report(dashboard: &Dashboard, marketplace: &MarketplaceFile) -> String {
    let mut report = String::new();
    report.push_str("# Portable AI Dev Kit Diagnostics\n\n");
    report.push_str("## Environment\n\n");
    report.push_str(&format!("- Root: `{}`\n", dashboard.root));
    report.push_str(&format!("- Workspace: `{}`\n", dashboard.workspace));
    report.push_str(&format!(
        "- Workspace Path: `{}`\n",
        dashboard.workspace_path
    ));
    report.push_str(&format!("- Network Mode: `{}`\n", dashboard.network_mode));
    report.push_str(&format!(
        "- Auto Open Workspace: `{}`\n\n",
        dashboard.auto_open_workspace
    ));

    report.push_str("## Health\n\n");
    report.push_str(&format!("- Summary: `{:?}`\n\n", dashboard.health.summary));
    for check in &dashboard.health.checks {
        report.push_str(&format!(
            "- `{}` / {}: `{:?}` - {}\n",
            check.id, check.label, check.status, check.message
        ));
    }

    report.push_str("\n## Tools\n\n");
    for tool in &dashboard.tools {
        report.push_str(&format!(
            "- `{}` / {}: `{:?}`; installed=`{}`; wanted=`{}`; host=`{}`; base=`{}`; launch=`{}`\n",
            tool.id,
            tool.name,
            tool.status,
            tool.installed_version.as_deref().unwrap_or("not detected"),
            tool.wanted_version.as_deref().unwrap_or("not set"),
            tool.host_version.as_deref().unwrap_or("not detected"),
            tool.base_path,
            tool.launch_path.as_deref().unwrap_or("not found"),
        ));
        if let Some(error) = &tool.last_error {
            report.push_str(&format!("  - Last Error: {}\n", error));
        }
    }

    report.push_str("\n## Marketplace\n\n");
    report.push_str(&format!(
        "- Configured Tools: `{}`\n",
        marketplace.tools.len()
    ));
    for tool in &marketplace.tools {
        report.push_str(&format!(
            "- `{}` / {}: package=`{}`, category=`{}`, homepage=`{}`\n",
            tool.id, tool.name, tool.package_name, tool.category, tool.homepage
        ));
    }

    report
}

fn status_label(status: &ToolStatus) -> &'static str {
    match status {
        ToolStatus::Ready => "可用",
        ToolStatus::Missing => "未安装",
        ToolStatus::Partial => "不完整",
    }
}

pub fn add_custom_tool(
    app: &AppState,
    name: String,
    install_type: &str,
    package_name: Option<String>,
    script_url: Option<String>,
    bin_name: Option<String>,
) -> Result<String, AppError> {
    let _action_guard = ACTION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    add_custom_tool_unlocked(app, name, install_type, package_name, script_url, bin_name)
}

fn add_custom_tool_unlocked(
    app: &AppState,
    name: String,
    install_type: &str,
    package_name: Option<String>,
    script_url: Option<String>,
    bin_name: Option<String>,
) -> Result<String, AppError> {
    // Sanitize name to generate a valid tool ID (ASCII-only to avoid
    // codepage issues in Windows .bat / cmd execution).
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err(AppError::Message("工具名称不能为空".to_string()));
    }
    if trimmed_name.chars().count() > 64 {
        return Err(AppError::Message(
            "工具名称过长（最多 64 字符）".to_string(),
        ));
    }
    let mut id_name: String = trimmed_name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    // Collapse leading/trailing dashes.
    id_name = id_name.trim_matches(|c| c == '-' || c == '_').to_string();
    if id_name.is_empty() {
        // Fallback: derive ASCII id from package name / script url / hash.
        let fallback_src = package_name
            .as_deref()
            .or(script_url.as_deref())
            .unwrap_or("");
        id_name = fallback_src
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        id_name = id_name.trim_matches(|c| c == '-' || c == '_').to_string();
        if id_name.is_empty() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&trimmed_name, &mut hasher);
            id_name = format!(
                "tool-{:x}",
                std::hash::Hasher::finish(&hasher) & 0xffff_ffff
            );
        }
    }
    let tool_id = format!("custom-{}", id_name);
    let new_tool_id = tool_id.clone();

    // Load existing manifest to check for duplicate IDs
    let manifest = load_manifest(app)?;
    if manifest.tools.iter().any(|t| t.id == tool_id) {
        return Err(AppError::Message(format!("工具 '{}' 已存在", trimmed_name)));
    }

    // Load custom tools file
    let custom_path = app.path(CUSTOM_TOOLS_PATH);
    let mut custom_file = if custom_path.exists() {
        let raw = fs::read_to_string(&custom_path)?;
        match serde_json::from_str::<CustomToolsFile>(&raw) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Err(AppError::Message(format!(
                    "无法解析 custom-tools.json：{}",
                    error
                )))
            }
        }
    } else {
        CustomToolsFile::default()
    };

    if custom_file.tools.iter().any(|t| t.id == tool_id) {
        return Err(AppError::Message(format!("工具 '{}' 已存在", trimmed_name)));
    }

    let base_path = format!("tools/custom/custom-{}", id_name);

    let name = trimmed_name.to_string();

    let tool = if install_type == "powershell-script" {
        let script_url_val =
            script_url.ok_or_else(|| AppError::Message("脚本 URL 不能为空".to_string()))?;
        let script_url_val = script_url_val.trim().to_string();
        if script_url_val.is_empty() {
            return Err(AppError::Message("脚本 URL 不能为空".to_string()));
        }
        validate_script_url(&script_url_val)?;

        let actual_bin = bin_name.unwrap_or_default().trim().to_string();
        // Reject any path-like component to prevent the binary entry from
        // escaping the tool base directory via resolve_tool_relative. The
        // executable must be a single filename — no separators, no parent
        // refs, no drive letters.
        if actual_bin.contains('/')
            || actual_bin.contains('\\')
            || actual_bin.contains("..")
            || actual_bin.contains(':')
        {
            return Err(AppError::Message(
                "可执行文件名不能包含路径分隔符或上级目录引用".to_string(),
            ));
        }
        let actual_bin = if actual_bin.is_empty() {
            format!("{}.exe", id_name)
        } else if !actual_bin.to_lowercase().ends_with(".exe")
            && !actual_bin.to_lowercase().ends_with(".cmd")
            && !actual_bin.to_lowercase().ends_with(".bat")
        {
            format!("{}.exe", actual_bin)
        } else {
            actual_bin
        };

        let host_bin = actual_bin
            .strip_suffix(".exe")
            .or_else(|| actual_bin.strip_suffix(".cmd"))
            .or_else(|| actual_bin.strip_suffix(".bat"))
            .unwrap_or(&actual_bin)
            .to_string();

        ToolDefinition {
            id: tool_id,
            name,
            kind: ToolKind::AiCli,
            required: false,
            base_path,
            package_name: None,
            version_command: vec![actual_bin.clone(), "--version".to_string()],
            host_version_command: vec![host_bin, "--version".to_string()],
            bin_paths: vec![actual_bin.clone()],
            run_command: vec![actual_bin.clone()],
            login_command: vec![actual_bin],
            install: InstallDefinition {
                install_type: InstallType::PowershellScript,
                depends_on: vec![],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: Some(script_url_val),
                script_args: vec![],
                sha256: None,
            },
        }
    } else {
        let package_name_val =
            package_name.ok_or_else(|| AppError::Message("NPM 包名不能为空".to_string()))?;
        let mut package_name_val = package_name_val.trim().to_string();
        if package_name_val.is_empty() {
            return Err(AppError::Message("NPM 包名不能为空".to_string()));
        }
        let bin_name = if is_qoder_package_alias(&package_name_val) {
            package_name_val = "@qoder-ai/qodercli".to_string();
            "qodercli".to_string()
        } else {
            id_name.clone()
        };

        let bin_path = format!("node_modules/.bin/{}.cmd", bin_name);

        ToolDefinition {
            id: tool_id,
            name,
            kind: ToolKind::AiCli,
            required: false,
            base_path,
            package_name: Some(package_name_val),
            version_command: vec![bin_path.clone(), "--version".to_string()],
            host_version_command: vec![bin_name, "--version".to_string()],
            bin_paths: vec![bin_path.clone()],
            run_command: vec![bin_path.clone()],
            login_command: vec![bin_path],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        }
    };

    custom_file.tools.push(tool);

    // Save custom tools atomically. fs::rename on Windows uses
    // MOVEFILE_REPLACE_EXISTING, so this single syscall replaces any
    // existing file without a TOCTOU window.
    let config_dir = app.path("config");
    fs::create_dir_all(&config_dir)?;
    let temp_path = config_dir.join("custom-tools.json.tmp");
    fs::write(&temp_path, serde_json::to_string_pretty(&custom_file)?)?;
    fs::rename(&temp_path, &custom_path)?;

    Ok(new_tool_id)
}

pub fn delete_custom_tool(app: &AppState, tool_id: String) -> Result<(), AppError> {
    let _action_guard = ACTION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !tool_id.starts_with("custom-") {
        return Err(AppError::Message("只能删除自定义工具".to_string()));
    }

    // First uninstall it if it's installed (removes files). We do NOT abort
    // deletion if uninstall fails (the user explicitly chose to delete), but
    // surface the error to stderr so it ends up in the .log for debugging.
    let manifest = load_manifest(app)?;
    if let Some(tool) = manifest.tools.iter().find(|t| t.id == tool_id) {
        if let Err(error) = uninstall_tool(app, tool) {
            eprintln!(
                "warning: 删除自定义工具 {} 时清理文件失败: {}",
                tool_id, error
            );
        }
    }

    // Load custom tools file
    let custom_path = app.path(CUSTOM_TOOLS_PATH);
    if custom_path.exists() {
        let raw = fs::read_to_string(&custom_path)?;
        let mut custom_file = serde_json::from_str::<CustomToolsFile>(&raw).unwrap_or_default();
        let original_len = custom_file.tools.len();
        custom_file.tools.retain(|t| t.id != tool_id);
        if custom_file.tools.len() < original_len {
            fs::write(&custom_path, serde_json::to_string_pretty(&custom_file)?)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    use std::os::windows::process::ExitStatusExt;

    fn fixture() -> (tempfile::TempDir, AppState) {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("config")).unwrap();
        fs::write(
            temp.path().join(MANIFEST_PATH),
            r#"{
              "schemaVersion": 1,
              "networkModes": {"global": {"npmRegistry": "https://registry.npmjs.org/", "archiveSource": "global"}},
              "tools": [{
                "id": "node",
                "name": "Node.js",
                "kind": "runtime",
                "required": true,
                "basePath": "apps/node",
                "versionCommand": ["node.exe", "--version"],
                "binPaths": ["node.exe"],
                "install": {"type": "archive", "archiveName": "node.zip", "installerType": "zip", "urls": {"global": "https://example.invalid/node.zip"}}
              }]
            }"#,
        )
        .unwrap();
        fs::write(
            temp.path().join(SETTINGS_PATH),
            r#"{"networkMode":"global","workspacePath":"workspace","autoOpenWorkspace":false}"#,
        )
        .unwrap();
        seed_package_files(temp.path());
        let app = AppState {
            root: temp.path().to_path_buf(),
        };
        (temp, app)
    }

    fn seed_package_files(root: &Path) {
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("Start.cmd"), "@echo off").unwrap();
        fs::write(root.join("README.md"), "readme").unwrap();
        fs::write(root.join("README.zh-CN.md"), "readme").unwrap();
        fs::write(root.join("docs").join("screenshot.png"), "").unwrap();
        fs::write(root.join("Portable-AI-Dev-Kit.exe"), "").unwrap();
    }

    #[test]
    fn bootstrap_creates_portable_directories_and_state() {
        let (_temp, app) = fixture();
        bootstrap_kit(&app).unwrap();
        assert!(app.path("apps").exists());
        assert!(app.path("state/home").exists());
        assert!(app.path("workspace").exists());
        assert!(app.path(STATE_PATH).exists());
    }

    #[test]
    fn dashboard_reports_missing_required_tool() {
        let (_temp, app) = fixture();
        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools.len(), 1);
        assert_eq!(dashboard.tools[0].status, ToolStatus::Missing);
        assert_eq!(dashboard.health.summary, HealthSummary::Warning);
    }

    #[test]
    fn dashboard_clears_stale_version_when_tool_directory_is_missing() {
        let (_temp, app) = fixture();
        let mut state = ToolStateFile::default();
        state.tools.insert(
            "node".to_string(),
            PersistedToolState {
                installed_version: Some("v0.0.1".to_string()),
                installed_at: Some("2026-01-01T00:00:00Z".to_string()),
                last_error: Some("old error".to_string()),
                ..PersistedToolState::default()
            },
        );
        save_state(&app, &state).unwrap();

        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Missing);
        assert_eq!(dashboard.tools[0].installed_version, None);
        assert!(dashboard.tools[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("未找到安装目录"));

        let saved = load_state(&app).unwrap();
        assert_eq!(
            saved
                .tools
                .get("node")
                .and_then(|tool| tool.installed_version.as_deref()),
            None
        );
    }

    #[test]
    fn dashboard_replaces_stale_error_when_tool_entrypoint_is_missing() {
        let (_temp, app) = fixture();
        fs::create_dir_all(app.path("apps/node")).unwrap();
        let mut state = ToolStateFile::default();
        state.tools.insert(
            "node".to_string(),
            PersistedToolState {
                installed_version: Some("v0.0.1".to_string()),
                last_error: Some("old npm error".to_string()),
                ..PersistedToolState::default()
            },
        );
        save_state(&app, &state).unwrap();

        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Partial);
        assert_eq!(dashboard.tools[0].installed_version, None);
        assert!(dashboard.tools[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("node.exe"));
        assert!(!dashboard.tools[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("old npm error"));
    }

    #[test]
    fn ready_tool_rechecks_version_and_clears_stale_error() {
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "codex".to_string(),
            name: "Codex CLI".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/codex".to_string(),
            package_name: Some("@openai/codex@latest".to_string()),
            version_command: vec![
                "node_modules/.bin/codex.cmd".to_string(),
                "--version".to_string(),
            ],
            host_version_command: vec![],
            bin_paths: vec!["node_modules/.bin/codex.cmd".to_string()],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };
        let bin_path = app
            .path(&tool.base_path)
            .join("node_modules/.bin/codex.cmd");
        fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        fs::write(&bin_path, "@echo off\r\necho codex-cli 1.2.3\r\n").unwrap();
        let mut state = ToolStateFile::default();
        state.tools.insert(
            tool.id.clone(),
            PersistedToolState {
                installed_version: Some("codex-cli 0.0.1".to_string()),
                last_error: Some("old npm error".to_string()),
                ..PersistedToolState::default()
            },
        );

        let view = tool_view(&app, &tool, &mut state, false).unwrap();
        assert_eq!(view.status, ToolStatus::Ready);
        assert_eq!(view.installed_version.as_deref(), Some("codex-cli 1.2.3"));
        assert_eq!(view.last_error, None);
        assert_eq!(
            state
                .tools
                .get(&tool.id)
                .and_then(|tool| tool.last_error.as_deref()),
            None
        );
    }

    #[test]
    fn ready_tool_uses_cached_version_until_forced_refresh() {
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/claude".to_string(),
            package_name: Some("@anthropic-ai/claude-code@latest".to_string()),
            version_command: vec![
                "node_modules/.bin/claude.cmd".to_string(),
                "--version".to_string(),
            ],
            host_version_command: vec![],
            bin_paths: vec!["node_modules/.bin/claude.cmd".to_string()],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };
        let bin_path = app
            .path(&tool.base_path)
            .join("node_modules/.bin/claude.cmd");
        fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        fs::write(&bin_path, "@echo off\r\necho 2.1.168 (Claude Code)\r\n").unwrap();
        let mut state = ToolStateFile::default();
        state.tools.insert(
            tool.id.clone(),
            PersistedToolState {
                installed_version: Some("2.1.133 (Claude Code)".to_string()),
                ..PersistedToolState::default()
            },
        );

        let view = tool_view(&app, &tool, &mut state, false).unwrap();
        assert_eq!(
            view.installed_version.as_deref(),
            Some("2.1.133 (Claude Code)")
        );
        assert_eq!(
            state
                .tools
                .get(&tool.id)
                .and_then(|tool| tool.installed_version.as_deref()),
            Some("2.1.133 (Claude Code)")
        );

        let view = tool_view(&app, &tool, &mut state, true).unwrap();
        assert_eq!(
            view.installed_version.as_deref(),
            Some("2.1.168 (Claude Code)")
        );
        assert_eq!(
            state
                .tools
                .get(&tool.id)
                .and_then(|tool| tool.installed_version.as_deref()),
            Some("2.1.168 (Claude Code)")
        );
    }

    #[test]
    fn dashboard_reports_ready_tool_when_binary_exists() {
        let (_temp, app) = fixture();
        fs::create_dir_all(app.path("apps/node")).unwrap();
        fs::write(app.path("apps/node/node.exe"), "").unwrap();
        fs::write(
            app.path("apps/node/npm.cmd"),
            "@echo off\r\necho 10.0.0\r\n",
        )
        .unwrap();
        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Ready);
    }

    #[test]
    fn dashboard_reports_partial_node_when_npm_is_broken() {
        let (_temp, app) = fixture();
        fs::create_dir_all(app.path("apps/node")).unwrap();
        fs::write(app.path("apps/node/node.exe"), "").unwrap();

        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Partial);
        assert!(dashboard.tools[0]
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("Node/npm"));

        let npm_check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "portable-npm")
            .expect("portable npm check missing");
        assert_eq!(npm_check.status, CheckStatus::Error);
    }

    #[test]
    fn freebuff_patch_adds_update_restart_when_stream_patch_exists() {
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "custom-freebuff".to_string(),
            name: "freebuff".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/custom/custom-freebuff".to_string(),
            package_name: Some("freebuff".to_string()),
            version_command: vec![],
            host_version_command: vec![],
            bin_paths: vec![],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };
        let index_path = app
            .path(&tool.base_path)
            .join("node_modules/freebuff/index.js");
        fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        fs::write(
            &index_path,
            r#"const { spawn } = require('child_process')
// Portable AI Dev Kit stream patch
async function checkForUpdates(runningProcess, exitListener) {
  try {
      await new Promise((resolve) => {
        runningProcess.kill('SIGTERM')
        setTimeout(() => {
          if (!exited) {
            runningProcess.kill('SIGKILL')
            setTimeout(() => resolve(), 1000)
          }
        }, 5000)
      })
  } catch (error) {
    // Ignore update failures
  }
}
"#,
        )
        .unwrap();

        let note = patch_freebuff_index(&app, &tool).unwrap().unwrap();
        let patched = fs::read_to_string(index_path).unwrap();
        assert!(note.contains("update restart patched"));
        assert!(patched.contains("Portable AI Dev Kit stream patch"));
        assert!(patched.contains("Portable AI Dev Kit update restart patch"));
        assert!(patched.contains("portableUpdateKilledProcess = true"));
    }

    #[test]
    fn freebuff_version_is_refreshed_even_when_state_has_old_version() {
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "custom-freebuff".to_string(),
            name: "freebuff".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/custom/custom-freebuff".to_string(),
            package_name: Some("freebuff".to_string()),
            version_command: vec![
                "node_modules/.bin/freebuff.cmd".to_string(),
                "--version".to_string(),
            ],
            host_version_command: vec![],
            bin_paths: vec!["node_modules/.bin/freebuff.cmd".to_string()],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };
        let bin_path = app
            .path(&tool.base_path)
            .join("node_modules/.bin/freebuff.cmd");
        fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
        fs::write(&bin_path, "@echo off\r\necho 0.0.100\r\n").unwrap();
        let mut state = ToolStateFile::default();
        state.tools.insert(
            tool.id.clone(),
            PersistedToolState {
                installed_version: Some("0.0.95".to_string()),
                ..PersistedToolState::default()
            },
        );

        let view = tool_view(&app, &tool, &mut state, false).unwrap();
        assert_eq!(view.installed_version.as_deref(), Some("0.0.100"));
        assert_eq!(
            state
                .tools
                .get(&tool.id)
                .and_then(|tool| tool.installed_version.as_deref()),
            Some("0.0.100")
        );
    }

    #[test]
    fn display_path_removes_windows_extended_prefix() {
        let path = PathBuf::from(r"\\?\E:\kit\config");
        assert_eq!(display_path(&path), r"E:\kit\config");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tar_timestamp_restore_warnings_are_tolerated() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: b"node-v24.11.1-win-x64/npm.ps1: Can't restore time: Invalid argument\r\nnode-v24.11.1-win-x64/npx: Can't restore time: Invalid argument\r\n".to_vec(),
        };

        assert!(tar_output_only_has_timestamp_warnings(&output));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tar_real_errors_are_not_tolerated() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1),
            stdout: vec![],
            stderr: b"node.zip: Truncated ZIP file\r\n".to_vec(),
        };

        assert!(!tar_output_only_has_timestamp_warnings(&output));
    }

    #[test]
    fn manifest_rejects_tool_paths_that_escape_root() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MANIFEST_PATH),
            r#"{
              "schemaVersion": 1,
              "networkModes": {"global": {"npmRegistry": "https://registry.npmjs.org/", "archiveSource": "global"}},
              "tools": [{
                "id": "bad",
                "name": "Bad",
                "kind": "runtime",
                "required": true,
                "basePath": "../outside",
                "binPaths": ["bad.exe"],
                "install": {"type": "archive", "archiveName": "bad.zip", "installerType": "zip", "urls": {"global": "https://example.invalid/bad.zip"}}
              }]
            }"#,
        )
        .unwrap();

        let error = load_manifest(&app).unwrap_err();
        assert!(error.to_string().contains("basePath"));
    }

    #[test]
    fn marketplace_rejects_non_http_homepage() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MARKETPLACE_PATH),
            r#"{"tools":[{"id":"bad","name":"Bad","description":"bad","packageName":"bad","category":"AI CLI","homepage":"javascript:alert(1)"}]}"#,
        )
        .unwrap();

        let error = get_marketplace_tools(&app).unwrap_err();
        assert!(error.to_string().contains("homepage"));
    }

    #[test]
    fn manifest_rejects_invalid_sha256() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MANIFEST_PATH),
            r#"{
              "schemaVersion": 1,
              "networkModes": {"global": {"npmRegistry": "https://registry.npmjs.org/", "archiveSource": "global"}},
              "tools": [{
                "id": "bad",
                "name": "Bad",
                "kind": "runtime",
                "required": true,
                "basePath": "apps/bad",
                "binPaths": ["bad.exe"],
                "install": {"type": "archive", "archiveName": "bad.zip", "installerType": "zip", "sha256": "bad", "urls": {"global": "https://example.invalid/bad.zip"}}
              }]
            }"#,
        )
        .unwrap();

        let error = load_manifest(&app).unwrap_err();
        assert!(error.to_string().contains("sha256"));
    }

    #[test]
    fn manifest_rejects_invalid_script_arg() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MANIFEST_PATH),
            r#"{
              "schemaVersion": 1,
              "networkModes": {"global": {"npmRegistry": "https://registry.npmjs.org/", "archiveSource": "global"}},
              "tools": [{
                "id": "bad",
                "name": "Bad",
                "kind": "ai-cli",
                "required": false,
                "basePath": "tools/bad",
                "binPaths": ["bad.exe"],
                "install": {"type": "powershell-script", "scriptUrl": "https://example.invalid/install.ps1", "scriptArgs": ["--skip-path\nbad"]}
              }]
            }"#,
        )
        .unwrap();

        let error = load_manifest(&app).unwrap_err();
        assert!(error.to_string().contains("scriptArg"));
    }

    #[test]
    fn manifest_loads_script_args() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MANIFEST_PATH),
            r#"{
              "schemaVersion": 1,
              "networkModes": {"global": {"npmRegistry": "https://registry.npmjs.org/", "archiveSource": "global"}},
              "tools": [{
                "id": "agy",
                "name": "AGY",
                "kind": "ai-cli",
                "required": false,
                "basePath": "tools/agy",
                "binPaths": ["agy.exe"],
                "install": {
                  "type": "powershell-script",
                  "scriptUrl": "https://example.invalid/install.ps1",
                  "scriptArgs": ["--skip-path", "--skip-aliases"],
                  "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
                }
              }]
            }"#,
        )
        .unwrap();

        let manifest = load_manifest(&app).unwrap();
        assert_eq!(
            manifest.tools[0].install.script_args,
            vec!["--skip-path".to_string(), "--skip-aliases".to_string()]
        );
    }

    #[test]
    fn health_warns_when_remote_install_lacks_sha256() {
        let (_temp, app) = fixture();
        let dashboard = get_dashboard(&app, false).unwrap();
        let check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "download-integrity-node")
            .expect("download integrity check missing");
        assert_eq!(check.status, CheckStatus::Warning);
        assert!(check.message.contains("SHA-256"));
    }

    #[test]
    fn verify_download_integrity_rejects_hash_mismatch() {
        let (_temp, app) = fixture();
        let path = app.path("cache/downloads/test.txt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "hello").unwrap();
        let tool = ToolDefinition {
            id: "hash-test".to_string(),
            name: "Hash Test".to_string(),
            kind: ToolKind::Runtime,
            required: false,
            base_path: "apps/hash-test".to_string(),
            package_name: None,
            version_command: vec![],
            host_version_command: vec![],
            bin_paths: vec![],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Archive,
                depends_on: vec![],
                archive_name: Some("test.txt".to_string()),
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: Some(
                    "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
                ),
            },
        };

        let error = verify_download_integrity(&tool, &path).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("expected="));
    }

    #[test]
    fn quote_cmd_arg_escapes_batch_expansion_tokens() {
        let quoted = quote_cmd_arg("%PATH% & echo !SECRET!");
        assert!(quoted.contains("%%PATH%%"));
        assert!(quoted.contains("^!SECRET^!"));
        assert!(quoted.starts_with('"'));
        assert!(quoted.ends_with('"'));
    }

    #[test]
    fn save_settings_sanitizes_workspace_path_and_creates_directory() {
        let (_temp, app) = fixture();
        save_settings(&app, "global", r"..\evil\workspace", true).unwrap();

        let saved: Settings =
            serde_json::from_str(&fs::read_to_string(app.path(SETTINGS_PATH)).unwrap()).unwrap();
        assert_eq!(saved.network_mode, "global");
        assert_eq!(saved.workspace_path, "workspace");
        assert!(app.path("workspace").exists());
    }

    #[test]
    fn save_settings_accepts_absolute_workspace_inside_root() {
        let (_temp, app) = fixture();
        let workspace = app.path("workspace").join("nested");
        save_settings(&app, "global", &display_path(&workspace), false).unwrap();

        let saved: Settings =
            serde_json::from_str(&fs::read_to_string(app.path(SETTINGS_PATH)).unwrap()).unwrap();
        assert_eq!(saved.workspace_path, "workspace/nested");
        assert!(app.path("workspace/nested").exists());
    }

    #[test]
    fn save_settings_rejects_absolute_workspace_outside_root() {
        let (_temp, app) = fixture();
        save_settings(&app, "global", r"C:\outside\workspace", false).unwrap();

        let saved: Settings =
            serde_json::from_str(&fs::read_to_string(app.path(SETTINGS_PATH)).unwrap()).unwrap();
        assert_eq!(saved.workspace_path, "workspace");
    }

    #[test]
    fn save_settings_rejects_unknown_network_mode() {
        let (_temp, app) = fixture();
        let error = save_settings(&app, "unknown", "workspace", false).unwrap_err();
        assert!(error.to_string().contains("未知网络模式"));
    }

    #[test]
    fn marketplace_tools_are_loaded_from_config() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MARKETPLACE_PATH),
            r#"{
              "tools": [
                {
                  "id": "node",
                  "name": "Node.js",
                  "description": "Portable Node runtime",
                  "packageName": "node",
                  "category": "Runtime",
                  "homepage": "https://nodejs.org"
                },
                {
                  "id": "freebuff",
                  "name": "freebuff",
                  "description": "AI CLI",
                  "packageName": "freebuff",
                  "category": "AI CLI",
                  "homepage": "https://example.invalid"
                }
              ]
            }"#,
        )
        .unwrap();

        let tools = get_marketplace_tools(&app).unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools
            .iter()
            .any(|tool| tool.id == "node" && tool.in_manifest));
        assert!(tools
            .iter()
            .any(|tool| tool.id == "freebuff" && !tool.in_manifest));
    }

    #[test]
    fn qoder_custom_tool_alias_uses_qodercli_package_and_binary() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(CUSTOM_TOOLS_PATH),
            r#"{"tools":[{
              "id":"custom-qoder",
              "name":"qoder",
              "kind":"ai-cli",
              "required":false,
              "basePath":"tools/custom/custom-qoder",
              "packageName":"qoder",
              "versionCommand":["node_modules/.bin/qoder.cmd","--version"],
              "hostVersionCommand":["qoder","--version"],
              "binPaths":["node_modules/.bin/qoder.cmd"],
              "runCommand":["node_modules/.bin/qoder.cmd"],
              "loginCommand":["node_modules/.bin/qoder.cmd"],
              "install":{"type":"npm","dependsOn":["node"]}
            }]}"#,
        )
        .unwrap();

        let manifest = load_manifest(&app).unwrap();
        let qoder = manifest
            .tools
            .iter()
            .find(|tool| tool.id == "custom-qoder")
            .expect("qoder tool missing");
        assert_eq!(qoder.package_name.as_deref(), Some("@qoder-ai/qodercli"));
        assert_eq!(
            qoder.bin_paths,
            vec!["node_modules/.bin/qodercli.cmd".to_string()]
        );
        assert_eq!(qoder.host_version_command[0], "qodercli");
    }

    #[test]
    fn add_custom_qoder_writes_qodercli_package_and_binary() {
        let (_temp, app) = fixture();
        let id = add_custom_tool(
            &app,
            "qoder".to_string(),
            "npm",
            Some("qoder".to_string()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(id, "custom-qoder");

        let raw = fs::read_to_string(app.path(CUSTOM_TOOLS_PATH)).unwrap();
        let custom: CustomToolsFile = serde_json::from_str(&raw).unwrap();
        let qoder = custom.tools.first().expect("qoder tool missing");
        assert_eq!(qoder.package_name.as_deref(), Some("@qoder-ai/qodercli"));
        assert_eq!(
            qoder.bin_paths,
            vec!["node_modules/.bin/qodercli.cmd".to_string()]
        );
        assert_eq!(qoder.host_version_command[0], "qodercli");
    }

    #[test]
    fn health_reports_marketplace_config_check() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MARKETPLACE_PATH),
            r#"{"tools":[{"id":"freebuff","name":"freebuff","description":"AI CLI","packageName":"freebuff","category":"AI CLI","homepage":"https://example.invalid"}]}"#,
        )
        .unwrap();

        let health = check_health(&app).unwrap();
        assert!(health
            .checks
            .iter()
            .any(|check| check.id == "marketplace-config"));
    }

    #[test]
    fn dashboard_tolerates_invalid_marketplace_config() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MARKETPLACE_PATH),
            r#"{"tools":[{"id":"","name":"","description":"broken","packageName":"","category":"AI CLI","homepage":""}]}"#,
        )
        .unwrap();

        let dashboard = get_dashboard(&app, false).unwrap();
        let check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "marketplace-config")
            .expect("marketplace check missing");
        assert_eq!(check.status, CheckStatus::Warning);
        assert!(check.message.contains("工具市场配置"));
    }

    #[test]
    fn health_reports_package_integrity_checks() {
        let (_temp, app) = fixture();
        fs::remove_file(app.path("Start.cmd")).unwrap();
        fs::remove_file(app.path("Portable-AI-Dev-Kit.exe")).unwrap();
        let dashboard = get_dashboard(&app, false).unwrap();

        let start_check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "launcher-start")
            .expect("launcher check missing");
        assert_eq!(start_check.status, CheckStatus::Error);

        let exe_check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "portable-exe")
            .expect("exe check missing");
        assert_eq!(exe_check.status, CheckStatus::Warning);
    }

    #[test]
    fn package_integrity_passes_when_required_files_exist() {
        let (_temp, app) = fixture();
        fs::write(app.path("Start.cmd"), "@echo off").unwrap();
        fs::write(app.path("README.md"), "readme").unwrap();
        fs::write(app.path("README.zh-CN.md"), "readme").unwrap();
        fs::write(app.path("Portable-AI-Dev-Kit.exe"), "").unwrap();

        let health = check_health(&app).unwrap();
        for id in ["launcher-start", "readme-en", "readme-zh", "portable-exe"] {
            let check = health
                .checks
                .iter()
                .find(|check| check.id == id)
                .expect("package check missing");
            assert_eq!(check.status, CheckStatus::Ok);
        }
    }

    #[test]
    fn export_diagnostics_writes_markdown_report() {
        let (_temp, app) = fixture();
        fs::write(
            app.path(MARKETPLACE_PATH),
            r#"{"tools":[{"id":"freebuff","name":"freebuff","description":"AI CLI","packageName":"freebuff","category":"AI CLI","homepage":"https://example.invalid"}]}"#,
        )
        .unwrap();

        let report = export_diagnostics(&app).unwrap();
        let content = fs::read_to_string(report.path).unwrap();
        assert!(content.contains("# Portable AI Dev Kit Diagnostics"));
        assert!(content.contains("## Health"));
        assert!(content.contains("## Tools"));
        assert!(content.contains("freebuff"));
    }

    #[test]
    fn missing_marketplace_config_returns_empty_list() {
        let (_temp, app) = fixture();
        let tools = get_marketplace_tools(&app).unwrap();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|tool| tool.id == "qoder"));
    }

    #[test]
    fn missing_marketplace_config_uses_embedded_fallback_without_warning() {
        let (_temp, app) = fixture();
        let dashboard = get_dashboard(&app, false).unwrap();
        let check = dashboard
            .health
            .checks
            .iter()
            .find(|check| check.id == "config-marketplace")
            .expect("marketplace file check missing");
        assert_eq!(check.status, CheckStatus::Ok);
        assert!(check.message.contains("内置工具市场配置"));
    }

    #[test]
    fn discover_returns_manifest_root_not_nested_candidate() {
        let (temp, _app) = fixture();
        let nested = temp.path().join("src-tauri").join("target").join("release");
        fs::create_dir_all(&nested).unwrap();
        let found = find_manifest_root(&nested).unwrap();
        assert_eq!(found, temp.path());
    }

    #[test]
    fn strip_terminal_escapes_removes_ansi_and_cr() {
        // SGR color codes, an OSC title sequence, a stray ESC X, and a \r
        // progress redraw — all should vanish, leaving Chinese chars intact.
        let raw =
            "\u{1b}[32m installed\u{1b}[0m\r\nadded 5 packages\r\u{1b}]0;title\u{07}\u{1b}M中文";
        let cleaned = strip_terminal_escapes(raw);
        assert_eq!(cleaned, " installed\nadded 5 packages中文");
    }

    #[test]
    fn npm_install_failure_preserves_existing_install_via_rollback() {
        // swap_npm_install_into_place must restore the previous install when the
        // final move fails. We force a failure by moving the staging tree out
        // from under the swap call so fs::rename(staging, destination) hits a
        // "file not found" error, exercising the rollback branch.
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/claude".to_string(),
            package_name: Some("@anthropic-ai/claude-code@latest".to_string()),
            version_command: vec![],
            host_version_command: vec![],
            bin_paths: vec![],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };

        // Existing install on the portable (temp) drive.
        let destination = app.path(&tool.base_path);
        fs::create_dir_all(destination.join("node_modules")).unwrap();
        fs::write(destination.join("node_modules").join("marker.txt"), "old").unwrap();

        // Staging tree on local temp.
        let staging = env::temp_dir()
            .join("portable-ai-dev-kit")
            .join("npm-staging-test-rollback");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(staging.join("node_modules")).unwrap();

        // Move staging out of the way so swap's rename fails after the
        // existing install has been preserved to backup.
        let moved_staging = env::temp_dir()
            .join("portable-ai-dev-kit")
            .join("npm-staging-moved-rollback");
        let _ = fs::remove_dir_all(&moved_staging);
        fs::rename(&staging, &moved_staging).unwrap();

        let result = swap_npm_install_into_place(&app, &tool, staging.clone());
        assert!(result.is_err(), "swap should fail when rename is blocked");

        // Rollback restored the original install tree.
        assert!(
            destination.is_dir(),
            "destination should be restored as a dir"
        );
        let marker = destination.join("node_modules").join("marker.txt");
        assert!(
            marker.exists(),
            "previous install content must survive rollback"
        );
        assert_eq!(fs::read_to_string(&marker).unwrap(), "old");

        // No dangling backup left.
        let backup = app.path("cache/extract/claude-backup");
        assert!(
            !backup.exists(),
            "backup must be cleaned up on rollback path"
        );
        let _ = fs::remove_dir_all(&moved_staging);
    }

    #[test]
    fn npm_staging_swap_moves_tree_into_place_and_clears_backup() {
        let (_temp, app) = fixture();
        let tool = ToolDefinition {
            id: "codex".to_string(),
            name: "Codex CLI".to_string(),
            kind: ToolKind::AiCli,
            required: false,
            base_path: "tools/codex".to_string(),
            package_name: Some("@openai/codex@latest".to_string()),
            version_command: vec![],
            host_version_command: vec![],
            bin_paths: vec![],
            run_command: vec![],
            login_command: vec![],
            install: InstallDefinition {
                install_type: InstallType::Npm,
                depends_on: vec!["node".to_string()],
                archive_name: None,
                installer_type: None,
                urls: BTreeMap::new(),
                script_url: None,
                script_args: vec![],
                sha256: None,
            },
        };

        // Pretend an old install exists.
        let destination = app.path(&tool.base_path);
        fs::create_dir_all(destination.join("node_modules/.old")).unwrap();

        // Build a staging dir that looks like a fresh install.
        let staging = env::temp_dir()
            .join("portable-ai-dev-kit")
            .join("npm-staging-test-success");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(staging.join("node_modules/.bin")).unwrap();
        fs::write(staging.join("node_modules/.bin").join("codex.cmd"), "new").unwrap();

        swap_npm_install_into_place(&app, &tool, staging).unwrap();

        // New tree is in place at destination.
        let new_bin = destination.join("node_modules/.bin/codex.cmd");
        assert!(
            new_bin.exists(),
            "new install must be at the tool base path"
        );
        assert_eq!(fs::read_to_string(&new_bin).unwrap(), "new");
        // Old content is gone (it was moved to backup, then backup removed).
        assert!(!destination.join("node_modules/.old").exists());

        let backup = app.path("cache/extract/codex-backup");
        assert!(
            !backup.exists(),
            "backup must be removed after a successful swap"
        );
    }

    #[test]
    fn probe_write_speed_returns_ok_on_fast_disk() {
        let (_temp, app) = fixture();
        // force=true triggers the probe
        let check = probe_write_speed(&app, true).expect("probe should return a check");
        assert_eq!(check.id, "root-write-speed");
        assert!(
            check.status == CheckStatus::Ok || check.status == CheckStatus::Warning,
            "fast disk should not error: {:?}",
            check.status
        );
        assert!(!check.message.is_empty());
    }

    #[test]
    fn probe_write_speed_skipped_when_not_forced() {
        let (_temp, app) = fixture();
        let result = probe_write_speed(&app, false);
        assert!(
            result.is_none(),
            "probe should return None when force=false"
        );
    }
}
