use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

static STATE_LOCK: Mutex<()> = Mutex::new(());
static BOOTSTRAPPED_ROOTS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

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
    // Skip the 14 create_dir_all + state-file check if we've already
    // bootstrapped this root in the current process. Dashboard refresh
    // triggers bootstrap_kit every call, which is visibly slow on network
    // drives.
    {
        let cache = BOOTSTRAPPED_ROOTS
            .lock()
            .unwrap_or_else(|p| p.into_inner());
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
        network_mode: settings.network_mode,
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
) -> Result<ToolCommandResult, AppError> {
    let mut state = load_state(app)?;
    for dependency in &tool.install.depends_on {
        let dep = find_tool(manifest, dependency)?;
        if tool_view(app, dep, &mut state, false)?.status != ToolStatus::Ready {
            return Err(AppError::Message(format!(
                "{} 依赖 {}，请先安装依赖项。",
                tool.name, dep.name
            )));
        }
    }
    save_state(app, &state)?;

    match tool.install.install_type {
        InstallType::Npm => install_npm_tool(app, settings, tool),
        InstallType::Archive => install_archive_tool(app, manifest, settings, tool),
        InstallType::PowershellScript => install_powershell_script_tool(app, tool),
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

    let mut command = if npm
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("js"))
        .unwrap_or(false)
    {
        // npm-cli.js is a JS file — Windows can't execute it directly.
        // Use the bundled node.exe to run it instead.
        let node_exe = find_existing_path(&node_root, &["node.exe"])
            .ok_or_else(|| AppError::Message("Node 可执行文件未找到".to_string()))?;
        let mut cmd = Command::new(node_exe);
        cmd.arg(&npm);
        cmd
    } else {
        Command::new(&npm)
    };
    command
        .arg("install")
        .arg("--prefix")
        .arg(display_path(&tool_root))
        .arg(package_name)
        .arg("--no-fund")
        .arg("--no-audit")
        .arg("--registry")
        .arg(&registry)
        .current_dir(display_path(&tool_root));
    apply_portable_env(app, &mut command);
    prepend_path(&mut command, &node_root);

    let output = command.output()?;
    let mut combined = command_output(&output);
    let success = output.status.success();
    if success && package_name == "freebuff" {
        if let Some(patch_note) = patch_freebuff_index(app, tool)? {
            if !combined.is_empty() {
                combined.push_str("\n\n");
            }
            combined.push_str(&patch_note);
        }
    }
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

fn patch_freebuff_index(app: &AppState, tool: &ToolDefinition) -> Result<Option<String>, AppError> {
    let index_path = app
        .path(&tool.base_path)
        .join("node_modules/freebuff/index.js");
    if !index_path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&index_path)?;
    if raw.contains("Portable AI Dev Kit stream patch") {
        return Ok(Some(format!(
            "freebuff stream pipeline patch already present: {}",
            display_path(&index_path)
        )));
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
    fs::write(&index_path, patched)?;

    Ok(Some(format!(
        "freebuff stream pipeline patched: {}",
        display_path(&index_path)
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
        let output = download.output()?;
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
            escape_single_quote(&display_path(&download_path)),
            escape_single_quote(&display_path(&destination))
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
    let download_output = download.output()?;
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

    // Execute the PowerShell script with argument `--dir <destination>`
    let mut run = Command::new("powershell.exe");
    run.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(display_path(&script_path))
        .arg("--dir")
        .arg(display_path(&destination));

    apply_portable_env(app, &mut run);

    let run_output = run.output()?;
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
    let status = if launch.is_some() {
        ToolStatus::Ready
    } else if base.exists() {
        ToolStatus::Partial
    } else {
        ToolStatus::Missing
    };
    let mut persisted = state.tools.get(&tool.id).cloned().unwrap_or_default();
    let detected_version = if status == ToolStatus::Ready {
        if force || persisted.installed_version.is_none() {
            let detected = detect_version(app, tool);
            persisted.installed_version = detected.clone();
            detected
        } else {
            persisted.installed_version.clone()
        }
    } else {
        None
    };
    let host_version = if tool.host_version_command.is_empty() {
        None
    } else if force || persisted.host_version.is_none() {
        let detected = detect_host_version(tool);
        persisted.host_version = detected.clone();
        detected
    } else {
        persisted.host_version.clone()
    };
    let host_available = host_version.is_some();
    state.tools.insert(tool.id.clone(), persisted.clone());

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

fn run_command_with_timeout(
    mut command: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::io::Read;
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
                let mut local = Vec::new();
                let _ = pipe.read_to_end(&mut local);
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
                let mut local = Vec::new();
                let _ = pipe.read_to_end(&mut local);
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
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };

    if let Some(handle) = stdout_thread {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_thread {
        let _ = handle.join();
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
    let _state = load_state(app)?;

    let all_tools = vec![
        MarketplaceTool {
            id: "codebuff".to_string(),
            name: "Codebuff".to_string(),
            description: "AI驱动的对话式编程助手，在终端中通过自然语言与AI协作编码".to_string(),
            package_name: "codebuff".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://codebuff.com".to_string(),
            in_manifest: false,
            installed: false,
        },
        MarketplaceTool {
            id: "freebuff".to_string(),
            name: "freebuff".to_string(),
            description: "免费开源的AI CLI工具箱，支持多种AI模型的命令行交互".to_string(),
            package_name: "freebuff".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://deerflow.tech".to_string(),
            in_manifest: false,
            installed: false,
        },
        MarketplaceTool {
            id: "github-copilot".to_string(),
            name: "GitHub Copilot CLI".to_string(),
            description: "GitHub官方AI命令行助手，在终端中获取AI驱动的代码建议和解释".to_string(),
            package_name: "@githubnext/github-copilot-cli".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://github.com/githubnext/github-copilot-cli".to_string(),
            in_manifest: false,
            installed: false,
        },
        MarketplaceTool {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            description: "Anthropic 官方AI编程助手（已在工具清单中）".to_string(),
            package_name: "@anthropic-ai/claude-code".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://docs.anthropic.com/en/docs/claude-code".to_string(),
            in_manifest: true,
            installed: false,
        },
        MarketplaceTool {
            id: "codex".to_string(),
            name: "Codex CLI".to_string(),
            description: "OpenAI 官方AI编程助手（已在工具清单中）".to_string(),
            package_name: "@openai/codex".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://openai.com/index/codex-cli".to_string(),
            in_manifest: true,
            installed: false,
        },
        MarketplaceTool {
            id: "antigravity".to_string(),
            name: "Antigravity CLI".to_string(),
            description: "Google 官方AI CLI工具（已在工具清单中）".to_string(),
            package_name: "antigravity".to_string(),
            category: "AI CLI".to_string(),
            homepage: "https://antigravity.google".to_string(),
            in_manifest: true,
            installed: false,
        },
    ];

    let result_tools: Vec<MarketplaceTool> = all_tools
        .into_iter()
        .map(|mut tool| {
            let custom_id = format!("custom-{}", tool.id);
            // Check if installed via manifest (built-in) or custom-tools
            let in_manifest = manifest.tools.iter().any(|t| t.id == tool.id)
                || manifest.tools.iter().any(|t| t.id == custom_id);
            tool.in_manifest = in_manifest;
            // Check if installed via manifest tool status
            let is_installed = manifest.tools.iter().any(|t| {
                (t.id == tool.id || t.id == custom_id)
                    && app.path(&t.base_path).exists()
                    && t.bin_paths
                        .iter()
                        .any(|bin| app.path(&t.base_path).join(bin.replace('/', "\\")).exists())
            });
            tool.installed = is_installed;
            tool
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
        return tool_action(app, ToolActionRequest::new(tool.id.clone(), "install"));
    }

    // Not in manifest, add as custom tool first, then install
    add_custom_tool(
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
        return install_tool(app, &updated_manifest, &settings, tool);
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
    cmd.output()
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

fn host_system_path() -> Option<String> {
    // PowerShell to read the Machine + User PATH (i.e. the host's PATH as seen
    // by a freshly spawned cmd window), so portable PATH prepends do not leak.
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + \
             [Environment]::GetEnvironmentVariable('Path','User')",
        )
        .output()
        .ok()?;
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

    Ok(manifest)
}

fn load_settings(app: &AppState) -> Result<Settings, AppError> {
    let path = app.path(SETTINGS_PATH);
    if !path.exists() {
        return Ok(Settings::default());
    }
    let mut settings: Settings = serde_json::from_str(&fs::read_to_string(path)?)?;
    settings.workspace_path = sanitize_relative_path(&settings.workspace_path, "workspace");
    Ok(settings)
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

fn load_state(app: &AppState) -> Result<ToolStateFile, AppError> {
    let _guard = STATE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
    let _guard = STATE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
                if child
                    .file_type()
                    .map(|t| t.is_symlink())
                    .unwrap_or(false)
                {
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
        .env("XDG_CONFIG_HOME", display_path(&app.path("state/xdg/config")))
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
    format!("{}{}", stdout, stderr).trim().to_string()
}

fn quote_cmd_arg(input: &str) -> String {
    // cmd.exe metacharacters that need protection when an argument is
    // splatted into a .bat file.
    const CMD_META: &[char] = &[' ', '\t', '"', '&', '|', '<', '>', '^', '(', ')', '%', '!', ';', ',', '`'];
    if input.is_empty() {
        return "\"\"".to_string();
    }
    if input.chars().any(|c| CMD_META.contains(&c)) {
        let escaped = input.replace('"', "\\\"");
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
        if name_str == ".." || name_str == "." || name_str.contains('/') || name_str.contains('\\') {
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

pub fn add_custom_tool(
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
        return Err(AppError::Message("工具名称过长（最多 64 字符）".to_string()));
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
            id_name = format!("tool-{:x}", std::hash::Hasher::finish(&hasher) & 0xffff_ffff);
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
        let url_lower = script_url_val.to_lowercase();
        let is_local_http = url_lower.starts_with("http://localhost")
            || url_lower.starts_with("http://127.0.0.1");
        if !(url_lower.starts_with("https://") || is_local_http) {
            return Err(AppError::Message(
                "脚本 URL 必须使用 HTTPS（仅 localhost / 127.0.0.1 允许 HTTP）".to_string(),
            ));
        }

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
            },
        }
    } else {
        let package_name_val =
            package_name.ok_or_else(|| AppError::Message("NPM 包名不能为空".to_string()))?;
        let package_name_val = package_name_val.trim().to_string();
        if package_name_val.is_empty() {
            return Err(AppError::Message("NPM 包名不能为空".to_string()));
        }

        let bin_path = format!("node_modules/.bin/{}.cmd", id_name);

        ToolDefinition {
            id: tool_id,
            name,
            kind: ToolKind::AiCli,
            required: false,
            base_path,
            package_name: Some(package_name_val),
            version_command: vec![bin_path.clone(), "--version".to_string()],
            host_version_command: vec![id_name.clone(), "--version".to_string()],
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
        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools.len(), 1);
        assert_eq!(dashboard.tools[0].status, ToolStatus::Missing);
        assert_eq!(dashboard.health.summary, HealthSummary::Warning);
    }

    #[test]
    fn dashboard_reports_ready_tool_when_binary_exists() {
        let (_temp, app) = fixture();
        fs::create_dir_all(app.path("apps/node")).unwrap();
        fs::write(app.path("apps/node/node.exe"), "").unwrap();
        let dashboard = get_dashboard(&app, false).unwrap();
        assert_eq!(dashboard.tools[0].status, ToolStatus::Ready);
    }

    #[test]
    fn display_path_removes_windows_extended_prefix() {
        let path = PathBuf::from(r"\\?\E:\kit\config");
        assert_eq!(display_path(&path), r"E:\kit\config");
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
