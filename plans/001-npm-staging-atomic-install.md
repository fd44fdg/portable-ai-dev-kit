# Plan 001: Make npm AI-CLI install/update robust on slow removable drives

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 65bc91d..HEAD -- src-tauri/src/portable.rs`
> If that file changed since this plan was written, compare the "Current state"
> excerpts against the live code before proceeding; on a mismatch, treat it as
> a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug | perf
- **Planned at**: commit `65bc91d`, 2026-06-19

## Why this matters

When the kit lives on a slow removable drive (the operator measured the target
`F:` drive at **233 ms per small-file write** and **0.5 MB/s sequential**),
updating AI CLIs (npm packages like `@anthropic-ai/claude-code` whose
`node_modules` is ~444 MB / thousands of files) **times out and fails**:

- `install_npm_tool` runs `npm install --prefix <SLOW DRIVE>/tools/<cli>`, so
  every file npm writes lands directly on the slow drive.
- `NPM_INSTALL_TIMEOUT` is a fixed 10 minutes (`portable.rs:19`), which the
  measured write speed makes unreachable for large packages.
- npm overwrites `node_modules` **in place**. A timeout mid-install leaves the
  tool half-written → next dashboard shows `partial` / broken tool.

The archive install path (Node.js / Git) already does the right thing:
`prepare_local_archive_copy` (`portable.rs:1422`) copies to the local SSD
`%TEMP%`, and `install_archive_tool` extracts into a `*-staging` dir then
atomically renames into place with a `*-backup` rollback
(`portable.rs:1109-1171`). This plan brings the **npm** path to the same
robustness: install on local SSD, then atomically swap onto the slow drive, and
redirect the npm **cache** to the local SSD so cache I/O doesn't hit the slow
drive either. After this, a slow-drive npm update succeeds in wall-clock terms
that are independent of drive small-file latency, and a failure never corrupts
the existing install.

## Current state

### Relevant files (roles)

- `src-tauri/src/portable.rs` — all install/state logic. The ONLY file this
  plan modifies.
- `config/tool-manifest.json` — tool definitions; `codex` / `claude` use
  `"type": "npm"`. Read-only reference; do not change.

### The function to change: `install_npm_tool` (today)

`src-tauri/src/portable.rs:751-830`. Current shape (abbreviated, keep line
markers for orientation):

```rust
fn install_npm_tool(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    let node_root = app.path("apps/node");
    validate_portable_npm(app).map_err(AppError::Message)?;
    let package_name = tool.package_name.as_ref()
        .ok_or_else(|| AppError::Message(format!("{} 未配置 npm 包", tool.name)))?;
    let registry = resolve_registry(app, settings)?;
    let tool_root = app.path(&tool.base_path);                 // <- ON SLOW DRIVE
    fs::create_dir_all(&tool_root)?;

    if !tool_root.join("package.json").exists() {
        fs::write(tool_root.join("package.json"),
            "{\"name\":\"portable-ai-tool\",\"private\":true}\n")?;
    }

    let mut command = portable_npm_command(app)?;
    command.arg("install")
        .arg("--prefix").arg(display_path(&tool_root))         // <- writes to slow drive
        .arg(package_name)
        .arg("--no-fund").arg("--no-audit")
        .arg("--registry").arg(&registry)
        .current_dir(display_path(&tool_root));
    apply_portable_env(app, &mut command);                     // <- sets NPM_CONFIG_CACHE on slow drive
    prepend_path(&mut command, &node_root);

    let output = match run_command_with_timeout(command, NPM_INSTALL_TIMEOUT) {
        Some(output) => output,
        None => { /* ... timeout message ... */ }
    };
    // ... freebuff patch, persist_action_state, return ToolCommandResult ...
}
```

### Key helper signatures already in the file (reuse, don't reinvent)

- `run_command_with_timeout(mut command: Command, timeout: std::time::Duration) -> Option<std::process::Output>`
  — `portable.rs:1475`. Spawns, drains stdout/stderr, kills on timeout.
- `apply_portable_env(app: &AppState, command: &mut Command)` — `portable.rs:2268`.
  Sets HOME/APPDATA/TEMP/TMP/NPM_CONFIG_CACHE/NPM_CONFIG_PREFIX, etc.
- `command_output(output: &std::process::Output) -> String` — `portable.rs:2325`.
- `display_path(path: &Path) -> String` — `portable.rs:2245`.
- `persist_action_state(app, tool, success, source, output)` — `portable.rs:2102`.
- The archive path's swap+rollback idiom to mirror (from
  `install_archive_tool`, `portable.rs:1109-1171`):
  ```rust
  // staging dir exists; destination may exist -> rename to backup first
  if destination.exists() {
      fs::rename(&destination, &backup)?;
  }
  if let Err(error) = fs::rename(&staging, &destination) {
      if backup.exists() && !destination.exists() {
          let _ = fs::rename(&backup, &destination);   // rollback
      }
      return Err(error.into());
  }
  if backup.exists() {
      fs::remove_dir_all(&backup)?;
  }
  ```

### The `freebuff` post-install patch depends on the tool path

`patch_freebuff_index(app, tool)` (`portable.rs:876`) reads/writes
`<tool_root>/node_modules/freebuff/index.js`. It MUST run **after** the install
lands at the final `tool_root`. Keep its call position correct after the move.

### Repo conventions to match

- Error type: return `AppError` (`Message(String)`, `Io(io::Error)`, `Json`).
  `?` works for `io::Error` → `AppError` via the `#[from]` impl (`portable.rs:34`).
- Chinese user-facing strings (the whole file uses them, e.g. `"{} 安装超时"`).
  Keep new/changed messages in Chinese to match.
- Atomic file ops: Windows `fs::rename` uses `MOVEFILE_REPLACE_EXISTING`; the
  codebase relies on this for `.bat`/state files (`portable.rs:2095-2098`).
  **Caveat**: `fs::rename` on Windows does NOT replace an existing *non-empty
  directory* — see Step 2 for the handling.
- `AppState` has `app.path(&str) -> PathBuf` (`portable.rs:90`) and `app.root`.
- `std::env::temp_dir()` is already used for the archive local copy
  (`portable.rs:1423`): `env::temp_dir().join("portable-ai-dev-kit")`.

## Commands you will need

| Purpose              | Command                                                                          | Expected on success |
|----------------------|----------------------------------------------------------------------------------|---------------------|
| Build lib            | `cargo check --manifest-path src-tauri/Cargo.toml --lib`                         | exit 0              |
| Tests (this repo)    | `cargo test --manifest-path src-tauri/Cargo.toml --lib`                          | all pass           |
| Lint                 | `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings`         | exit 0             |

(All three were verified to pass during audit against `65bc91d`.)

## Scope

**In scope** (the only file you should modify):
- `src-tauri/src/portable.rs`

**Out of scope** (do NOT touch, even though they look related):
- `src-tauri/src/lib.rs` — Tauri command wrappers; signature of `install_npm_tool` does NOT change.
- `src/main.tsx` — frontend (progress/timeout UX is a separate plan, 003).
- `config/tool-manifest.json`, `config/marketplace.json` — no schema change.
- The archive install path (`install_archive_tool`) and PowerShell-script path
  (`install_powershell_script_tool`) — already robust; leave them.
- `NPM_INSTALL_TIMEOUT` value — changing it is plan 003. This plan keeps the
  constant as-is; the staging approach makes the 10-minute budget realistic
  because the heavy I/O moves to local SSD.

## Git workflow

- Branch: `advisor/001-npm-staging-atomic-install`
- Commit per step; message style matches recent history, e.g.
  `Stage npm installs on local SSD and swap atomically`. See `git log --oneline`
  for tone (imperative, short subject).
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add a local-SSD staging install helper

Add a new private function near `install_npm_tool` (after it, ~`portable.rs:830`).
It performs the npm install into a staging directory rooted at
`%TEMP%\portable-ai-dev-kit\npm-staging\<tool.id>-<unique>` so concurrent installs
can't collide, returns the staging `PathBuf`, and removes the staging dir on
failure. Target shape:

```rust
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
) -> Result<PathBuf, AppError> {
    let node_root = app.path("apps/node");
    validate_portable_npm(app).map_err(AppError::Message)?;
    let package_name = tool.package_name.as_ref()
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

    let output = match run_command_with_timeout(command, NPM_INSTALL_TIMEOUT) {
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
        .and_then(|p| p.split('@').next().and_then(|scope| {
            // package_name may be "@scope/name@latest" or "name@latest"
            let trimmed = scope.trim_start_matches('@');
            Some(trimmed.rsplit('/').next().unwrap_or(trimmed).to_string())
        }))
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
```

Notes:
- `OffsetDateTime` and `Rfc3339` are already imported at the top of the file
  (`portable.rs:11`).
- Keep the timeout message Chinese and similar to the existing one.
- Do NOT add new dependencies. Reuse `std::env`, `std::fs`.

**Verify**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0
(new function compiles, even though it's not yet called).

### Step 2: Swap the staging tree into the tool path atomically with rollback

Add a private helper that moves a freshly-installed staging tree onto the
portable drive's tool path, preserving a backup for rollback. Put it next to
Step 1's function. Windows `fs::rename` will not replace a non-empty dir, so we
move the existing destination to `backup` first (same idiom as the archive
path), then rename staging → destination; on failure, restore from backup.

```rust
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
```

IMPORTANT — cross-device rename reality check: `fs::rename` across volumes
(local SSD `%TEMP%` → portable `F:`) **fails with `EXDEV`/Error 17 on Windows
for directories is not the Unix case** — on Windows, `MoveFileExW` **does**
move a directory across volumes (it's a real move, recursive). This is the same
primitive `prepare_local_archive_copy`-driven flow relies on in reverse
(extract locally, the archive path itself renames staging→destination across
the same volume boundary at `portable.rs:1159`). If in testing a
cross-volume directory rename fails, the STOP condition below covers it; do
not silently fall back to leaving the install broken.

**Verify**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.

### Step 3: Rewrite `install_npm_tool` to use staging + swap, keep freebuff patch + state

Replace the body of `install_npm_tool` (`portable.rs:751-830`) so it:
1. Calls `install_npm_tool_to_staging` (Step 1).
2. Runs the `freebuff` patch **on the staging tree** BEFORE the move only if
   you keep `patch_freebuff_index` path-resolving via `app.path(&tool.base_path)`.
   **Simplest correct approach**: do the move first, then run the existing
   `patch_freebuff_index(app, tool)` exactly as today (it reads the final
   `tools/<cli>/node_modules/freebuff/index.js`). This keeps Step 3 minimal and
   the patch unchanged.
3. Calls `swap_npm_install_into_place` (Step 2).
4. Runs `patch_freebuff_index` after the move (unchanged call).
5. Computes success/combined output and calls `persist_action_state` exactly as
   today, returning the same `ToolCommandResult`.

New body (replace lines 751-830 in full):

```rust
fn install_npm_tool(
    app: &AppState,
    settings: &Settings,
    tool: &ToolDefinition,
) -> Result<ToolCommandResult, AppError> {
    let package_name = tool
        .package_name
        .clone()
        .ok_or_else(|| AppError::Message(format!("{} 未配置 npm 包", tool.name)))?;

    // Install on the local SSD first; the heavy npm I/O must not hit the
    // (possibly slow) portable drive. On failure this returns an error with
    // a Chinese message and the staging dir is already cleaned up.
    let staging = install_npm_tool_to_staging(app, settings, tool)?;

    // Move the finished tree onto the portable drive, with rollback. On
    // failure the previous install is restored and we surface an error.
    if let Err(error) = swap_npm_install_into_place(app, tool, staging) {
        // Preserve prior error semantics: persist a failure state and return
        // a ToolCommandResult so the UI shows the message.
        persist_action_state(app, tool, false, Some(package_name.clone()), &error.to_string())?;
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
    persist_action_state(app, tool, true, Some(package_name.clone()), &combined)?;

    Ok(ToolCommandResult {
        tool_id: tool.id.clone(),
        action: "install".to_string(),
        success: true,
        message: format!("{} 已安装", tool.name),
        output: combined,
    })
}
```

Rationale for the structural changes vs. the old body:
- Old code built `package_name` as `&str` from `tool.package_name`; new code
  clones it once (needed because `persist_action_state` takes `Option<String>`
  and we also compare it).
- The old "timeout returns Ok with success=false" path is now inside
  `install_npm_tool_to_staging` returning `Err`; Step 3 converts that `Err`
  into the same UI-visible `ToolCommandResult { success: false, .. }` plus a
  persisted failure state, preserving the old user-visible behavior (the UI
  shows the message and the dashboard re-reads state).
- `validate_portable_npm`, `portable_npm_command`, registry resolution, and
  `apply_portable_env` are now inside `install_npm_tool_to_staging`, so remove
  them from `install_npm_tool` (they're no longer duplicated here).

**Verify**:
- `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` → exit 0.

### Step 4: Add a unit test that staging failure leaves the prior install intact

There's an existing test fixture pattern in `portable.rs` `mod tests`
(`fixture()` at `portable.rs:2936` sets up a temp root with manifest + settings
+ package files; `AppState { root: temp.path() }`). Model the new test on
`ready_tool_rechecks_version_and_clears_stale_error` (`portable.rs:3061`) and
the codex/claude tool-definition shape used there.

Add this test in `mod tests`:

```rust
#[test]
fn npm_install_failure_preserves_existing_install_via_rollback() {
    // swap_npm_install_into_place must restore the previous install when the
    // final move fails. We force a failure by making the destination
    // non-empty AFTER staging but point staging somewhere we control, then
    // assert the backup/rollback path restores the original.
    //
    // Since triggering a real cross-volume rename failure portably is hard,
    // this test covers the rollback branch by calling swap directly with a
    // staging dir whose rename is made to fail: we create destination as a
    // FILE (not dir) while staging is a dir, so fs::rename(dir, file) errors,
    // exercising the rollback branch.
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
    fs::write(
        destination.join("node_modules").join("marker.txt"),
        "old",
    )
    .unwrap();

    // Staging tree on local temp.
    let staging = env::temp_dir()
        .join("portable-ai-dev-kit")
        .join("npm-staging-test-rollback");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(staging.join("node_modules")).unwrap();

    // Sabotage the final rename: turn destination into a FILE so
    // rename(staging_dir, file_path) fails (types differ), forcing rollback.
    let _ = fs::remove_dir_all(&destination);
    fs::write(&destination, "i-am-a-file-blocker").unwrap();

    let result = swap_npm_install_into_place(&app, &tool, staging.clone());
    assert!(result.is_err(), "swap should fail when rename is blocked");

    // Rollback restored the original install tree.
    assert!(destination.is_dir(), "destination should be restored as a dir");
    let marker = destination.join("node_modules").join("marker.txt");
    assert!(marker.exists(), "previous install content must survive rollback");
    assert_eq!(fs::read_to_string(&marker).unwrap(), "old");

    // No dangling backup left.
    let backup = app.path("cache/extract/claude-backup");
    assert!(!backup.exists(), "backup must be cleaned up on rollback path");
    let _ = fs::remove_dir_all(&staging);
}
```

> If you find that `fs::rename(dir, file)` on Windows in this test environment
> does NOT error (and instead the test can't force the rollback branch), that
> is a STOP condition — do not weaken the assertion; report it.

**Verify**: `cargo test --manifest-path src-tauri/Cargo.toml --lib npm_install_failure_preserves_existing_install_via_rollback` → test passes.

### Step 5: Add a unit test that a successful staging install lands at the tool path

Also in `mod tests`, model on the same fixture. This one fakes a "staging"
success by writing the tree ourselves, then asserts `swap_npm_install_into_place`
moves it into `tools/<id>` and removes the prior backup:

```rust
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
    assert!(new_bin.exists(), "new install must be at the tool base path");
    assert_eq!(fs::read_to_string(&new_bin).unwrap(), "new");
    // Old content is gone (it was moved to backup, then backup removed).
    assert!(!destination.join("node_modules/.old").exists());

    let backup = app.path("cache/extract/codex-backup");
    assert!(!backup.exists(), "backup must be removed after a successful swap");
}
```

**Verify**: `cargo test --manifest-path src-tauri/Cargo.toml --lib npm_staging_swap` → test passes.

## Test plan

- New tests (Steps 4 & 5) cover: (a) rollback restores the prior install when
  the final move fails — the exact data-corruption bug this plan fixes; (b)
  the happy path moves the staging tree into place and clears the backup.
- Structural pattern: `ready_tool_rechecks_version_and_clears_stale_error`
  (`portable.rs:3061`) and the inline `ToolDefinition { .. }` shape used there.
- Full suite must stay green: `cargo test --manifest-path src-tauri/Cargo.toml --lib`
  → 39 passed (37 existing + 2 new).
- Manual smoke test (optional, operator-run on the real `F:` kit): trigger an
  "update" on a large npm CLI (e.g. Claude Code) and confirm it completes within
  the 10-minute budget where it previously timed out.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml --lib` exits 0
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` exits 0
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib` exits 0; the 2 new
      tests pass and all 37 prior tests still pass
- [ ] `grep -n "fs::rename(&staging, &destination)" src-tauri/src/portable.rs` shows
      the swap call inside `swap_npm_install_into_place`
- [ ] `install_npm_tool` no longer calls `npm install --prefix` with
      `app.path(&tool.base_path)` directly (the prefix is now the local staging dir)
- [ ] No files outside `src-tauri/src/portable.rs` are modified (`git status`)
- [ ] `plans/README.md` status row for 001 updated

## STOP conditions

Stop and report back (do not improvise) if:

- `portable.rs` at the cited line ranges doesn't match the "Current state"
  excerpts (the codebase has drifted since `65bc91d`).
- A cross-volume directory rename (`%TEMP%` → portable drive) fails on the
  target Windows configuration — `swap_npm_install_into_place`'s whole premise
  is that `MoveFileExW` moves a directory tree across volumes. If it does not,
  report before adding a copy-based fallback (the fallback must still preserve
  atomicity/rollback, which needs a different design).
- Step 4's test cannot force the rollback branch on Windows (`rename(dir,file)`
  unexpectedly succeeds) — report rather than weakening the assertion.
- `apply_portable_env`'s `NPM_CONFIG_CACHE` override ordering assumption
  (setting the env after the call wins) turns out false on this Rust/Windows
  version — report so the cache redirect can be re-placed.
- A step's verification fails twice after a reasonable fix attempt.

## Maintenance notes

- **Future timeout tuning**: plan 003 may raise/make-configurable
  `NPM_INSTALL_TIMEOUT`. With staging on local SSD the 10-minute budget should
  be ample, but if a huge package still times out, the fix lives entirely in
  `install_npm_tool_to_staging`'s timeout arg — the swap logic is unaffected.
- **npm cache location**: this plan redirects the cache to local SSD only for
  the install subprocess. `apply_portable_env` (used by launch/version/login)
  still points the CLI's own cache at the portable `cache/npm`. That's intended
  (runtime cache should move with the drive); only install-time I/O is
  redirected. If someone later wants the runtime cache local too, that's a
  separate change to `apply_portable_env` — note the tradeoff (cache wouldn't
  persist across machines).
- **Reviewer focus**: confirm (1) the rollback branch in
  `swap_npm_install_into_place` restores `destination` from `backup` on every
  failure exit, (2) `patch_freebuff_index` still runs AFTER the move so it
  patches the live file, (3) no `?` on `fs::rename` swallows the rollback.
- **Deferred out of this plan**: frontend install progress, cancellation, and
  configurable timeout are plan 003; drive-speed health diagnostics are plan 005.
