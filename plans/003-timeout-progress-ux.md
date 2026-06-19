# Plan 003: Add install/update progress feedback and configurable timeout for slow drives

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 65bc91d..HEAD -- src-tauri/src/portable.rs src-tauri/src/lib.rs src/main.tsx src/i18n/locales/`
> If any of these files changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plans/001-npm-staging-atomic-install.md (the staging approach
  makes the fixed 10-minute timeout survivable, but UX is still opaque)
- **Category**: dx
- **Planned at**: commit `65bc91d`, 2026-06-19

## Why this matters

Today, clicking "Install" or "Update" on an AI CLI shows a spinner and nothing
else. On a slow removable drive, the user stares at a static UI for 5–15
minutes with zero indication of progress. If the operation times out, the only
feedback is a log message like "Claude Code 安装超时" — no hint that the root
cause is drive speed, no way to adjust the timeout, and no progress bar.

This plan adds: (1) an elapsed-time counter that ticks while an install/update
is in flight so the user sees something is happening, (2) a "Cancel" button
that aborts a hung operation, and (3) a configurable per-action timeout in
the settings so users on slow drives can raise the 10-minute default without a
code change.

## Current state

### Relevant files (roles)

- `src/main.tsx` — React frontend; contains `runAction` callback (the
  install/update/uninstall dispatcher), the log panel, toasts, and all UI.
  This is the primary file modified.
- `src/i18n/locales/zh-CN.json` — Chinese translations (104 keys). Add new
  keys here.
- `src/i18n/locales/en.json` — English translations (104 keys, same keys).
  Add matching new keys here.
- `src-tauri/src/portable.rs` — Rust backend; the timeout constants
  (`NPM_INSTALL_TIMEOUT`, `EXPAND_TIMEOUT`, etc.) and `run_command_with_timeout`
  live here. Minor change: expose `tool_action` timeout as a parameter.
- `src-tauri/src/lib.rs` — Tauri command wrappers (`install_tool`,
  `update_tool`, etc.). Minor change: pass optional timeout.

### Frontend install flow (today)

`src/main.tsx:339-379`, the `runAction` callback:

```tsx
const runAction = useCallback(async (
    action: 'install_tool' | 'uninstall_tool' | 'update_tool' | 'launch_tool',
    toolId: string,
  ) => {
    if (runActionInFlightRef.current) return;
    runActionInFlightRef.current = true;
    setBusyTool(toolId);
    setLog(`${actionLabelFn(action)} ${toolId}...`);
    try {
      // ... workspace selection for launch_tool ...
      const result = await invoke<ToolCommandResult>(action, args);
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join('\n');
      if (combined) setLog(combined);
      await load(true, true);
    } catch (error) { /* ... */ }
    finally { runActionInFlightRef.current = false; if (isMountedRef.current) setBusyTool(null); }
  }, [dashboard, load, t, setLog, pushToast]);
```

Key observations:
- `runActionInFlightRef` prevents concurrent actions — good, keep this.
- `busyTool` state drives a spinner on the active tool card. No elapsed-time
  display, no cancel, no timeout awareness.
- The Tauri `invoke` call blocks until the backend returns. For npm installs
  on slow drives this can be 10+ minutes.

### Action buttons (where "Install"/"Update" are rendered)

`src/main.tsx:797,804`:
```tsx
onClick={() => runAction('install_tool', active.id)}
onClick={() => runAction('update_tool', active.id)}
```

### Timeout constants (backend)

`src-tauri/src/portable.rs:19-22`:
```rust
const NPM_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3 * 60);
const EXPAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const SCRIPT_INSTALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
```

These are private constants. Plan 001 uses `NPM_INSTALL_TIMEOUT` inside the new
`install_npm_tool_to_staging`. This plan makes the timeout overridable.

### `ToolActionRequest` struct

`src-tauri/src/portable.rs:312-324`:
```rust
pub struct ToolActionRequest {
    tool_id: String,
    action: String,
}
impl ToolActionRequest {
    pub fn new(tool_id: String, action: &str) -> Self {
        Self { tool_id, action: action.to_string() }
    }
}
```

Currently no timeout field. This plan adds one.

### `tool_action` dispatcher

`src-tauri/src/portable.rs:515-536`:
```rust
pub fn tool_action(app, request) -> Result<ToolCommandResult, AppError> {
    let _action_guard = ACTION_LOCK.lock()...;
    bootstrap_kit(app)?;
    // ...
    match request.action.as_str() {
        "install" => install_tool(app, &manifest, &settings, tool),
        "update" => install_tool(app, &manifest, &settings, tool),
        "uninstall" => uninstall_tool(app, tool),
        _ => Err(...)
    }
}
```

### i18n conventions

- Keys are camelCase. Values are Chinese strings in `zh-CN.json`, English in
  `en.json`. Both files must have exactly the same keys (verified: 104 keys
  each, zero mismatch at `65bc91d`).
- Used in components as `t('keyName')` via `useTranslation()`.
- Pluralization/interpolation: `{{variable}}` syntax (see
  `diagnosticsExported` key as exemplar).

### TypeScript conventions

- Types are declared at the top of `main.tsx` (lines 28-103).
- React state is managed with `useState` + `useCallback` + `useRef`.
- CSS classes follow kebab-case in `styles.css`.

## Commands you will need

| Purpose              | Command                                                                          | Expected on success |
|----------------------|----------------------------------------------------------------------------------|---------------------|
| Typecheck            | `npx tsc --noEmit`                                                               | exit 0, no errors   |
| Rust build           | `cargo check --manifest-path src-tauri/Cargo.toml --lib`                         | exit 0              |
| Rust tests           | `cargo test --manifest-path src-tauri/Cargo.toml --lib`                          | all pass           |
| Rust lint            | `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings`         | exit 0             |
| Locale key parity    | `node -e "const a=require('./src/i18n/locales/zh-CN.json');const b=require('./src/i18n/locales/en.json');const ka=Object.keys(a),kb=Object.keys(b);if(ka.length!==kb.length||ka.some(k=>!b.hasOwnProperty(k))){process.exit(1)}"` | exit 0 |

## Scope

**In scope** (files you should modify):
- `src/main.tsx` — elapsed-time UI, cancel button, settings form for timeout
- `src/i18n/locales/zh-CN.json` — new translation keys
- `src/i18n/locales/en.json` — matching new translation keys
- `src-tauri/src/portable.rs` — add timeout field to `ToolActionRequest`,
  thread it through `tool_action` → `install_npm_tool_to_staging`
- `src-tauri/src/lib.rs` — pass optional timeout from Tauri commands to
  `tool_action`

**Out of scope** (do NOT touch):
- `src/styles.css` — CSS changes needed should be inline or Tailwind if
  already in use (this repo does NOT use Tailwind; CSS is in `styles.css`).
  Minor class additions in `styles.css` are acceptable if truly needed (e.g.
  for a timer display), but keep them minimal.
- Archive and PowerShell-script install paths — they have their own timeouts
  (`DOWNLOAD_TIMEOUT`, `EXPAND_TIMEOUT`, `SCRIPT_INSTALL_TIMEOUT`) and are
  less affected by slow drives after plan 001. Do not change their timeout
  handling.
- `config/tool-manifest.json` — no schema change.
- Plan 001's new functions — this plan only overrides the timeout value that
  001's `install_npm_tool_to_staging` passes to `run_command_with_timeout`.

## Git workflow

- Branch: `advisor/003-timeout-progress-ux`
- Commit per step; message style: imperative short subject (e.g. `Add elapsed
  timer to install/update actions`).
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add timeout field to ToolActionRequest and thread it through

In `src-tauri/src/portable.rs`:

1a. Add a `timeout_secs` field to `ToolActionRequest`:
```rust
pub struct ToolActionRequest {
    tool_id: String,
    action: String,
    timeout_secs: Option<u64>,
}
```

1b. Update `ToolActionRequest::new` to accept an optional timeout:
```rust
impl ToolActionRequest {
    pub fn new(tool_id: String, action: &str) -> Self {
        Self { tool_id, action: action.to_string(), timeout_secs: None }
    }
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }
}
```

1c. In `install_npm_tool_to_staging` (created by plan 001), add a `timeout`
parameter of type `std::time::Duration` and use it instead of the hardcoded
`NPM_INSTALL_TIMEOUT`:
```rust
fn install_npm_tool_to_staging(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
    timeout: std::time::Duration,     // <-- NEW parameter
) -> Result<PathBuf, AppError> {
```
Replace the call `run_command_with_timeout(command, NPM_INSTALL_TIMEOUT)` with
`run_command_with_timeout(command, timeout)`.

**Important**: if plan 001 has not yet landed, `install_npm_tool_to_staging` does
not exist yet. In that case this step only modifies the *current*
`install_npm_tool` (use `timeout` parameter instead of `NPM_INSTALL_TIMEOUT`
at the `run_command_with_timeout` call). The plan works either way — adjust
the exact insertion point based on what exists at drift-check time.

1d. In `install_tool` and `tool_action`, propagate the timeout. In
`install_tool`:
```rust
fn install_tool(
    app: &AppState,
    manifest: &Manifest,
    settings: &Settings,
    tool: &ToolDefinition,
    timeout: std::time::Duration,     // <-- NEW
) -> Result<ToolCommandResult, AppError> {
```
Pass it through to `install_npm_tool` / `install_npm_tool_to_staging`.

In `tool_action`:
```rust
pub fn tool_action(app, request) -> Result<ToolCommandResult, AppError> {
    // ...
    let timeout = request.timeout_secs
        .map(|s| std::time::Duration::from_secs(s))
        .unwrap_or(NPM_INSTALL_TIMEOUT);  // default for non-npm is fine; archive/script paths ignore it
    match request.action.as_str() {
        "install" => install_tool(app, &manifest, &settings, tool, timeout),
        "update" => install_tool(app, &manifest, &settings, tool, timeout),
        "uninstall" => uninstall_tool(app, tool),
        _ => Err(...)
    }
}
```

**Verify**:
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` → exit 0.

### Step 2: Accept optional timeout in the Tauri commands

In `src-tauri/src/lib.rs`, update `install_tool`, `update_tool`, and
`uninstall_tool` commands to accept an optional `timeout_secs: Option<u64>`
parameter. Pass it through to `ToolActionRequest`:

```rust
#[tauri::command]
async fn install_tool(
    tool_id: String,
    timeout_secs: Option<u64>,
) -> Result<ToolCommandResult, AppError> {
    tokio::task::spawn_blocking(move || -> Result<ToolCommandResult, AppError> {
        let app = AppState::discover()?;
        let request = ToolActionRequest::new(tool_id, "install");
        let request = if let Some(s) = timeout_secs {
            request.with_timeout(s)
        } else {
            request
        };
        tool_action(&app, request)
    })
    .await
    .map_err(|e| AppError::Message(format!("Task join error: {}", e)))?
}
```

Apply the same pattern to `update_tool`. `uninstall_tool` does not need it
(pass `None`). Also update `run_headless_tool_install` to accept and forward
the parameter.

Also update `install_marketplace_tool` (which internally calls
`install_tool`-like logic) to pass the timeout through if it becomes a
parameter.

**Verify**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.

### Step 3: Add i18n keys for progress UX

Add these keys to BOTH `src/i18n/locales/zh-CN.json` and
`src/i18n/locales/en.json`:

**zh-CN.json** (add to the object):
```json
"actionInProgress": "正在处理... ({{elapsed}})",
"cancelAction": "取消",
"actionCancelled": "操作已取消",
"installTimeoutLabel": "安装超时（秒）",
"installTimeoutDescription": "在低速移动盘上安装大型 AI CLI 时可能需要更长时间（默认 600 秒 = 10 分钟）"
```

**en.json** (add to the object):
```json
"actionInProgress": "In progress... ({{elapsed}})",
"cancelAction": "Cancel",
"actionCancelled": "Action cancelled",
"installTimeoutLabel": "Install timeout (seconds)",
"installTimeoutDescription": "Large AI CLIs on slow removable drives may need more time (default 600 = 10 min)"
```

**Verify**: `node -e "const a=require('./src/i18n/locales/zh-CN.json');const b=require('./src/i18n/locales/en.json');const ka=Object.keys(a),kb=Object.keys(b);console.log('zh:',ka.length,'en:',kb.length,'missing:',ka.filter(k=>!b.hasOwnProperty(k)).join(','))"` → 110 keys each, no missing.

### Step 4: Add elapsed-time timer and cancel to the frontend

In `src/main.tsx`:

4a. Add a new state ref for tracking the active action's start time and a cancel
mechanism. Near the existing state declarations (around line 120-150), add:

```tsx
const actionStartRef = useRef<number>(0);
const actionAbortRef = useRef<AbortController | null>(null);
```

4b. Add a timer `useEffect` that ticks every second while an action is in
flight, updating a display string. Add a new state:

```tsx
const [actionElapsed, setActionElapsed] = useState<string>('');
```

And a `useEffect`:
```tsx
useEffect(() => {
  if (!busyTool) {
    setActionElapsed('');
    return;
  }
  actionStartRef.current = Date.now();
  const id = window.setInterval(() => {
    const secs = Math.floor((Date.now() - actionStartRef.current) / 1000);
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    setActionElapsed(`${m}:${String(s).padStart(2, '0')}`);
  }, 1000);
  return () => window.clearInterval(id);
}, [busyTool]);
```

4c. In the `runAction` callback, integrate the elapsed display and cancel:

```tsx
const runAction = useCallback(async (
    action: 'install_tool' | 'uninstall_tool' | 'update_tool' | 'launch_tool',
    toolId: string,
  ) => {
    if (runActionInFlightRef.current) return;
    runActionInFlightRef.current = true;
    actionAbortRef.current = new AbortController();
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
      const args: Record<string, unknown> =
        action === 'launch_tool'
          ? { toolId, workspaceDir }
          : { toolId, timeoutSecs: settingsRef.current?.installTimeout ?? null };
      const result = await invoke<ToolCommandResult>(action, args);
      if (!isMountedRef.current) return;
      const combined = [result.message, result.output].filter(Boolean).join('\n');
      if (combined) setLog(combined);
      await load(true, true);
    } catch (error) {
      if (isMountedRef.current) {
        const message = extractErrorMessage(error, t);
        setLog(message);
        pushToast(message, 'error');
      }
    } finally {
      runActionInFlightRef.current = false;
      actionAbortRef.current = null;
      if (isMountedRef.current) setBusyTool(null);
    }
  }, [dashboard, load, t, setLog, pushToast]);
```

Note: the actual cancellation of a long-running backend `invoke` is not
trivially possible in Tauri's current invoke model (it's not abortable).
The "Cancel" button should set a UI flag that prevents future invokes and
shows a user-facing message, but the current backend operation will run to
completion or timeout in the background. For this plan, the cancel button
dismisses the spinner in the UI and logs a cancellation message. A true
backend cancellation would require a side-channel (e.g. a cancel flag file or
a separate Tauri command) which is deferred.

Add a `cancelAction` function:
```tsx
const cancelAction = useCallback(() => {
  // The backend invoke is not abortable in Tauri's invoke model.
  // We clear the UI busy state so the user can interact again.
  // The backend operation continues and will complete or timeout.
  if (isMountedRef.current) {
    runActionInFlightRef.current = false;
    setBusyTool(null);
    setActionElapsed('');
    setLog(t('actionCancelled'));
  }
}, [t]);
```

4d. In the action buttons area (around `src/main.tsx:797-820`), show the
elapsed time and a cancel button while `busyTool` matches. Find where the
install/update buttons are rendered and add a condition:

Near the install/update buttons, add:
```tsx
{busyTool === active.id && actionElapsed && (
  <div className="action-progress">
    <span className="action-elapsed">{t('actionInProgress', { elapsed: actionElapsed })}</span>
    <button className="cancel-action-btn" onClick={cancelAction}>
      {t('cancelAction')}
    </button>
  </div>
)}
```

**Verify**: `npx tsc --noEmit` → exit 0, no errors.

### Step 5: Add install timeout setting to the Settings form

The settings form is in `src/main.tsx` (the `showSettings` modal). The current
settings state type is `SettingsValues` with `networkMode`, `workspacePath`,
`autoOpenWorkspace`. Add `installTimeout`:

5a. Extend the `SettingsValues` type (near line 99):
```tsx
type SettingsValues = {
  networkMode: string;
  workspacePath: string;
  autoOpenWorkspace: boolean;
  installTimeout: number;       // seconds, default 600
};
```

5b. In the settings form, add an input field after the existing fields (look
for the `autoOpenWorkspace` checkbox and add after it):
```tsx
<label className="settings-label">
  {t('installTimeoutLabel')}
  <span className="settings-hint">{t('installTimeoutDescription')}</span>
  <input
    type="number"
    min={60}
    max={3600}
    step={60}
    value={settingsValues.installTimeout}
    onChange={(e) => setSettingsValues({ ...settingsValues, installTimeout: Number(e.target.value) || 600 })}
  />
</label>
```

5c. Store the timeout in the settings state. The existing `load` callback
reads dashboard and can also read a `config/app-settings.json`-backed value.
The simplest approach: store `installTimeout` in localStorage (it's a UI
preference, not a kit-level setting). Add to the `load` callback where
settings are initialized, and to the save flow. Alternatively, if you want it
to persist in the kit's `config/app-settings.json`, add the field to the Rust
`Settings` struct and `save_settings` command. **The simpler approach
(localStorage) is preferred** — keep the Rust `Settings` struct unchanged.

Use a `useEffect` to load the initial value:
```tsx
const [installTimeout, setInstallTimeout] = useState<number>(() => {
  const saved = localStorage.getItem('installTimeout');
  return saved ? Number(saved) : 600;
});
```
And persist on change:
```tsx
useEffect(() => { localStorage.setItem('installTimeout', String(installTimeout)); }, [installTimeout]);
```

5d. Expose this to `runAction` via a ref (so the callback doesn't need it in
its dependency array):
```tsx
const installTimeoutRef = useRef(installTimeout);
installTimeoutRef.current = installTimeout;
```

Then in `runAction`'s args construction:
```tsx
const args: Record<string, unknown> =
  action === 'launch_tool'
    ? { toolId, workspaceDir }
    : { toolId, timeoutSecs: installTimeoutRef.current };
```

**Verify**: `npx tsc --noEmit` → exit 0, no errors.

### Step 6: Minimal CSS for the timer and cancel button

In `src/styles.css`, add at the end (or near the existing action button styles):

```css
.action-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
  font-size: 0.85rem;
  color: var(--text-secondary, #888);
}
.action-elapsed {
  font-variant-numeric: tabular-nums;
}
.cancel-action-btn {
  padding: 4px 12px;
  border: 1px solid #666;
  border-radius: 4px;
  background: transparent;
  color: #ccc;
  cursor: pointer;
  font-size: 0.8rem;
}
.cancel-action-btn:hover {
  border-color: #e55;
  color: #e55;
}
```

**Verify**: `npx tsc --noEmit` → exit 0.

## Test plan

- This plan is primarily a frontend DX change. The backend changes are minor
  (adding an optional parameter that defaults to the existing constant).
- Backend: existing tests should continue passing unchanged (they don't pass
  `timeout_secs`). Verify: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
  → all pass.
- Manual smoke test: open the app, click "Update" on an installed npm tool.
  Confirm the elapsed timer ticks (`1:00`, `1:01`, ...) and the cancel button
  appears. Click cancel → spinner clears. Change the timeout in Settings →
  confirm the value is used on the next install.
- Locale parity: `node -e "const a=require('./src/i18n/locales/zh-CN.json');const b=require('./src/i18n/locales/en.json');const ka=Object.keys(a),kb=Object.keys(b);if(ka.length!==kb.length){process.exit(1)}console.log('ok',ka.length)"` → `ok 110`.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml --lib` exits 0
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` exits 0
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib` → all pass
- [ ] `npx tsc --noEmit` → exit 0
- [ ] Locale key parity check → 110 keys, no mismatch
- [ ] `ToolActionRequest` struct has `timeout_secs: Option<u64>` field
- [ ] `grep -n "actionElapsed" src/main.tsx` shows the timer rendering
- [ ] `grep -n "cancelAction" src/main.tsx` shows the cancel function
- [ ] `grep -n "installTimeout" src/i18n/locales/zh-CN.json` shows the key
- [ ] `grep -n "installTimeout" src/i18n/locales/en.json` shows the key
- [ ] No files outside the in-scope list are modified (`git status`)
- [ ] `plans/README.md` status row for 003 updated

## STOP conditions

Stop and report back (do not improvise) if:

- The cited line ranges in `portable.rs`, `lib.rs`, or `main.tsx` don't match
  the "Current state" excerpts (codebase drifted since `65bc91d`).
- Plan 001 has landed and `install_npm_tool_to_staging` exists but its signature
  differs from what's described here — report the actual signature so the plan
  can be reconciled.
- `npx tsc --noEmit` produces type errors that aren't trivially fixable
  (e.g. a Tauri invoke signature mismatch between frontend and backend).
- The CSS file uses a CSS variable or class that doesn't exist (no design system
  was found during recon; keep CSS self-contained).
- A step's verification fails twice after a reasonable fix attempt.

## Maintenance notes

- **True backend cancellation**: Tauri's `invoke` is not abortable. The cancel
  button currently only clears the UI. A future enhancement could add a
  `cancel_tool_action` Tauri command that sets a shared `AtomicBool` flag
  checked by `run_command_with_timeout`'s polling loop — but that requires
  changes to the timeout polling mechanism and is deferred.
- **Timeout default**: `600` (10 minutes) is hardcoded as the localStorage
  default and the Rust `NPM_INSTALL_TIMEOUT` fallback. If either needs
  changing, both must be updated together — a maintenance burden worth noting.
- **Reviewer focus**: confirm the `timeout_secs` parameter flows from the
  frontend `invoke` → Tauri command → `tool_action` → `install_npm_tool`
  (or `install_npm_tool_to_staging` if plan 001 landed) without being silently
  dropped. Check that the cancel button doesn't leave `runActionInFlightRef`
  permanently stuck (it must be cleared in `finally` AND in `cancelAction`).
- **Plan 005 interaction**: the diagnostics plan will add a drive-speed check
  that could auto-set the timeout to a higher value on slow drives. The
  localStorage-based timeout is compatible with that (005 would write to
  localStorage too).
