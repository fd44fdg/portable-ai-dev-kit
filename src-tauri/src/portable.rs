use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const MANIFEST_PATH: &str = "config/tool-manifest.json";
const SETTINGS_PATH: &str = "config/app-settings.json";
const STATE_PATH: &str = "state/tool-state.json";

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

        let candidates = [
            env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(Path::to_path_buf)),
            env::current_dir().ok(),
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")),
        ];

        for candidate in candidates.into_iter().flatten() {
            if let Some(root) = find_manifest_root(&candidate) {
                return Ok(Self {
                    root: normalize_path(root)?,
                });
            }
        }

        Ok(Self {
            root: normalize_path(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".."))?,
        })
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
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallType {
    Npm,
    Archive,
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
    pub network_mode: String,
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

pub struct ToolActionRequest {
    tool_id: String,
    action: String,
}

impl ToolActionRequest {
    pub fn new(tool_id: String, action: &str) -> Self {
        Self {
            tool_id,
            action: action.to_string(),
        }
    }
}

pub fn bootstrap_kit(app: &AppState) -> Result<(), AppError> {
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

    Ok(())
}

pub fn get_dashboard(app: &AppState) -> Result<Dashboard, AppError> {
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let state = load_state(app)?;
    let tools = manifest
        .tools
        .iter()
        .map(|tool| tool_view(app, tool, &state))
        .collect::<Result<Vec<_>, _>>()?;
    let health = check_health(app)?;

    Ok(Dashboard {
        root: display_path(&app.root),
        workspace: display_path(&app.path(&settings.workspace_path)),
        network_mode: settings.network_mode,
        tools,
        health,
    })
}

pub fn check_health(app: &AppState) -> Result<HealthReport, AppError> {
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let state = load_state(app)?;
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
        "workspace",
        "工作目录",
        &app.path(&settings.workspace_path),
        false,
    );

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

    for tool in &manifest.tools {
        let view = tool_view(app, tool, &state)?;
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
    bootstrap_kit(app)?;
    let manifest = load_manifest(app)?;
    let settings = load_settings(app)?;
    let tool = find_tool(&manifest, &request.tool_id)?;

    match request.action.as_str() {
        "install" => install_tool(app, &manifest, &settings, tool),
        "update" => install_tool(app, &manifest, &settings, tool),
        "uninstall" => uninstall_tool(app, tool),
        _ => Err(AppError::Message(format!(
            "未知工具操作：{}",
            request.action
        ))),
    }
}

pub fn run_tool(app: &AppState, tool_id: &str) -> Result<ToolCommandResult, AppError> {
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

    spawn_terminal_command(app, tool, &command, "运行")?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "launch".to_string(),
        success: true,
        message: format!("已在新终端中启动 {}", tool.name),
        output: String::new(),
    })
}

pub fn login_tool(app: &AppState, tool_id: &str) -> Result<ToolCommandResult, AppError> {
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

    spawn_terminal_command(app, tool, &command, "登录")?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "login".to_string(),
        success: true,
        message: format!("已在新终端中打开 {} 登录流程", tool.name),
        output: String::new(),
    })
}

fn spawn_terminal_command(
    app: &AppState,
    tool: &ToolDefinition,
    command: &[String],
    purpose: &str,
) -> Result<(), AppError> {
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
    let workspace = app.path("workspace");
    let mut cmd = Command::new("cmd.exe");
    cmd.arg("/K")
        .arg(format!(
            "cd /d \"{}\" && \"{}\" {}",
            workspace.display(),
            exe.display(),
            args
        ))
        .current_dir(&workspace);
    apply_portable_env(app, &mut cmd);
    cmd.spawn().map_err(|error| {
        AppError::Message(format!(
            "无法为 {} 打开 {} 终端：{}",
            tool.name, purpose, error
        ))
    })?;
    Ok(())
}

fn install_tool(
    app: &AppState,
    manifest: &Manifest,
    settings: &Settings,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    for dependency in &tool.install.depends_on {
        let dep = find_tool(manifest, dependency)?;
        if tool_view(app, dep, &load_state(app)?)?.status != ToolStatus::Ready {
            return Err(AppError::Message(format!(
                "{} 依赖 {}，请先安装依赖项。",
                tool.name, dep.name
            )));
        }
    }

    match tool.install.install_type {
        InstallType::Npm => install_npm_tool(app, settings, tool),
        InstallType::Archive => install_archive_tool(app, manifest, settings, tool),
    }
}

fn install_npm_tool(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    let node_root = app.path("apps/node");
    let npm = find_existing_path(&node_root, &["npm.cmd", "node_modules/npm/bin/npm-cli.js"])
        .ok_or_else(|| AppError::Message("Node/npm 尚未安装".to_string()))?;
    let package_name = tool
        .package_name
        .as_ref()
        .ok_or_else(|| AppError::Message(format!("{} 未配置 npm 包", tool.name)))?;
    let registry = resolve_registry(app, settings)?;
    let tool_root = app.path(&tool.base_path);
    fs::create_dir_all(&tool_root)?;

    if !tool_root.join("package.json").exists() {
        fs::write(
            tool_root.join("package.json"),
            "{\"name\":\"portable-ai-tool\",\"private\":true}\n",
        )?;
    }

    let mut command = Command::new(&npm);
    command
        .arg("install")
        .arg("--prefix")
        .arg(&tool_root)
        .arg(package_name)
        .arg("--no-fund")
        .arg("--no-audit")
        .arg("--registry")
        .arg(&registry)
        .current_dir(&tool_root);
    apply_portable_env(app, &mut command);
    prepend_path(&mut command, &node_root);

    let output = command.output()?;
    let combined = command_output(&output);
    let success = output.status.success();
    persist_action_state(
        app,
        tool,
        success,
        Some(package_name.to_string()),
        &combined,
    )?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "install".to_string(),
        success,
        message: if success {
            format!("{} 已安装", tool.name)
        } else {
            format!("{} 安装失败", tool.name)
        },
        output: combined,
    })
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
    fs::create_dir_all(download_path.parent().unwrap_or(&app.root))?;

    if !download_path.exists() {
        let mut download = Command::new("powershell.exe");
        download
            .arg("-NoProfile")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                escape_single_quote(url),
                escape_single_quote(&powershell_path(&download_path))
            ));
        let output = download.output()?;
        if !output.status.success() {
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

    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::create_dir_all(&destination)?;

    let mut expand = Command::new("powershell.exe");
    expand
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            escape_single_quote(&powershell_path(&download_path)),
            escape_single_quote(&powershell_path(&destination))
        ));
    let output = expand.output()?;
    flatten_single_root(&destination)?;
    let combined = command_output(&output);
    let success = output.status.success();
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
    state: &ToolStateFile,
) -> Result<ToolView, AppError> {
    let base = app.path(&tool.base_path);
    let launch = find_existing_path(&base, &tool.bin_paths);
    let status = if launch.is_some() {
        ToolStatus::Ready
    } else if base.exists() {
        ToolStatus::Partial
    } else {
        ToolStatus::Missing
    };
    let persisted = state.tools.get(&tool.id).cloned().unwrap_or_default();
    let detected_version = if status == ToolStatus::Ready {
        detect_version(app, tool)
    } else {
        None
    };
    let host_version = detect_host_version(tool);
    let host_available = host_version.is_some();

    Ok(ToolView {
        id: tool.id.clone(),
        name: tool.name.clone(),
        kind: tool.kind.clone(),
        required: tool.required,
        status,
        installed_version: detected_version.or(persisted.installed_version),
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

fn detect_version(app: &AppState, tool: &ToolDefinition) -> Option<String> {
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
    command
        .output()
        .ok()
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
    host_executable_path(executable)?;

    let mut command = Command::new(executable);
    for arg in tool.host_version_command.iter().skip(1) {
        command.arg(arg);
    }

    command
        .output()
        .ok()
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

fn host_executable_path(executable: &str) -> Option<String> {
    Command::new("where.exe")
        .arg(executable)
        .output()
        .ok()
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

fn load_manifest(app: &AppState) -> Result<Manifest, AppError> {
    let raw = fs::read_to_string(app.path(MANIFEST_PATH))?;
    Ok(serde_json::from_str(&raw)?)
}

fn load_settings(app: &AppState) -> Result<Settings, AppError> {
    let path = app.path(SETTINGS_PATH);
    if !path.exists() {
        return Ok(Settings::default());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn load_state(app: &AppState) -> Result<ToolStateFile, AppError> {
    let path = app.path(STATE_PATH);
    if !path.exists() {
        return Ok(ToolStateFile::default());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn save_state(app: &AppState, state: &ToolStateFile) -> Result<(), AppError> {
    fs::create_dir_all(app.path("state"))?;
    fs::write(app.path(STATE_PATH), serde_json::to_string_pretty(state)?)?;
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

fn powershell_path(path: &Path) -> String {
    display_path(path)
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
    command
        .env("HOME", app.path("state/home"))
        .env("USERPROFILE", app.path("state/home"))
        .env("APPDATA", app.path("state/appdata"))
        .env("LOCALAPPDATA", app.path("state/localappdata"))
        .env("XDG_CONFIG_HOME", app.path("state/xdg/config"))
        .env("XDG_CACHE_HOME", app.path("state/xdg/cache"))
        .env("XDG_STATE_HOME", app.path("state/xdg/state"));
}

fn prepend_path(command: &mut Command, path: &Path) {
    let original = env::var("PATH").unwrap_or_default();
    command.env("PATH", format!("{};{}", path.display(), original));
}

fn command_output(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    format!("{}{}", stdout, stderr).trim().to_string()
}

fn quote_cmd_arg(input: &str) -> String {
    if input.is_empty() || input.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", input.replace('"', "\\\""))
    } else {
        input.to_string()
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
    for child in fs::read_dir(&nested)?.flatten() {
        fs::rename(child.path(), destination.join(child.file_name()))?;
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

fn status_label(status: &ToolStatus) -> &'static str {
    match status {
        ToolStatus::Ready => "可用",
        ToolStatus::Missing => "未安装",
        ToolStatus::Partial => "不完整",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let app = AppState {
            root: temp.path().to_path_buf(),
        };
        (temp, app)
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
        let dashboard = get_dashboard(&app).unwrap();
        assert_eq!(dashboard.tools.len(), 1);
        assert_eq!(dashboard.tools[0].status, ToolStatus::Missing);
        assert_eq!(dashboard.health.summary, HealthSummary::Warning);
    }

    #[test]
    fn dashboard_reports_ready_tool_when_binary_exists() {
        let (_temp, app) = fixture();
        fs::create_dir_all(app.path("apps/node")).unwrap();
        fs::write(app.path("apps/node/node.exe"), "").unwrap();
        let dashboard = get_dashboard(&app).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Ready);
    }

    #[test]
    fn display_path_removes_windows_extended_prefix() {
        let path = PathBuf::from(r"\\?\E:\kit\config");
        assert_eq!(display_path(&path), r"E:\kit\config");
    }

    #[test]
    fn powershell_path_removes_windows_extended_prefix() {
        let path = PathBuf::from(r"\\?\F:\BXAI\cache\downloads\node.zip");
        assert_eq!(powershell_path(&path), r"F:\BXAI\cache\downloads\node.zip");
    }

    #[test]
    fn discover_returns_manifest_root_not_nested_candidate() {
        let (temp, _app) = fixture();
        let nested = temp.path().join("src-tauri").join("target").join("release");
        fs::create_dir_all(&nested).unwrap();
        let found = find_manifest_root(&nested).unwrap();
        assert_eq!(found, temp.path());
    }
}
