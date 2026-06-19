# Windows-Only Manual Tests

This document describes tests that **cannot be executed on Linux** and must be run manually on a Windows x64 machine with the Portable AI Dev Kit installed on a removable USB drive.

---

## Plan 001 — NPM Staging Atomic Install

### Rollback test: `npm_install_failure_preserves_existing_install_via_rollback`

**Why Linux cannot test this:** On Linux, `fs::rename(dir, file)` (where the destination is a file) succeeds silently instead of failing, so the rollback branch is never exercised.

**Steps (Windows):**

1. Open the Portable AI Dev Kit on a removable drive.
2. Install a tool (e.g. Claude Code) so the tool directory exists at `tools/claude/`.
3. Verify `tools/claude/node_modules/marker.txt` (or any file inside) exists — this is the "old" install.
4. Open a PowerShell terminal as Administrator.
5. Delete the staging directory if it exists: `Remove-Item -Recurse -Force $env:TEMP\portable-ai-dev-kit\npm-staging -ErrorAction SilentlyContinue`.
6. Trigger an install/update of the same tool via the UI.
7. **Expected:** The old install (`tools/claude/node_modules/marker.txt`) is restored after the failed swap; no `cache/extract/claude-backup` directory remains.

---

## Plan 003 — Timeout + Progress UX

### Cancel button (frontend)

**Steps (Windows):**

1. Open the app on Windows.
2. Select any tool with status "Not Installed" (e.g. Claude Code).
3. Click **Install**.
4. Within 5 seconds, click the **Cancel install** button that appears next to the elapsed timer.
5. **Expected:** The install stops; the timer disappears; the tool status remains "Not Installed" (or whatever partial state resulted from the cancellation).

### Elapsed timer display

**Steps (Windows):**

1. Click **Install** on a tool that is not yet installed.
2. **Expected:** An elapsed-time counter (e.g. "Elapsed 3s") appears next to the Install button, updating every second.
3. Wait for the install to complete or fail.
4. **Expected:** The timer disappears when the action finishes.

### Install timeout setting

**Steps (Windows):**

1. Open **Settings** from the sidebar.
2. Find **Install timeout (seconds)** field.
3. Change the value from default 600 to e.g. 120.
4. Click **Save**.
5. Reopen Settings; verify the value persists as 120.
6. Click a tool and install it — the backend should now use 120s as the npm timeout.

---

## Plan 005 — Drive Speed Diagnostics

### Health check: Write-speed probe display

**Steps (Windows):**

1. Open the app on Windows.
2. Click **Refresh status** (or the app auto-refreshes on startup).
3. In the **Health Check** panel, look for the **根目录写入速度** (Root write speed) entry.
4. **Expected:** One of the following statuses appears:
   - **OK** — "写入速度正常（约 N 个文件/秒）" (N >= 200)
   - **Warning** — "写入速度偏慢" (N between 10 and 200)
   - **Error** — "写入速度极慢" (N < 10), with a suggestion to increase the timeout.

### Slow drive warning message

**Steps (Windows):**

1. Use the app on a USB 2.0 flash drive (known to be slow).
2. Click **Refresh status**.
3. **Expected:** The write-speed check shows Warning or Error with the appropriate guidance message.
