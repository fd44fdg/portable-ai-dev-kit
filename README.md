# Portable AI Dev Kit

Portable AI Dev Kit is a Windows x64 desktop control center for carrying AI developer tools on a USB drive, external SSD, or portable work disk.

It is designed for people who frequently switch computers and cannot rely on the host machine having Node.js, Git, Codex CLI, Claude Code, Gemini CLI, or matching configuration already installed.

[中文说明](README.zh-CN.md)

![Portable AI Dev Kit screenshot](docs/screenshot.png)

## What It Does

- Provides a `Tauri + React + TypeScript` desktop GUI.
- Initializes a portable workspace under the current drive root.
- Installs and manages portable runtimes such as Node.js and Git.
- Installs and manages AI CLI tools:
  - Codex CLI: `@openai/codex`
  - Claude Code: `@anthropic-ai/claude-code`
  - Gemini CLI: `@google/gemini-cli`
- Shows installed versions, target sources, install paths, launch entries, and host-machine availability.
- Supports one-click install, update, uninstall, login, and launch actions.
- Redirects `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`, and `XDG_*` paths to the portable `state/` directory when launching AI tools.
- Keeps the host system clean by avoiding system PATH changes and administrator-only setup.

## Portable Semantics

The app distinguishes between:

- **Portable installed**: the tool exists inside this kit and can move with the drive.
- **Host available**: the tool exists on the current computer, but it will not be available on another computer.

Host detection is shown only as a hint. A tool is considered portable-ready only after it is installed into the current drive by this kit.

## Requirements

- Windows x64.
- Internet access for first-time runtime and AI CLI installation.
- No administrator permission is required for normal use.

## Quick Start

From the project directory:

```powershell
npm install --cache .\cache\npm --registry https://registry.npmjs.org/
npm run tauri:build
.\src-tauri\target\release\portable-ai-dev-kit.exe
```

For networks that work better with the npm mirror:

```powershell
npm install --cache .\cache\npm --registry https://registry.npmmirror.com/
```

You can also run:

```powershell
.\Start.cmd
```

`Start.cmd` launches the release executable when it exists, or falls back to development mode when dependencies are installed.

## Development

```powershell
npm install --cache .\cache\npm --registry https://registry.npmjs.org/
npm run tauri:dev
```

## Verification

```powershell
npm run build
cargo test --manifest-path src-tauri\Cargo.toml --lib
cargo clippy --manifest-path src-tauri\Cargo.toml --lib -- -D warnings
```

## Directory Layout

```text
apps/       Portable runtimes such as Node.js and Git
cache/      Download and npm cache
config/     Tool manifest and app settings
docs/       Project documentation and screenshots
logs/       Operation logs
scripts/    Recovery and helper scripts
state/      Portable HOME, APPDATA, XDG state, and tool-state.json
tools/      AI CLI installations
workspace/  Default working directory
```

The source-controlled configuration lives mainly in `config/tool-manifest.json`. Local runtime state, installed tools, caches, and build artifacts are intentionally ignored by Git.

## Current Scope

The first version targets Windows x64 only. macOS and Linux can be considered later, but they are not part of the current compatibility promise.

## Notes

- First-time installation of Node.js, Git, and AI CLIs requires network access.
- The Tauri installer bundle is disabled for now; the release output is a portable `.exe`.
- OAuth and CLI state are redirected into the portable `state/` directory where the CLI supports standard environment variables.
