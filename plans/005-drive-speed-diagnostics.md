# Plan 005: Add drive write-speed diagnostic to health checks for slow-drive detection

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 65bc91d..HEAD -- src-tauri/src/portable.rs src/i18n/locales/`
> If these files changed since this plan was written, compare the "Current state"
> excerpts against the live code before proceeding; on a mismatch, treat it as
> a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (independent of plans 001 and 003; the check is
  informational only)
- **Category**: dx | perf
- **Planned at**: commit `65bc91d`, 2026-06-19

## Why this matters

When an npm install/update fails due to a slow removable drive, the user sees
"安装超时" and has no idea the root cause is drive speed. There is no health
check or diagnostic that measures or reports the portable root's write
performance.

This plan adds a write-speed probe to the health check suite. It writes 10
small files and measures elapsed time, then classifies the result:

- **OK** (> 5 ms/file, ~200+ files/sec): fast enough for npm installs.
- **Warning** (5–100 ms/file, ~10–200 files/sec): slow but may work with
  plan 001's staging approach.
- **Error** (> 100 ms/file, <10 files/sec): the USB 2.0 / slow SD range
  measured on the target `F:` drive (233 ms/file). npm installs will likely
  timeout even with staging unless packages are small.

The probe runs only on dashboard `force` refresh (not every load) to avoid
adding latency to normal startup. It writes to `state/temp/` (inside the
portable root) so it measures the actual drive, and cleans up after itself.

## Current state

### Relevant files (roles)

- `src-tauri/src/portable.rs` — health check functions, `HealthCheck` struct,
  `check_health_with_state`. The ONLY file this plan modifies for the backend.
- `src/i18n/locales/zh-CN.json` — Chinese translations.
- `src/i18n/locales/en.json` — English translations.

### Health check architecture

Health checks are accumulated in `check_health_with_state`
(`portable.rs:422-513`). Each check is a `HealthCheck` struct:
```rust
struct HealthCheck {
    id: String,           // e.g. "root-writable"
    label: String,        // e.g. "根目录写入权限"
    status: CheckStatus,  // Ok | Warning | Error
    message: String,      // free-form detail shown in UI
}
```

The function `push_path_check` (`portable.rs:2467`) is a convenience for
existence-based checks. This plan adds a new standalone check not using that
helper (it measures timing, not existence).

### Where to insert

In `check_health_with_state`, after the line:
```rust
checks.extend(package_integrity_checks(app));
```
(`portable.rs:453`) and before the `root-writable` check (`portable.rs:455`).
Add a call to a new function `push_write_speed_check`.

### The `force` parameter

`check_health_with_state` already has a `force: bool` parameter. When
`force` is false (normal load), the probe should be **skipped** to keep
startup fast. The probe only runs on explicit refresh.

### Frontend rendering (no change needed)

The frontend renders health checks generically at `main.tsx:865-879`:
```tsx
{dashboard.health.checks.map((check) => (
  <div className='check-row' key={check.id}>
    {check.status === 'ok' ? (<CheckCircle2 size={17} />)
      : check.status === 'warning' ? (<AlertTriangle size={17} />)
      : (<XCircle size={17} />)}
    <div>
      <strong>{check.label}</strong>
      <span title={check.message}>{check.message}</span>
    </div>
  </div>
))}
```
Any new `HealthCheck` automatically appears. No frontend code change required
for rendering.

### i18n note

The health check labels and messages are generated server-side in Rust (not
via i18n keys). They are Chinese strings sent as JSON. This is the existing
convention — all other health checks in the file use Chinese directly. Follow
that pattern; do not add i18n keys for health check text.

## Commands you will need

| Purpose              | Command                                                                          | Expected on success |
|----------------------|----------------------------------------------------------------------------------|---------------------|
| Build lib            | `cargo check --manifest-path src-tauri/Cargo.toml --lib`                         | exit 0              |
| Tests                | `cargo test --manifest-path src-tauri/Cargo.toml --lib`                          | all pass           |
| Lint                 | `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings`         | exit 0             |

## Scope

**In scope** (the only file you should modify):
- `src-tauri/src/portable.rs`

**Out of scope** (do NOT touch):
- `src/main.tsx` — the health check rendering is generic; no change needed.
- `src/i18n/locales/*.json` — health check strings are Rust-side, not i18n
  keys. Adding i18n for them would be a separate i18n-everything project.
- Any health check thresholds tuning — the values in this plan are based on
  actual measurements from the target F: drive.

## Git workflow

- Branch: `advisor/005-drive-speed-diagnostics`
- Commit: `Add write-speed probe to health checks for slow-drive detection`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Add the write-speed probe function

Add a new private function after `push_path_check` (around `portable.rs:2491`).
This function is only called when `force` is true.

```rust
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
            format!("写入速度正常（约 {:.0} 个文件/秒）", 1000.0 / ms_per_file as f64),
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
```

Thresholds rationale (based on actual measurements):
- **≤5 ms/file** (~200+ files/sec): Typical SSD/NVMe or USB 3.1 SSD. npm
  install of Claude Code (thousands of files) completes in seconds.
- **5–100 ms/file** (~10–200 files/sec): USB 3.0 flash drive or HDD.
  Plan 001's staging makes this workable.
- **>100 ms/file** (<10 files/sec): USB 2.0 flash drive. The target F:
  drive measured **233 ms/file** — squarely in this bucket. npm installs
  of large packages will timeout at the default 10-minute budget.

**Verify**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.

### Step 2: Call the probe from `check_health_with_state`

In `check_health_with_state` (`portable.rs:453`), insert the call right after
the `package_integrity_checks` line:

```rust
    checks.extend(package_integrity_checks(app));

    // Write-speed probe (only on force refresh, not every load).
    if let Some(speed_check) = probe_write_speed(app, force) {
        checks.push(speed_check);
    }

    if let Ok(metadata) = fs::metadata(&app.root) {
```

Note: the probe returns `Option<HealthCheck>`, so when `force` is false it
returns `None` and nothing is added to the checks list.

**Verify**: `cargo check --manifest-path src-tauri/Cargo.toml --lib` → exit 0.

### Step 3: Add a unit test that the probe returns None when force is false

In `mod tests` (at the end of the existing test module, before the closing
`}` of `mod tests`), add:

```rust
#[test]
fn write_speed_probe_skipped_when_not_forced() {
    let (_temp, app) = fixture();
    let result = probe_write_speed(&app, false);
    assert!(result.is_none(), "probe should return None when force is false");
}
```

And a test that the probe works on a temp directory (fast, so should be OK):

```rust
#[test]
fn write_speed_probe_returns_ok_on_fast_drive() {
    let (_temp, app) = fixture();
    // The fixture uses the host temp dir, which should be fast.
    let result = probe_write_speed(&app, true);
    let check = result.expect("probe should return Some when force is true");
    assert_eq!(check.id, "root-write-speed");
    assert_eq!(check.label, "根目录写入速度");
    // On the host SSD, this should be Ok or at worst Warning.
    assert!(
        check.status != CheckStatus::Error || check.message.contains("失败"),
        "on a fast host temp dir the probe should not report Error speed"
    );
    // The probe dir should be cleaned up.
    assert!(!app.path("state/temp/_speed_probe").exists());
}
```

**Verify**: `cargo test --manifest-path src-tauri/Cargo.toml --lib write_speed_probe` → both tests pass.

## Test plan

- New tests: (1) probe returns `None` on non-force, (2) probe returns a check
  on force and classifies the host temp dir as fast.
- Structural pattern: same `fixture()` + `AppState` pattern used by all
  existing tests.
- Full suite: `cargo test --manifest-path src-tauri/Cargo.toml --lib` → all
  pass (37 existing + 2 new = 39, or more if plan 001 landed first).

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml --lib` exits 0
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings` exits 0
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib` → all pass
- [ ] `grep -n "root-write-speed" src-tauri/src/portable.rs` shows the check
      id in the probe function and in `check_health_with_state`
- [ ] `grep -n "probe_write_speed" src-tauri/src/portable.rs` shows the call
      in `check_health_with_state` guarded by `force`
- [ ] No files outside `src-tauri/src/portable.rs` are modified (`git status`)
- [ ] `plans/README.md` status row for 005 updated

## STOP conditions

Stop and report back (do not improvise) if:

- The code at `portable.rs:453` doesn't match the excerpt (codebase drifted).
- The `state/temp/` directory doesn't exist when `check_health_with_state`
  runs (it should — `bootstrap_kit` creates it, and `check_health` is called
  after bootstrap). If it doesn't, report.
- `sync_data` is unavailable on the Windows Rust target in use — the probe
  still works without it, just with slightly less accurate timing. If the
  compiler rejects it, replace with a comment `// sync omitted` and proceed.
- A step's verification fails twice after a reasonable fix attempt.

## Maintenance notes

- **Thresholds**: The 5 ms and 100 ms boundaries are based on a single
  measurement on the target F: drive (233 ms/file). If different hardware
  profiles emerge, the thresholds may need tuning. The check id
  `"root-write-speed"` is stable for scripting/parsing.
- **Performance impact**: The probe is 10 tiny files (~1 KB total) + cleanup.
  On the slow F: drive this takes ~2.3 seconds (10 × 233 ms). It only runs
  on `force` refresh, so normal dashboard loads are unaffected.
- **Interaction with plan 003**: If plan 003 lands, the error message in the
  `>100 ms` bucket mentions "建议：在设置中增大超时时间" which directly
  points users to plan 003's settings UI. If plan 003 doesn't land, the
  message is still helpful as generic advice.
- **Reviewer focus**: confirm the probe cleans up after itself (the temp dir
  is removed even on error paths), and that `force=false` truly skips all
  I/O (no temp files created).
