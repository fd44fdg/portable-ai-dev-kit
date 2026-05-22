import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  CheckCircle2,
  Download,
  ExternalLink,
  HardDrive,
  KeyRound,
  Play,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  Store,
  Terminal,
  Trash2,
  X,
  XCircle,
} from "lucide-react";
import "./styles.css";

type ToolKind = "runtime" | "ai-cli" | "app";
type ToolStatus = "ready" | "missing" | "partial";
type HealthSummary = "healthy" | "warning" | "unhealthy";
type CheckStatus = "ok" | "warning" | "error";

type ToolView = {
  id: string;
  name: string;
  kind: ToolKind;
  required: boolean;
  status: ToolStatus;
  installedVersion?: string;
  wantedVersion?: string;
  installSource: string;
  basePath: string;
  launchPath?: string;
  hostAvailable: boolean;
  hostVersion?: string;
  lastError?: string;
};

type HealthCheck = {
  id: string;
  label: string;
  status: CheckStatus;
  message: string;
};

type HealthReport = {
  summary: HealthSummary;
  checks: HealthCheck[];
};

type Dashboard = {
  root: string;
  workspace: string;
  networkMode: string;
  tools: ToolView[];
  health: HealthReport;
};

type ToolCommandResult = {
  toolId: string;
  action: string;
  success: boolean;
  message: string;
  output: string;
};

type MarketplaceTool = {
  id: string;
  name: string;
  description: string;
  packageName: string;
  category: string;
  homepage: string;
  inManifest: boolean;
  installed: boolean;
};

type AddCustomToolResult = {
  dashboard: Dashboard;
  newToolId: string;
};

function extractErrorMessage(error: unknown): string {
  if (error == null) return "未知错误";
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

type LogEntry = { ts: string; text: string };
const MAX_LOG_ENTRIES = 80;
function nowStamp(): string {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}:${String(d.getSeconds()).padStart(2, "0")}`;
}

const statusText: Record<ToolStatus, string> = {
  ready: "可用",
  missing: "未安装",
  partial: "不完整",
};

const kindText: Record<ToolKind, string> = {
  runtime: "运行时",
  "ai-cli": "AI 工具",
  app: "应用",
};

function App() {
  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [activeTool, setActiveTool] = useState<string>("");
  const [busyTool, setBusyTool] = useState<string | null>(null);
  const [logEntries, setLogEntries] = useState<LogEntry[]>([
    { ts: nowStamp(), text: "正在启动便携环境控制台..." },
  ]);
  const setLog = useCallback((text: string) => {
    if (!text) return;
    setLogEntries((prev) => {
      const next = [...prev, { ts: nowStamp(), text }];
      return next.length > MAX_LOG_ENTRIES ? next.slice(next.length - MAX_LOG_ENTRIES) : next;
    });
  }, []);
  const logText = useMemo(
    () => logEntries.map((entry) => `[${entry.ts}] ${entry.text}`).join("\n"),
    [logEntries],
  );
  const [startupError, setStartupError] = useState<string | null>(null);

  const [showAddModal, setShowAddModal] = useState<boolean>(false);
  const [customName, setCustomName] = useState<string>("");
  const [customPackage, setCustomPackage] = useState<string>("");
  const [addError, setAddError] = useState<string | null>(null);
  const [installType, setInstallType] = useState<"npm" | "powershell-script">("npm");
  const [customScriptUrl, setCustomScriptUrl] = useState<string>("");
  const [customBinName, setCustomBinName] = useState<string>("");

  const [showMarketplace, setShowMarketplace] = useState<boolean>(false);
  const [marketplaceTools, setMarketplaceTools] = useState<MarketplaceTool[]>([]);
  const [marketplaceLoading, setMarketplaceLoading] = useState<boolean>(false);
  const [marketplaceBusy, setMarketplaceBusy] = useState<string | null>(null);
  const [marketplaceSearch, setMarketplaceSearch] = useState<string>("");

  const isMountedRef = useRef(true);
  const bootstrapStartedRef = useRef(false);
  const runActionInFlightRef = useRef(false);
  useEffect(() => {
    return () => { isMountedRef.current = false; };
  }, []);

  const load = useCallback(async (keepLog = false, force = false) => {
    setStartupError(null);
    const next = await invoke<Dashboard>("bootstrap", { force });
    setDashboard(next);
    setActiveTool((current) => next.tools.some((tool) => tool.id === current) ? current : next.tools[0]?.id ?? "");
    if (!keepLog) {
      setLog(`已加载便携环境：${next.root}`);
    }
  }, []);

  useEffect(() => {
    if (bootstrapStartedRef.current) return;
    bootstrapStartedRef.current = true;
    load(false).catch((error) => {
      if (!isMountedRef.current) return;
      const message = extractErrorMessage(error);
      setStartupError(message);
      setLog(message);
    });
  }, [load]);

  const refresh = useCallback(async () => {
    setLog("正在刷新状态...");
    try {
      await load(true, true);
      setLog("状态已刷新");
    } catch (error) {
      setLog(`刷新失败: ${extractErrorMessage(error)}`);
    }
  }, [load]);

  const active = useMemo(
    () => dashboard?.tools.find((tool) => tool.id === activeTool) ?? dashboard?.tools[0],
    [dashboard, activeTool],
  );

  const runAction = useCallback(async (
    action: "install_tool" | "uninstall_tool" | "update_tool" | "launch_tool" | "login_tool",
    toolId: string,
  ) => {
    if (runActionInFlightRef.current) return;
    runActionInFlightRef.current = true;
    setBusyTool(toolId);
    setLog(`正在${actionLabel(action)}：${toolId}...`);
    try {
      const tool = dashboard?.tools.find((item) => item.id === toolId);
      let workspaceDir: string | null = null;
      if ((action === "launch_tool" || action === "login_tool") && tool?.kind === "ai-cli") {
        workspaceDir = await invoke<string | null>("select_workspace_dialog");
        if (workspaceDir === null || workspaceDir === undefined) {
          if (isMountedRef.current) setLog("已取消操作");
          return;
        }
      }
      const args =
        action === "launch_tool" || action === "login_tool"
          ? { toolId, workspaceDir }
          : { toolId };
      const result = await invoke<ToolCommandResult>(action, args);
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join("\n");
      if (combined) setLog(combined);
      await load(true);
    } catch (error) {
      if (isMountedRef.current) setLog(extractErrorMessage(error));
    } finally {
      runActionInFlightRef.current = false;
      if (isMountedRef.current) setBusyTool(null);
    }
  }, [dashboard, load]);

  useEffect(() => {
    if (!showAddModal && !showMarketplace) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (showAddModal) setShowAddModal(false);
        if (showMarketplace) { setShowMarketplace(false); setMarketplaceSearch(""); }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [showAddModal, showMarketplace]);

  const filteredMarketplaceTools = useMemo(
    () => marketplaceTools.filter(
      (t) =>
        !marketplaceSearch ||
        t.name.toLowerCase().includes(marketplaceSearch.toLowerCase()) ||
        t.description.toLowerCase().includes(marketplaceSearch.toLowerCase()),
    ),
    [marketplaceTools, marketplaceSearch],
  );

  async function openMarketplace() {
    setShowMarketplace(true);
    setMarketplaceSearch("");
    setMarketplaceTools([]);
    setMarketplaceLoading(true);
    try {
      const tools = await invoke<MarketplaceTool[]>("marketplace_tools");
      if (!isMountedRef.current) return;
      setMarketplaceTools(tools);
    } catch (error) {
      if (isMountedRef.current) setLog(extractErrorMessage(error));
    } finally {
      if (isMountedRef.current) setMarketplaceLoading(false);
    }
  }

  async function handleMarketplaceInstall(tool: MarketplaceTool) {
    setMarketplaceBusy(tool.id);
    setLog(`正在从市场安装：${tool.name}...`);
    try {
      const result = await invoke<ToolCommandResult>("install_marketplace_tool", {
        id: tool.id,
        name: tool.name,
        packageName: tool.packageName,
      });
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join("\n");
      if (combined) setLog(combined);
      if (result.success) {
        setMarketplaceTools((prev) =>
          prev.map((t) => (t.id === tool.id ? { ...t, installed: true } : t)),
        );
      }
      const tools = await invoke<MarketplaceTool[]>("marketplace_tools");
      if (!isMountedRef.current) return;
      setMarketplaceTools(tools);
      await load(true);
    } catch (error) {
      if (isMountedRef.current) setLog(extractErrorMessage(error));
    } finally {
      if (isMountedRef.current) setMarketplaceBusy(null);
    }
  }

  async function handleAddCustomTool(e: React.FormEvent) {
    e.preventDefault();
    let name = customName.trim();

    if (installType === "npm") {
      if (!customPackage.trim()) {
        setAddError("请输入 npm 包名");
        return;
      }
      if (!name) {
        const pkg = customPackage.trim();
        const tail = pkg.includes("/")
          ? pkg.split("/").pop() || pkg
          : pkg.replace(/^@[^/]+\//, "");
        name = (tail.split("@").filter(Boolean)[0] || "").trim() || "custom-tool";
      }
    } else {
      if (!customScriptUrl.trim()) {
        setAddError("请输入 PowerShell 脚本 URL");
        return;
      }
      if (!name) {
        const parts = customScriptUrl.trim().split('/');
        const last = parts[parts.length - 1];
        name = last.split('.')[0] || "custom-script";
        if (name.toLowerCase() === "install") {
          name = parts[parts.length - 2] || "custom-script";
        }
      }
    }

    setAddError(null);
    try {
      const result = await invoke<AddCustomToolResult>("add_custom_tool", {
        name,
        installType,
        packageName: installType === "npm" ? customPackage.trim() : null,
        scriptUrl: installType === "powershell-script" ? customScriptUrl.trim() : null,
        binName: installType === "powershell-script" ? customBinName.trim() || null : null,
      });
      if (!isMountedRef.current) return;
      setDashboard(result.dashboard);
      if (result.newToolId && result.dashboard.tools.some((t) => t.id === result.newToolId)) {
        setActiveTool(result.newToolId);
      }
      setShowAddModal(false);
      setCustomName("");
      setCustomPackage("");
      setCustomScriptUrl("");
      setCustomBinName("");
      setLog(`已成功添加自定义工具：${name}`);
    } catch (error) {
      if (isMountedRef.current) setAddError(extractErrorMessage(error));
    }
  }

  const logRef = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logText]);

  if (!dashboard || !active) {
    return (
      <main className="shell loading-shell">
        <div className="aurora" />
        <section className="glass-panel loading-panel">
          {startupError ? <XCircle size={28} /> : <Activity className="spin" size={28} />}
          <div>
            <p>{startupError ? "便携环境加载失败" : "正在加载便携环境"}</p>
            {startupError && <small>{startupError}</small>}
            {startupError && (
              <button
                type="button"
                className="primary"
                style={{ marginTop: 12 }}
                onClick={() => {
                  setStartupError(null);
                  setLog("正在重试加载便携环境...");
                  load(false).catch((error) => {
                    if (!isMountedRef.current) return;
                    const message = extractErrorMessage(error);
                    setStartupError(message);
                    setLog(message);
                  });
                }}
              >
                <RefreshCw size={17} /> 重试
              </button>
            )}
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className="shell">
      <div className="aurora" />
      <aside className="sidebar glass-panel">
        <div className="brand-lockup">
          <div className="brand-mark"><Terminal size={22} /></div>
          <div>
            <h1>Portable AI Dev Kit</h1>
            <p>Windows x64 便携控制台</p>
          </div>
        </div>

        <div className="root-card">
          <HardDrive size={18} />
          <div>
            <span>环境根目录</span>
            <strong title={dashboard.root}>{dashboard.root}</strong>
          </div>
        </div>

        <nav className="tool-nav" aria-label="工具列表">
          {dashboard.tools.map((tool) => (
            <button
              key={tool.id}
              className={tool.id === active.id ? "tool-tab active" : "tool-tab"}
              onClick={() => setActiveTool(tool.id)}
            >
              <span className={`status-dot ${tool.status}`} />
              <span>{tool.name}</span>
              <small>{kindText[tool.kind]}</small>
            </button>
          ))}
          <button className="add-tool-btn" onClick={() => { setShowAddModal(true); setAddError(null); }}>
            <Plus size={16} />
            添加自定义 AI 工具
          </button>
          <button className="add-tool-btn marketplace-btn" onClick={openMarketplace}>
            <Store size={16} />
            工具市场
          </button>
        </nav>

        <a className="deerflow-badge" href="https://deerflow.tech" target="_blank" rel="noreferrer">
          Created By Deerflow
        </a>
      </aside>

      <section className="content">
        <header className="topbar glass-strip">
          <div>
            <p className="eyebrow">网络模式：{networkModeText(dashboard.networkMode)}</p>
            <h2>{active.name}</h2>
          </div>
          <button className="icon-button" onClick={refresh} title="刷新状态">
            <RefreshCw size={18} />
          </button>
        </header>

        <section className="main-grid">
          <article className="tool-detail glass-panel">
            <div className="detail-head">
              <div>
                <p className="eyebrow">{kindText[active.kind]}</p>
                <h3>{active.name}</h3>
              </div>
              <span className={`pill ${active.status}`}>{statusText[active.status]}</span>
            </div>

            <dl className="facts">
              <div>
                <dt>已安装版本</dt>
                <dd>{active.installedVersion ?? "未检测到"}</dd>
              </div>
              <div>
                <dt>目标来源</dt>
                <dd>{active.wantedVersion ?? active.installSource}</dd>
              </div>
              <div>
                <dt>安装路径</dt>
                <dd title={active.basePath}>{active.basePath}</dd>
              </div>
              <div>
                <dt>启动文件</dt>
                <dd title={active.launchPath}>{active.launchPath ?? "未找到可执行入口"}</dd>
              </div>
              <div>
                <dt>宿主机检测</dt>
                <dd>{active.hostAvailable ? active.hostVersion ?? "本机可用" : "未检测到"}</dd>
              </div>
              <div>
                <dt>便携状态</dt>
                <dd>{portableStateText(active)}</dd>
              </div>
            </dl>

            <div className="actions">
              <button
                className="primary"
                disabled={busyTool === active.id}
                onClick={() => runAction("install_tool", active.id)}
              >
                {busyTool === active.id ? <Activity className="spin" size={17} /> : <Download size={17} />}
                {busyTool === active.id ? "安装中..." : "安装"}
              </button>
              <button
                disabled={busyTool === active.id || active.status === "missing"}
                onClick={() => runAction("update_tool", active.id)}
              >
                {busyTool === active.id ? <Activity className="spin" size={17} /> : <RefreshCw size={17} />}
                {busyTool === active.id ? "处理中..." : "更新"}
              </button>
              <button
                disabled={busyTool === active.id || active.status === "missing" || active.kind !== "ai-cli"}
                onClick={() => runAction("login_tool", active.id)}
              >
                <KeyRound size={17} /> 登录
              </button>
              <button
                disabled={busyTool === active.id || active.status === "missing" || active.kind !== "ai-cli"}
                onClick={() => runAction("launch_tool", active.id)}
              >
                <Play size={17} /> 运行
              </button>
              <button
                className="danger"
                disabled={busyTool === active.id || active.status === "missing"}
                onClick={() => runAction("uninstall_tool", active.id)}
              >
                {busyTool === active.id ? <Activity className="spin" size={17} /> : <Trash2 size={17} />}
                {busyTool === active.id ? "处理中..." : "卸载"}
              </button>
              {active.id.startsWith("custom-") && (
                <button
                  className="danger"
                  disabled={busyTool === active.id}
                  onClick={async () => {
                    let confirmed = false;
                    try {
                      confirmed = window.confirm(`确定要彻底删除自定义工具 "${active.name}" 吗？这也会自动物理清理其安装文件。`);
                    } catch {
                      confirmed = false;
                    }
                    if (!confirmed) return;
                    setBusyTool(active.id);
                    setLog(`正在删除自定义工具：${active.name}...`);
                    try {
                      const nextDashboard = await invoke<Dashboard>("delete_custom_tool", { toolId: active.id });
                      if (!isMountedRef.current) return;
                      setDashboard(nextDashboard);
                      setActiveTool(nextDashboard.tools[0]?.id ?? "");
                      setLog(`已成功删除自定义工具：${active.name}`);
                    } catch (error) {
                      if (isMountedRef.current) setLog(extractErrorMessage(error));
                    } finally {
                      if (isMountedRef.current) setBusyTool(null);
                    }
                  }}
                >
                  <Trash2 size={17} /> 删除
                </button>
              )}
            </div>
          </article>

          <article className="health glass-panel">
            <div className="detail-head">
              <div>
                <p className="eyebrow">就绪状态</p>
                <h3>健康检查</h3>
              </div>
              <HealthIcon summary={dashboard.health.summary} />
            </div>
            <div className="check-list">
              {dashboard.health.checks.map((check) => (
                <div className="check-row" key={check.id}>
                  {check.status === "ok" ? <CheckCircle2 size={17} /> : <XCircle size={17} />}
                  <div>
                    <strong>{check.label}</strong>
                    <span title={check.message}>{check.message}</span>
                  </div>
                </div>
              ))}
            </div>
          </article>
        </section>

        <section className="log-panel glass-panel">
          <div className="log-head">
            <span>操作日志</span>
            <ExternalLink size={16} />
          </div>
          <pre ref={logRef}>{logText}</pre>
        </section>
      </section>

      {showAddModal && (
        <div className="modal-overlay" onClick={() => setShowAddModal(false)}>
          <div className="modal-content glass-panel" onClick={(e) => e.stopPropagation()}>
            <h3>添加自定义 AI 工具</h3>
            
            <div className="modal-tabs">
              <button
                type="button"
                className={installType === "npm" ? "modal-tab active" : "modal-tab"}
                onClick={() => { setInstallType("npm"); setAddError(null); }}
              >
                NPM 包
              </button>
              <button
                type="button"
                className={installType === "powershell-script" ? "modal-tab active" : "modal-tab"}
                onClick={() => { setInstallType("powershell-script"); setAddError(null); }}
              >
                PowerShell 脚本
              </button>
            </div>

            <form onSubmit={handleAddCustomTool}>
              {installType === "npm" ? (
                <div className="form-group">
                  <label>npm 包名 (例如: freebuff)</label>
                  <input
                    type="text"
                    required
                    placeholder="e.g. freebuff"
                    value={customPackage}
                    onChange={(e) => { setCustomPackage(e.target.value); if (addError) setAddError(null); }}
                    autoFocus
                  />
                </div>
              ) : (
                <>
                  <div className="form-group">
                    <label>PowerShell 脚本 URL (安装脚本地址)</label>
                    <input
                      type="text"
                      required
                      placeholder="https://example.com/install.ps1"
                      value={customScriptUrl}
                      onChange={(e) => { setCustomScriptUrl(e.target.value); if (addError) setAddError(null); }}
                      autoFocus
                    />
                  </div>
                  <div className="form-group">
                    <label>可执行二进制文件名 (可选，例如: agy.exe)</label>
                    <input
                      type="text"
                      placeholder="留空则自动从 URL / 包名推断"
                      value={customBinName}
                      onChange={(e) => { setCustomBinName(e.target.value); if (addError) setAddError(null); }}
                    />
                  </div>
                </>
              )}
              
              <div className="form-group">
                <label>工具名称 (可选，留空则自动推断)</label>
                <input
                  type="text"
                  placeholder="e.g. Antigravity"
                  value={customName}
                  onChange={(e) => { setCustomName(e.target.value); if (addError) setAddError(null); }}
                />
              </div>

              {addError && (
                <div style={{ color: "#ffc2b9", fontSize: "13px", marginTop: "8px" }}>
                  {addError}
                </div>
              )}
              <div className="modal-actions">
                <button type="button" className="btn-cancel" onClick={() => setShowAddModal(false)}>
                  取消
                </button>
                <button type="submit" className="btn-confirm">
                  确定
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {showMarketplace && (
        <div className="modal-overlay" onClick={() => { setShowMarketplace(false); setMarketplaceSearch(""); }}>
          <div className="modal-content marketplace-modal glass-panel" onClick={(e) => e.stopPropagation()}>
            <div className="marketplace-topbar">
              <h3>
                <Store size={20} />
                工具市场
              </h3>
              <div className="marketplace-search">
                <Search size={16} />
                <input
                  type="text"
                  placeholder="搜索工具..."
                  value={marketplaceSearch}
                  onChange={(e) => setMarketplaceSearch(e.target.value)}
                />
              </div>
              <button
                className="icon-button marketplace-close"
                onClick={() => { setShowMarketplace(false); setMarketplaceSearch(""); }}
              >
                <X size={18} />
              </button>
            </div>

            {marketplaceLoading ? (
              <div className="marketplace-loading">
                <Activity className="spin" size={24} />
                <span>加载工具列表...</span>
              </div>
            ) : filteredMarketplaceTools.length === 0 ? (
              <div className="marketplace-empty">
                <Search size={32} />
                <p>未找到匹配的工具</p>
              </div>
            ) : (
              <div className="marketplace-grid">
                {filteredMarketplaceTools.map((tool) => (
                  <div className={`marketplace-card ${tool.installed ? "installed" : ""}`} key={tool.id}>
                    <div className="marketplace-card-top">
                      <div className="marketplace-card-icon">
                        {tool.name.charAt(0).toUpperCase()}
                      </div>
                      <div className="marketplace-card-info">
                        <h4>{tool.name}</h4>
                        <span className="marketplace-category">{tool.category}</span>
                      </div>
                    </div>
                    <p className="marketplace-desc">{tool.description}</p>
                    <div className="marketplace-card-actions">
                      <a
                        href={tool.homepage}
                        target="_blank"
                        rel="noreferrer"
                        className="marketplace-link"
                        title="访问主页"
                      >
                        <ExternalLink size={15} />
                      </a>
                      <div className="marketplace-card-buttons">
                        {tool.inManifest && (
                          <span className="marketplace-manifest-badge" title="此工具已在预设清单中，可在侧边栏管理">
                            预设
                          </span>
                        )}
                        {tool.installed ? (
                          <span className="marketplace-installed-badge">
                            <CheckCircle2 size={15} />
                            已安装
                          </span>
                        ) : (
                          <button
                            className="primary"
                            disabled={marketplaceBusy === tool.id}
                            onClick={() => handleMarketplaceInstall(tool)}
                          >
                            {marketplaceBusy === tool.id ? (
                              <><Activity className="spin" size={15} /> 安装中</>
                            ) : (
                              <><Download size={15} /> 安装</>
                            )}
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </main>
  );
}

function HealthIcon({ summary }: { summary: HealthSummary }) {
  if (summary === "healthy") {
    return <ShieldCheck className="health-icon ok" size={30} />;
  }
  if (summary === "warning") {
    return <Activity className="health-icon warn" size={30} />;
  }
  return <XCircle className="health-icon error" size={30} />;
}

function actionLabel(action: string) {
  const labels: Record<string, string> = {
    install_tool: "安装",
    uninstall_tool: "卸载",
    update_tool: "更新",
    launch_tool: "运行",
    login_tool: "登录",
  };
  return labels[action] ?? action;
}

function networkModeText(mode: string) {
  if (mode === "china") {
    return "国内镜像";
  }
  if (mode === "global") {
    return "国际源";
  }
  return mode;
}

function portableStateText(tool: ToolView) {
  if (tool.status === "ready") {
    return "已安装在当前移动盘";
  }
  if (tool.hostAvailable) {
    return "仅宿主机可用，换电脑后不可依赖";
  }
  return "需要安装到当前移动盘";
}

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
