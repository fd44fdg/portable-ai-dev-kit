import React, { useCallback, useEffect, useMemo, useState } from "react";
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
  RefreshCw,
  ShieldCheck,
  Terminal,
  Trash2,
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
  const [activeTool, setActiveTool] = useState<string>("codex");
  const [busyTool, setBusyTool] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>(["正在启动便携环境控制台..."]);
  const [startupError, setStartupError] = useState<string | null>(null);

  const appendLog = useCallback((message: string) => {
    setLogs((current) => [...current, message].slice(-50));
  }, []);

  const load = useCallback(async (silent = false) => {
    setStartupError(null);
    const next = await invoke<Dashboard>("bootstrap");
    setDashboard(next);
    setActiveTool((current) => next.tools.some((tool) => tool.id === current) ? current : next.tools[0]?.id ?? "");
    if (!silent) {
      appendLog(`已加载便携环境：${next.root}`);
    }
  }, [appendLog]);

  useEffect(() => {
    load().catch((error) => {
      const message = String(error);
      setStartupError(message);
      appendLog(message);
    });
  }, [appendLog, load]);

  const active = useMemo(
    () => dashboard?.tools.find((tool) => tool.id === activeTool) ?? dashboard?.tools[0],
    [dashboard, activeTool],
  );

  async function runAction(
    action: "install_tool" | "uninstall_tool" | "update_tool" | "launch_tool" | "login_tool",
    toolId: string,
  ) {
    setBusyTool(toolId);
    appendLog(`正在${actionLabel(action)}：${toolId}...`);
    try {
      const result = await invoke<ToolCommandResult>(action, { toolId });
      appendLog([result.message, result.output].filter(Boolean).join("\n\n"));
      await load(true);
    } catch (error) {
      appendLog(String(error));
    } finally {
      setBusyTool(null);
    }
  }

  if (!dashboard || !active) {
    return (
      <main className="shell loading-shell">
        <div className="aurora" />
        <section className="glass-panel loading-panel">
          {startupError ? <XCircle size={28} /> : <Activity className="spin" size={28} />}
          <div>
            <p>{startupError ? "便携环境加载失败" : "正在加载便携环境"}</p>
            {startupError && <small>{startupError}</small>}
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
          <button className="icon-button" onClick={() => load()} title="刷新状态">
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
                disabled={busyTool === active.id || active.status === "ready"}
                onClick={() => runAction("install_tool", active.id)}
              >
                <Download size={17} /> 安装
              </button>
              <button
                disabled={busyTool === active.id || active.status === "missing"}
                onClick={() => runAction("update_tool", active.id)}
              >
                <RefreshCw size={17} /> 更新
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
                <Trash2 size={17} /> 卸载
              </button>
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
          <pre>{logs.join("\n\n")}</pre>
        </section>
      </section>
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
