import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import {
  Activity,
  CheckCircle2,
  Download,
  AlertTriangle,
  ExternalLink,
  FileText,
  HardDrive,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  ShieldCheck,
  Store,
  Terminal,
  Trash2,
  X,
  XCircle,
} from 'lucide-react';
import './i18n';
import './styles.css';

type ToolKind = 'runtime' | 'ai-cli' | 'app';
type ToolStatus = 'ready' | 'missing' | 'partial';
type HealthSummary = 'healthy' | 'warning' | 'unhealthy';
type CheckStatus = 'ok' | 'warning' | 'error';

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
  workspacePath: string;
  networkMode: string;
  autoOpenWorkspace: boolean;
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

type DiagnosticsReport = {
  path: string;
};

type SettingsValues = {
  networkMode: string;
  workspacePath: string;
  autoOpenWorkspace: boolean;
};

function extractErrorMessage(error: unknown, t: (key: string) => string): string {
  if (error == null) return t('unknownError');
  const e = error as { message?: string };
  if (e instanceof Error) return e.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

type LogEntry = { ts: string; text: string };
type ToastKind = 'error' | 'success' | 'info';
type Toast = { id: number; message: string; kind: ToastKind };
const MAX_LOG_ENTRIES = 80;
function nowStamp(): string {
  const d = new Date();
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`;
}

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]):not([type=hidden]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex=-1])';
const BOOTSTRAP_TIMEOUT_MS = 120000;

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timeout = window.setTimeout(() => reject(new Error(message)), timeoutMs);
    promise.then(resolve, reject).finally(() => window.clearTimeout(timeout));
  });
}

function useFocusTrap(open: boolean) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const node = ref.current;
    if (!node) return;
    const previouslyFocused = document.activeElement as HTMLElement | null;

    const queryFocusables = () =>
      Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
        (el) => el.offsetParent !== null || el === document.activeElement,
      );

    const initialFocusTimer = window.setTimeout(() => {
      if (!node.contains(document.activeElement)) {
        queryFocusables()[0]?.focus();
      }
    }, 0);

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') return;
      const list = queryFocusables();
      if (list.length === 0) {
        event.preventDefault();
        return;
      }
      const first = list[0];
      const last = list[list.length - 1];
      const active = document.activeElement;
      if (event.shiftKey) {
        if (active === first || !node.contains(active)) {
          event.preventDefault();
          last.focus();
        }
      } else if (active === last || !node.contains(active)) {
        event.preventDefault();
        first.focus();
      }
    };

    node.addEventListener('keydown', onKeyDown);
    return () => {
      window.clearTimeout(initialFocusTimer);
      node.removeEventListener('keydown', onKeyDown);
      if (previouslyFocused && typeof previouslyFocused.focus === 'function') {
        previouslyFocused.focus();
      }
    };
  }, [open]);
  return ref;
}

function App() {
  const { t, i18n } = useTranslation();

  const [dashboard, setDashboard] = useState<Dashboard | null>(null);
  const [activeTool, setActiveTool] = useState<string>('');
  const [busyTool, setBusyTool] = useState<string | null>(null);
  const [logEntries, setLogEntries] = useState<LogEntry[]>([
    { ts: nowStamp(), text: t('loadingApp') },
  ]);
  const setLog = useCallback((text: string) => {
    if (!text) return;
    setLogEntries((prev) => {
      const next = [...prev, { ts: nowStamp(), text }];
      return next.length > MAX_LOG_ENTRIES ? next.slice(next.length - MAX_LOG_ENTRIES) : next;
    });
  }, [t]);
  const logText = useMemo(
    () => logEntries.map((entry) => `[${entry.ts}] ${entry.text}`).join('\n'),
    [logEntries],
  );
  const [startupError, setStartupError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState<boolean>(false);
  const [exportingDiagnostics, setExportingDiagnostics] = useState<boolean>(false);

  const [toasts, setToasts] = useState<Toast[]>([]);
  const toastTimers = useRef<Map<number, number>>(new Map());
  const dismissToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((toast) => toast.id !== id));
    const handle = toastTimers.current.get(id);
    if (handle !== undefined) {
      window.clearTimeout(handle);
      toastTimers.current.delete(id);
    }
  }, []);
  const pushToast = useCallback(
    (message: string, kind: ToastKind = 'error') => {
      if (!message) return;
      const id = Date.now() + Math.floor(Math.random() * 1000);
      setToasts((prev) => [...prev, { id, message, kind }]);
      const handle = window.setTimeout(() => dismissToast(id), 3000);
      toastTimers.current.set(id, handle);
    },
    [dismissToast],
  );
  useEffect(
    () => () => {
      toastTimers.current.forEach((handle) => window.clearTimeout(handle));
      toastTimers.current.clear();
    },
    [],
  );

  // Settings modal
  const [showSettings, setShowSettings] = useState<boolean>(false);
  const [settingsValues, setSettingsValues] = useState<SettingsValues>({
    networkMode: 'global',
    workspacePath: 'workspace',
    autoOpenWorkspace: false,
  });
  const [settingsSaving, setSettingsSaving] = useState<boolean>(false);
  const settingsModalRef = useFocusTrap(showSettings);

  // Add custom tool modal
  const [showAddModal, setShowAddModal] = useState<boolean>(false);
  const [customName, setCustomName] = useState<string>('');
  const [customPackage, setCustomPackage] = useState<string>('');
  const [addError, setAddError] = useState<string | null>(null);
  const [installType, setInstallType] = useState<'npm' | 'powershell-script'>('npm');
  const [customScriptUrl, setCustomScriptUrl] = useState<string>('');
  const [customBinName, setCustomBinName] = useState<string>('');
  const addModalRef = useFocusTrap(showAddModal);

  // Marketplace modal
  const [showMarketplace, setShowMarketplace] = useState<boolean>(false);
  const [marketplaceTools, setMarketplaceTools] = useState<MarketplaceTool[]>([]);
  const [marketplaceLoading, setMarketplaceLoading] = useState<boolean>(false);
  const [marketplaceBusy, setMarketplaceBusy] = useState<string | null>(null);
  const [marketplaceSearch, setMarketplaceSearch] = useState<string>('');
  const marketplaceModalRef = useFocusTrap(showMarketplace);

  const isMountedRef = useRef(true);
  const bootstrapStartedRef = useRef(false);
  const runActionInFlightRef = useRef(false);
  useEffect(() => {
    return () => { isMountedRef.current = false; };
  }, []);

  const load = useCallback(async (keepLog = false, force = false) => {
    setStartupError(null);
    const next = await withTimeout(
      invoke<Dashboard>('bootstrap', { force }),
      BOOTSTRAP_TIMEOUT_MS,
      t('startupTimeout'),
    );
    setDashboard(next);
    setActiveTool((current) => next.tools.some((tool) => tool.id === current) ? current : next.tools[0]?.id ?? '');
    if (!keepLog) {
      setLog(t('envLoaded', { root: next.root }));
    }
  }, [t, setLog]);

  useEffect(() => {
    if (bootstrapStartedRef.current) return;
    bootstrapStartedRef.current = true;
    load(false).catch((error) => {
      if (!isMountedRef.current) return;
      const message = extractErrorMessage(error, t);
      setStartupError(message);
      setLog(message);
    });
  }, [load, t, setLog]);

  const refresh = useCallback(async () => {
    if (refreshing) return;
    setRefreshing(true);
    setLog(t('refreshing'));
    try {
      await load(true, false);
      setLog(t('refreshDone'));
    } catch (error) {
      const message = extractErrorMessage(error, t);
      setLog(`${t('refreshFailed')}: ${message}`);
      pushToast(`${t('refreshFailed')}: ${message}`, 'error');
    } finally {
      if (isMountedRef.current) setRefreshing(false);
    }
  }, [load, refreshing, t, setLog, pushToast]);

  const exportDiagnostics = useCallback(async () => {
    if (exportingDiagnostics) return;
    setExportingDiagnostics(true);
    setLog(t('exportingDiagnostics'));
    try {
      const result = await invoke<DiagnosticsReport>('export_diagnostics_report');
      setLog(t('diagnosticsExported', { path: result.path }));
      pushToast(t('diagnosticsExported', { path: result.path }), 'success');
    } catch (error) {
      const message = extractErrorMessage(error, t);
      setLog(`${t('diagnosticsExportFailed')}: ${message}`);
      pushToast(`${t('diagnosticsExportFailed')}: ${message}`, 'error');
    } finally {
      if (isMountedRef.current) setExportingDiagnostics(false);
    }
  }, [exportingDiagnostics, t, setLog, pushToast]);

  const active = useMemo(
    () => dashboard?.tools.find((tool) => tool.id === activeTool) ?? dashboard?.tools[0],
    [dashboard, activeTool],
  );

  const runAction = useCallback(async (
    action: 'install_tool' | 'uninstall_tool' | 'update_tool' | 'launch_tool',
    toolId: string,
  ) => {
    if (runActionInFlightRef.current) return;
    runActionInFlightRef.current = true;
    setBusyTool(toolId);
    setLog(`${actionLabelFn(action)} ${toolId}...`);
    try {
      const tool = dashboard?.tools.find((item) => item.id === toolId);
      let workspaceDir: string | null = null;
      if (action === 'launch_tool' && tool?.kind === 'ai-cli') {
        if (dashboard?.autoOpenWorkspace) {
          workspaceDir = dashboard.workspace;
          setLog(t('usingDefaultWorkspace', { workspace: dashboard.workspace }));
        } else {
          workspaceDir = await invoke<string | null>('select_workspace_dialog', {
            defaultDir: dashboard?.workspace ?? null,
          });
          if (workspaceDir === null || workspaceDir === undefined) {
            if (isMountedRef.current) setLog(t('cancelled'));
            return;
          }
        }
      }
      const args =
        action === 'launch_tool'
          ? { toolId, workspaceDir }
          : { toolId };
      const result = await invoke<ToolCommandResult>(action, args);
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join('\n');
      if (combined) setLog(combined);
      await load(true, true);
    } catch (error) {
      if (isMountedRef.current) { const message = extractErrorMessage(error, t); setLog(message); pushToast(message, 'error'); }
    } finally {
      runActionInFlightRef.current = false;
      if (isMountedRef.current) setBusyTool(null);
    }
  }, [dashboard, load, t, setLog, pushToast]);

  useEffect(() => {
    if (!showAddModal && !showMarketplace && !showSettings) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (showAddModal) setShowAddModal(false);
        if (showMarketplace) { setShowMarketplace(false); setMarketplaceSearch(''); }
        if (showSettings) setShowSettings(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [showAddModal, showMarketplace, showSettings]);

  const filteredMarketplaceTools = useMemo(
    () => marketplaceTools.filter(
      (tool) =>
        !marketplaceSearch ||
        tool.name.toLowerCase().includes(marketplaceSearch.toLowerCase()) ||
        tool.description.toLowerCase().includes(marketplaceSearch.toLowerCase()),
    ),
    [marketplaceTools, marketplaceSearch],
  );

  async function openMarketplace() {
    setShowMarketplace(true);
    setMarketplaceSearch('');
    setMarketplaceTools([]);
    setMarketplaceLoading(true);
    try {
      const tools = await invoke<MarketplaceTool[]>('marketplace_tools');
      if (!isMountedRef.current) return;
      setMarketplaceTools(tools);
    } catch (error) {
      if (isMountedRef.current) { const message = extractErrorMessage(error, t); setLog(message); pushToast(message, 'error'); }
    } finally {
      if (isMountedRef.current) setMarketplaceLoading(false);
    }
  }

  async function handleMarketplaceInstall(tool: MarketplaceTool) {
    setMarketplaceBusy(tool.id);
    setLog(`${t('installingFromMarket')}: ${tool.name}...`);
    try {
      const result = await invoke<ToolCommandResult>('install_marketplace_tool', {
        id: tool.id,
        name: tool.name,
        packageName: tool.packageName,
      });
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join('\n');
      if (combined) setLog(combined);
      if (result.success) {
        setMarketplaceTools((prev) =>
          prev.map((t) => (t.id === tool.id ? { ...t, installed: true } : t)),
        );
      }
      const tools = await invoke<MarketplaceTool[]>('marketplace_tools');
      if (!isMountedRef.current) return;
      setMarketplaceTools(tools);
      await load(true);
    } catch (error) {
      if (isMountedRef.current) { const message = extractErrorMessage(error, t); setLog(message); pushToast(message, 'error'); }
    } finally {
      if (isMountedRef.current) setMarketplaceBusy(null);
    }
  }

  async function openSettings() {
    setShowSettings(true);
    if (dashboard) {
      setSettingsValues({
        networkMode: dashboard.networkMode,
        workspacePath: dashboard.workspacePath,
        autoOpenWorkspace: dashboard.autoOpenWorkspace,
      });
    }
  }

  async function handleSaveSettings(e: React.FormEvent) {
    e.preventDefault();
    setSettingsSaving(true);
    try {
      await invoke('save_settings', {
        networkMode: settingsValues.networkMode,
        workspacePath: settingsValues.workspacePath,
        autoOpenWorkspace: settingsValues.autoOpenWorkspace,
      });
      if (!isMountedRef.current) return;
      setSettingsSaving(false);
      setShowSettings(false);
      setLog(t('settingsSaved'));
      await load(true, false);
    } catch (error) {
      if (isMountedRef.current) { const message = extractErrorMessage(error, t); pushToast(message, 'error'); }
    } finally {
      if (isMountedRef.current) setSettingsSaving(false);
    }
  }

  async function handleBrowseWorkspace() {
    try {
      const dir = await invoke<string | null>('select_workspace_dialog', {
        defaultDir: dashboard?.workspace ?? null,
      });
      if (dir) {
        setSettingsValues((prev) => ({ ...prev, workspacePath: dir }));
      }
    } catch { /* ignore */ }
  }

  async function handleAddCustomTool(e: React.FormEvent) {
    e.preventDefault();
    let name = customName.trim();

    if (installType === 'npm') {
      if (!customPackage.trim()) {
        setAddError(t('enterNpmPackageName'));
        return;
      }
      if (!name) {
        const pkg = customPackage.trim();
        const tail = pkg.includes('/')
          ? pkg.split('/').pop() || pkg
          : pkg.replace(/^@[^/]+\//, '');
        name = (tail.split('@').filter(Boolean)[0] || '').trim() || 'custom-tool';
      }
    } else {
      if (!customScriptUrl.trim()) {
        setAddError(t('enterPsScriptUrl'));
        return;
      }
      if (!name) {
        const parts = customScriptUrl.trim().split('/');
        const last = parts[parts.length - 1];
        name = last.split('.')[0] || 'custom-script';
        if (name.toLowerCase() === 'install') {
          name = parts[parts.length - 2] || 'custom-script';
        }
      }
    }

    setAddError(null);
    try {
      const result = await invoke<AddCustomToolResult>('add_custom_tool', {
        name,
        installType,
        packageName: installType === 'npm' ? customPackage.trim() : null,
        scriptUrl: installType === 'powershell-script' ? customScriptUrl.trim() : null,
        binName: installType === 'powershell-script' ? customBinName.trim() || null : null,
      });
      if (!isMountedRef.current) return;
      setDashboard(result.dashboard);
      if (result.newToolId && result.dashboard.tools.some((tool) => tool.id === result.newToolId)) {
        setActiveTool(result.newToolId);
      }
      setShowAddModal(false);
      setCustomName('');
      setCustomPackage('');
      setCustomScriptUrl('');
      setCustomBinName('');
      setLog(`${t('customToolAdded')}: ${name}`);
    } catch (error) {
      if (isMountedRef.current) setAddError(extractErrorMessage(error, t));
    }
  }

  const logRef = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [logText]);

  const statusText: Record<ToolStatus, string> = {
    ready: t('statusReady'),
    missing: t('statusMissing'),
    partial: t('statusPartial'),
  };

  const kindText: Record<ToolKind, string> = {
    runtime: t('kindRuntime'),
    'ai-cli': t('kindAiCli'),
    app: t('kindApp'),
  };

  function actionLabelFn(action: string): string {
    const labels: Record<string, string> = {
      install_tool: t('actionInstall'),
      uninstall_tool: t('actionUninstall'),
      update_tool: t('actionUpdate'),
      launch_tool: t('actionRun'),
      login_tool: t('actionLogin'),
    };
    return labels[action] ?? action;
  }

  function networkModeText(mode: string): string {
    return mode === 'china' ? t('networkModeChina') : t('networkModeGlobal');
  }

  function portableStateText(tool: ToolView): string {
    if (tool.status === 'ready') return t('factInstalledOnDrive');
    if (tool.hostAvailable) return t('factHostOnly');
    return t('factNeedsInstall');
  }



  if (!dashboard) {
    return (
      <main className='shell loading-shell'>
        <div className='aurora' />
        <section className='glass-panel loading-panel'>
          {startupError ? <XCircle size={28} /> : <Activity className='spin' size={28} />}
          <div>
            <p>{startupError ? t('loadFailed') : t('loadingApp')}</p>
            {startupError && <small>{startupError}</small>}
            {startupError && (
              <button
                type='button'
                className='primary'
                style={{ marginTop: 12 }}
                onClick={() => {
                  setStartupError(null);
                  setLog(t('retryLoading'));
                  load(false).catch((error) => {
                    if (!isMountedRef.current) return;
                    const message = extractErrorMessage(error, t);
                    setStartupError(message);
                    setLog(message);
                  });
                }}
              >
                <RefreshCw size={17} /> {t('retry')}
              </button>
            )}
          </div>
        </section>
      </main>
    );
  }

  if (!active) {
    return (
      <main className='shell loading-shell'>
        <div className='aurora' />
        <section className='glass-panel loading-panel'>
          <AlertTriangle size={28} />
          <div>
            <p>{t('loadFailed')}</p>
            <small>{dashboard.tools.length === 0 ? t('emptyToolList') : t('noActiveTool')}</small>
            <button
              type='button'
              className='primary'
              style={{ marginTop: 12 }}
              onClick={() => {
                setStartupError(null);
                setLog(t('retryLoading'));
                load(false).catch((error) => {
                  if (!isMountedRef.current) return;
                  const message = extractErrorMessage(error, t);
                  setStartupError(message);
                  setLog(message);
                });
              }}
            >
              <RefreshCw size={17} /> {t('retry')}
            </button>
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className='shell'>
      <div className='aurora' />
      <aside className='sidebar glass-panel'>
        <div className='brand-lockup'>
          <div className='brand-mark'><Terminal size={22} aria-hidden='true' /></div>
          <div>
            <h1>{t('portableAiDevKit')}</h1>
            <p>{t('portableControlPanel')}</p>
          </div>
        </div>

        <div className='root-card'>
          <HardDrive size={18} aria-hidden='true' />
          <div>
            <span>{t('envRoot')}</span>
            <strong title={dashboard.root}>{dashboard.root}</strong>
          </div>
        </div>

        <nav className='tool-nav' aria-label={t('toolList')}>
          {dashboard.tools.map((tool) => (
            <button
              key={tool.id}
              className={tool.id === active.id ? 'tool-tab active' : 'tool-tab'}
              onClick={() => setActiveTool(tool.id)}
            >
              <span className={`status-dot ${tool.status}`} />
              <span>{tool.name}</span>
              <small>{kindText[tool.kind]}</small>
            </button>
          ))}
          <button className='add-tool-btn' onClick={() => { setShowAddModal(true); setAddError(null); }}>
            <Plus size={16} />
            {t('addCustomAiTool')}
          </button>
          <button className='add-tool-btn marketplace-btn' onClick={openMarketplace}>
            <Store size={16} />
            {t('toolMarket')}
          </button>
        </nav>

        <div className='sidebar-bottom'>
          <button className='settings-btn' onClick={openSettings} title={t('settingsTitle')}>
            <Settings2 size={17} />
            {t('settingsTitle')}
          </button>
          <a className='deerflow-badge' href='https://deerflow.tech' target='_blank' rel='noreferrer'>
            {t('createdBy')}
          </a>
        </div>
      </aside>

      <section className='content'>
        <header className='topbar glass-strip'>
          <div>
            <p className='eyebrow'>{t('networkModeLabel')}: {networkModeText(dashboard.networkMode)}</p>
            <h2>{active.name}</h2>
          </div>
          <div className='topbar-actions'>
            <button
              className='icon-button'
              onClick={openSettings}
              title={t('settingsTitle')}
              aria-label={t('settingsTitle')}
            >
              <Settings2 size={18} />
            </button>
            <button
              className='icon-button'
              onClick={exportDiagnostics}
              title={t('exportDiagnostics')}
              aria-label={t('exportDiagnostics')}
              disabled={exportingDiagnostics}
              aria-busy={exportingDiagnostics}
            >
              {exportingDiagnostics ? (
                <Activity size={18} className='spin' />
              ) : (
                <FileText size={18} />
              )}
            </button>
            <button
              className='icon-button'
              onClick={refresh}
              title={t('refreshStatus')}
              aria-label={t('refreshStatus')}
              disabled={refreshing}
              aria-busy={refreshing}
            >
              <RefreshCw size={18} className={refreshing ? 'spin' : undefined} />
            </button>
          </div>
        </header>

        <section className='main-grid'>
          <article className='tool-detail glass-panel'>
            <div className='detail-head'>
              <div>
                <p className='eyebrow'>{kindText[active.kind]}</p>
                <h3>{active.name}</h3>
              </div>
              <span
                key={`${active.id}-${active.status}`}
                className={`pill ${active.status}`}
              >
                {statusText[active.status]}
              </span>
            </div>

            <dl className='facts'>
              <div>
                <dt>{t('factInstalledVersion')}</dt>
                <dd>{active.installedVersion ?? t('factNotDetected')}</dd>
              </div>
              <div>
                <dt>{t('factTargetSource')}</dt>
                <dd>{active.wantedVersion ?? active.installSource}</dd>
              </div>
              <div>
                <dt>{t('factInstallPath')}</dt>
                <dd title={active.basePath}>{active.basePath}</dd>
              </div>
              <div>
                <dt>{t('factLaunchEntry')}</dt>
                <dd title={active.launchPath}>{active.launchPath ?? t('factNoLaunchEntry')}</dd>
              </div>
              <div>
                <dt>{t('factHostDetection')}</dt>
                <dd>{active.hostAvailable ? active.hostVersion ?? t('factHostAvailable') : t('factNotDetected2')}</dd>
              </div>
              <div>
                <dt>{t('factPortableState')}</dt>
                <dd>{portableStateText(active)}</dd>
              </div>
            </dl>

            <div className='actions'>
              <button
                className='primary'
                disabled={busyTool === active.id}
                onClick={() => runAction('install_tool', active.id)}
              >
                {busyTool === active.id ? <Activity className='spin' size={17} /> : <Download size={17} />}
                {busyTool === active.id ? t('actionInstalling') : t('actionInstall')}
              </button>
              <button
                disabled={busyTool === active.id || active.status === 'missing'}
                onClick={() => runAction('update_tool', active.id)}
              >
                {busyTool === active.id ? <Activity className='spin' size={17} /> : <RefreshCw size={17} />}
                {busyTool === active.id ? t('actionProcessing') : t('actionUpdate')}
              </button>
              <button
                disabled={busyTool === active.id || active.status === 'missing' || active.kind !== 'ai-cli'}
                onClick={() => runAction('launch_tool', active.id)}
              >
                <Play size={17} /> {t('actionRun')}
              </button>
              <button
                className='danger'
                disabled={busyTool === active.id || active.status === 'missing'}
                onClick={() => runAction('uninstall_tool', active.id)}
              >
                {busyTool === active.id ? <Activity className='spin' size={17} /> : <Trash2 size={17} />}
                {busyTool === active.id ? t('actionProcessing') : t('actionUninstall')}
              </button>
              {active.id.startsWith('custom-') && (
                <button
                  className='danger'
                  disabled={busyTool === active.id}
                  onClick={async () => {
                    let confirmed = false;
                    try {
                      confirmed = window.confirm(`${t('confirmDeleteCustomTool')} \"${active.name}\"？`);
                    } catch {
                      confirmed = false;
                    }
                    if (!confirmed) return;
                    setBusyTool(active.id);
                    setLog(`${t('deletingCustomTool')}: ${active.name}...`);
                    try {
                      const nextDashboard = await invoke<Dashboard>('delete_custom_tool', { toolId: active.id });
                      if (!isMountedRef.current) return;
                      setDashboard(nextDashboard);
                      setActiveTool(nextDashboard.tools[0]?.id ?? '');
                      setLog(`${t('customToolDeleted')}: ${active.name}`);
                    } catch (error) {
                      if (isMountedRef.current) { const message = extractErrorMessage(error, t); setLog(message); pushToast(message, 'error'); }
                    } finally {
                      if (isMountedRef.current) setBusyTool(null);
                    }
                  }}
                >
                  <Trash2 size={17} /> {t('actionDelete')}
                </button>
              )}
            </div>
          </article>

          <article className='health glass-panel'>
            <div className='detail-head'>
              <div>
                <p className='eyebrow'>{t('readinessStatus')}</p>
                <h3>{t('healthCheck')}</h3>
              </div>
              <HealthIcon summary={dashboard.health.summary} />
            </div>
            <div className='check-list'>
              {dashboard.health.checks.map((check) => (
                <div className='check-row' key={check.id}>
                  {check.status === 'ok' ? (
                    <CheckCircle2 size={17} />
                  ) : check.status === 'warning' ? (
                    <AlertTriangle size={17} />
                  ) : (
                    <XCircle size={17} />
                  )}
                  <div>
                    <strong>{check.label}</strong>
                    <span title={check.message}>{check.message}</span>
                  </div>
                </div>
              ))}
            </div>
          </article>
        </section>

        <section className='log-panel glass-panel'>
          <div className='log-head'>
            <span>{t('operationLog')}</span>
          </div>
          <pre ref={logRef}>{logText}</pre>
        </section>
      </section>

      {/* Settings Modal */}
      {showSettings && (
        <div className='modal-overlay' onClick={() => setShowSettings(false)}>
          <div
            className='modal-content glass-panel'
            onClick={(e) => e.stopPropagation()}
            ref={settingsModalRef}
            role='dialog'
            aria-modal='true'
            aria-labelledby='settings-modal-title'
          >
            <h3 id='settings-modal-title'>{t('settingsTitle')}</h3>

            <form onSubmit={handleSaveSettings}>
              {/* Network Mode */}
              <div className='form-group'>
                <label>{t('networkModeLabel')}</label>
                <div className='radio-group'>
                  <label className='radio-option'>
                    <input
                      type='radio'
                      name='networkMode'
                      value='global'
                      checked={settingsValues.networkMode === 'global'}
                      onChange={() => setSettingsValues((v) => ({ ...v, networkMode: 'global' }))}
                    />
                    <div>
                      <strong>{t('networkModeGlobal')}</strong>
                      <small>{t('networkModeGlobalDesc')}</small>
                    </div>
                  </label>
                  <label className='radio-option'>
                    <input
                      type='radio'
                      name='networkMode'
                      value='china'
                      checked={settingsValues.networkMode === 'china'}
                      onChange={() => setSettingsValues((v) => ({ ...v, networkMode: 'china' }))}
                    />
                    <div>
                      <strong>{t('networkModeChina')}</strong>
                      <small>{t('networkModeChinaDesc')}</small>
                    </div>
                  </label>
                </div>
              </div>

              {/* Workspace Path */}
              <div className='form-group'>
                <label>{t('workspacePathLabel')}</label>
                <div className='input-with-button'>
                  <input
                    type='text'
                    value={settingsValues.workspacePath}
                    onChange={(e) => setSettingsValues((v) => ({ ...v, workspacePath: e.target.value }))}
                  />
                  <button type='button' className='browse-btn' onClick={handleBrowseWorkspace}>
                    {t('browse')}
                  </button>
                </div>
              </div>

              {/* Auto open workspace */}
              <div className='form-group'>
                <label className='toggle-label'>
                  <input
                    type='checkbox'
                    checked={settingsValues.autoOpenWorkspace}
                    onChange={(e) => setSettingsValues((v) => ({ ...v, autoOpenWorkspace: e.target.checked }))}
                  />
                  <span>{t('autoOpenWorkspaceLabel')}</span>
                </label>
              </div>

              {/* Language */}
              <div className='form-group'>
                <label>{t('languageLabel')}</label>
                <div className='language-selector'>
                  <button
                    type='button'
                    className={i18n.language === 'zh-CN' ? 'lang-btn active' : 'lang-btn'}
                    onClick={() => i18n.changeLanguage('zh-CN')}
                  >
                    中文
                  </button>
                  <button
                    type='button'
                    className={i18n.language === 'en' ? 'lang-btn active' : 'lang-btn'}
                    onClick={() => i18n.changeLanguage('en')}
                  >
                    English
                  </button>
                </div>
              </div>

              <div className='modal-actions'>
                <button type='button' className='btn-cancel' onClick={() => setShowSettings(false)}>
                  {t('close')}
                </button>
                <button type='submit' className='btn-confirm' disabled={settingsSaving}>
                  {settingsSaving ? <Activity className='spin' size={15} /> : null}
                  {settingsSaving ? t('saving') : t('save')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Add Custom Tool Modal */}
      {showAddModal && (
        <div className='modal-overlay' onClick={() => setShowAddModal(false)}>
          <div
            className='modal-content glass-panel'
            onClick={(e) => e.stopPropagation()}
            ref={addModalRef}
            role='dialog'
            aria-modal='true'
            aria-labelledby='add-tool-modal-title'
          >
            <h3 id='add-tool-modal-title'>{t('addCustomAiToolTitle')}</h3>

            <div className='modal-tabs'>
              <button
                type='button'
                className={installType === 'npm' ? 'modal-tab active' : 'modal-tab'}
                onClick={() => { setInstallType('npm'); setAddError(null); }}
              >
                {t('npmPackage')}
              </button>
              <button
                type='button'
                className={installType === 'powershell-script' ? 'modal-tab active' : 'modal-tab'}
                onClick={() => { setInstallType('powershell-script'); setAddError(null); }}
              >
                {t('powershellScript')}
              </button>
            </div>

            <form onSubmit={handleAddCustomTool}>
              {installType === 'npm' ? (
                <div className='form-group'>
                  <label>{t('npmPackageNameLabel')}</label>
                  <input
                    type='text'
                    required
                    placeholder={t('npmPackageNamePlaceholder')}
                    value={customPackage}
                    onChange={(e) => { setCustomPackage(e.target.value); if (addError) setAddError(null); }}
                    autoFocus
                  />
                </div>
              ) : (
                <>
                  <div className='form-group'>
                    <label>{t('psScriptUrlLabel')}</label>
                    <input
                      type='text'
                      required
                      placeholder={t('psScriptUrlPlaceholder')}
                      value={customScriptUrl}
                      onChange={(e) => { setCustomScriptUrl(e.target.value); if (addError) setAddError(null); }}
                      autoFocus
                    />
                  </div>
                  <div className='form-group'>
                    <label>{t('binFileNameLabel')}</label>
                    <input
                      type='text'
                      placeholder={t('binFileNamePlaceholder')}
                      value={customBinName}
                      onChange={(e) => { setCustomBinName(e.target.value); if (addError) setAddError(null); }}
                    />
                  </div>
                </>
              )}

              <div className='form-group'>
                <label>{t('toolNameLabel')}</label>
                <input
                  type='text'
                  placeholder={t('toolNamePlaceholder')}
                  value={customName}
                  onChange={(e) => { setCustomName(e.target.value); if (addError) setAddError(null); }}
                />
              </div>

              {addError && (
                <div style={{ color: '#ffc2b9', fontSize: '13px', marginTop: '8px' }}>
                  {addError}
                </div>
              )}
              <div className='modal-actions'>
                <button type='button' className='btn-cancel' onClick={() => setShowAddModal(false)}>
                  {t('cancel')}
                </button>
                <button type='submit' className='btn-confirm'>
                  {t('confirm')}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Marketplace Modal */}
      {showMarketplace && (
        <div className='modal-overlay' onClick={() => { setShowMarketplace(false); setMarketplaceSearch(''); }}>
          <div
            className='modal-content marketplace-modal glass-panel'
            onClick={(e) => e.stopPropagation()}
            ref={marketplaceModalRef}
            role='dialog'
            aria-modal='true'
            aria-labelledby='marketplace-modal-title'
          >
            <div className='marketplace-topbar'>
              <h3 id='marketplace-modal-title'>
                <Store size={20} aria-hidden='true' />
                {t('toolMarketTitle')}
              </h3>
              <div className='marketplace-search'>
                <Search size={16} aria-hidden='true' />
                <input
                  type='text'
                  placeholder={t('searchToolsPlaceholder')}
                  value={marketplaceSearch}
                  onChange={(e) => setMarketplaceSearch(e.target.value)}
                />
              </div>
              <button
                className='icon-button marketplace-close'
                onClick={() => { setShowMarketplace(false); setMarketplaceSearch(''); }}
                aria-label={t('close')}
              >
                <X size={18} />
              </button>
            </div>

            {marketplaceLoading ? (
              <div className='marketplace-loading'>
                <Activity className='spin' size={24} aria-hidden='true' />
                <span>{t('loadingToolList')}</span>
              </div>
            ) : filteredMarketplaceTools.length === 0 ? (
              <div className='marketplace-empty'>
                <Search size={32} aria-hidden='true' />
                <p>{t('noMatchingTools')}</p>
              </div>
            ) : (
              <div className='marketplace-grid'>
                {filteredMarketplaceTools.map((tool) => (
                  <div className={`marketplace-card ${tool.installed ? 'installed' : ''}`} key={tool.id}>
                    <div className='marketplace-card-top'>
                      <div className='marketplace-card-icon' aria-hidden='true'>
                        {tool.name.charAt(0).toUpperCase()}
                      </div>
                      <div className='marketplace-card-info'>
                        <h4>{tool.name}</h4>
                        <span className='marketplace-category'>{tool.category}</span>
                      </div>
                    </div>
                    <p className='marketplace-desc'>{tool.description}</p>
                    <div className='marketplace-card-actions'>
                      <a
                        href={tool.homepage}
                        target='_blank'
                        rel='noreferrer'
                        className='marketplace-link'
                        title={t('visitHomepage')}
                        aria-label={t('visitHomepage')}
                      >
                        <ExternalLink size={15} />
                      </a>
                      <div className='marketplace-card-buttons'>
                        {tool.inManifest && (
                          <span className='marketplace-manifest-badge' title={t('manifestNote')}>
                            {t('preset')}
                          </span>
                        )}
                        {tool.installed ? (
                          <span className='marketplace-installed-badge'>
                            <CheckCircle2 size={15} aria-hidden='true' />
                            {t('installed')}
                          </span>
                        ) : (
                          <button
                            className='primary'
                            disabled={marketplaceBusy === tool.id}
                            onClick={() => handleMarketplaceInstall(tool)}
                          >
                            {marketplaceBusy === tool.id ? (
                              <><Activity className='spin' size={15} /> {t('installing')}</>
                            ) : (
                              <><Download size={15} /> {t('install')}</>
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

      {toasts.length > 0 && (
        <div className='toast-stack' role='region' aria-label={t('notifications')} aria-live='polite'>
          {toasts.map((toast) => (
            <div key={toast.id} className={`toast toast-${toast.kind}`} role='alert'>
              <span className='toast-message'>{toast.message}</span>
              <button
                type='button'
                className='toast-close'
                aria-label={t('closeToast')}
                onClick={() => dismissToast(toast.id)}
              >
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </main>
  );
}

function HealthIcon({ summary }: { summary: HealthSummary }) {
  if (summary === 'healthy') {
    return <ShieldCheck className='health-icon ok' size={30} />;
  }
  if (summary === 'warning') {
    return <AlertTriangle className='health-icon warn' size={30} />;
  }
  return <XCircle className='health-icon error' size={30} />;
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
