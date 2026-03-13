# Architecture

## Objective

Build a Windows-first portable AI development kit that restores the user's working environment from a USB drive with minimal host-machine assumptions.

## Constraints

- Host machine is Windows.
- Host machine can access the internet.
- USB storage is allowed.
- OAuth login is available.
- Admin rights must not be assumed.
- Tools should prefer portable binaries and per-user state.

## Design

The kit is split into four layers.

### 1. Launch Layer

Files:

- `Start.cmd`
- `scripts/bootstrap.ps1`

Responsibilities:

- determine the USB root reliably
- create required folders on first launch
- load local configuration
- populate `PATH` with portable runtimes and tools
- detect missing components
- open the preferred editor or terminal

### 2. Tooling Layer

Folders:

- `apps/`
- `tools/`
- `cache/`

Responsibilities:

- host portable applications such as VS Code, Git, Node, Python, and a terminal
- host AI CLI tools and wrappers
- cache downloaded artifacts and install metadata on the USB drive

### 3. Identity and Config Layer

Folders:

- `config/`
- `state/`

Responsibilities:

- keep portable settings on the drive
- prefer OAuth flows over static API keys
- store non-secret defaults in versionable files
- isolate machine-specific state from shared config

### 4. Workspace Layer

Folders:

- `workspace/`
- `logs/`

Responsibilities:

- provide a default working directory
- collect bootstrap logs for diagnosis

## Staged Implementation Plan

### Stage 1: Foundation

Implemented now.

- create standard directory layout
- implement startup entrypoint
- implement manifest-driven bootstrap checks
- add local config template

### Stage 2: Tool Acquisition

Partially implemented.

- add local archive install script for portable tools
- add one-click setup script with download profiles
- add checksum and version tracking in `state/`
- add first-run install mode and update mode
- later add direct download support

### Stage 3: AI CLI Integration

Partially implemented.

- add wrappers for Codex CLI and Gemini CLI
- add wrapper and package metadata for iFlow CLI
- add a shared `ai-tool.ps1` dispatcher for `status`, `login`, and `run`
- add `login-menu.ps1` so users choose a single tool to authenticate
- normalize login and startup UX
- later add health checks for browser-based OAuth and network access

### Stage 4: Workspace Recovery

Later.

- optional repo sync helpers
- extension/profile restore for VS Code
- prompt and shell profile injection

### Stage 5: Hardening

Later.

- redact secrets from logs
- optional encrypted local secret store
- recovery script for damaged or partially populated drives

## Directory Layout

```text
F:\
  apps\
    git\
    node\
    python\
    terminal\
    vscode\
  cache\
    downloads\
    tools\
  config\
    local.ps1
    local.ps1.example
    tool-manifest.json
  docs\
    architecture.md
  logs\
  scripts\
    add-ai-tool-template.ps1
    ai-tool.ps1
    bootstrap.ps1
    install-tools.ps1
    login-menu.ps1
    setup.ps1
  state\
    bootstrap-state.json
  tools\
    codex\
    gemini\
    iflow\
  workspace\
  Login.cmd
  Setup.cmd
  Start.cmd
```

## Operating Model

1. User runs `Start.cmd`.
2. Bootstrap creates missing folders.
3. Bootstrap loads `config\local.ps1` if present.
4. Bootstrap reads `config\tool-manifest.json`.
5. Bootstrap adds discovered portable binaries to `PATH`.
6. Bootstrap reports missing tools and suggested next actions.
7. Bootstrap opens the preferred portable app if available, otherwise falls back to PowerShell in `workspace\`.

## Current Installer Mode

`scripts\install-tools.ps1` supports installing a tool from a local archive into the manifest-defined target path.

`scripts\setup.ps1` adds the simplified distribution flow:

- `lite` installs `Node + iFlow`
- `dev` adds portable Git
- `studio` adds portable VS Code
- `full` adds Python and the portable terminal
- `codex` and `gemini` are optional add-ons selected during setup
- `china` mode switches npm installs to `registry.npmmirror.com`
- package sources live in `config\package-sources.json`

## Practical Recommendation

Treat the USB drive as a portable control plane and cache, not as a frozen monolith. Keep the launch path local and stable, but let individual tools update through explicit install scripts in later stages.
