# Portable AI Dev Kit

Portable AI coding workstation for Windows.

Run a consistent development environment from a USB drive, external SSD, or synced folder with portable runtimes, AI CLI wrappers, per-drive state isolation, and a default workspace that follows the drive instead of the host PC.

## Why It Exists

Most AI coding setups are tied to one machine:

- runtimes are installed globally
- auth state is scattered across the host
- tool behavior changes when you switch PCs
- working directories and local config drift over time

This project packages the environment itself as a portable toolkit.

## What It Includes

- portable launchers: `Start.cmd`, `Setup.cmd`, `Login.cmd`
- portable runtime layout for Git, Node, Python, terminal, and VS Code
- AI CLI wrappers for `codex`, `gemini`, and `iflow`
- manifest-driven tool detection and setup
- per-drive config and state isolation
- default portable workspace under `workspace/`

## Key Properties

- Windows-first
- no admin rights required
- OAuth-friendly login flow
- removable-drive friendly
- framework files separated from installed artifacts

## Quick Start

1. Put the project on a removable drive or stable folder on Windows.
2. Run `Setup.cmd`.
3. Choose a network mode and install profile.
4. Run `Login.cmd` if you want to authenticate a hosted AI tool.
5. Run `Start.cmd` to bootstrap the environment.
6. Use wrappers under `tools\<name>\` for direct tool launches.

Examples:

```powershell
powershell -File scripts\setup.ps1 -Profile dev -NetworkMode global
powershell -File scripts\setup.ps1 -Profile full -NetworkMode china -IncludeCodex -IncludeGemini
powershell -File scripts\ai-tool.ps1 -Tool codex -Action status
powershell -File scripts\ai-tool.ps1 -Tool iflow -Action login
```

## Repository Layout

```text
apps/       portable runtimes and desktop apps
cache/      downloaded installers and temp artifacts
config/     manifests and local config templates
docs/       architecture and release-prep notes
logs/       bootstrap/runtime logs
scripts/    setup, bootstrap, and shared PowerShell logic
state/      portable user state kept on the drive
tools/      AI tool wrappers and tool-local package metadata
workspace/  default working directory
```

## Versioned vs Local

This repository is meant to version the toolkit framework, not a live portable environment.

Versioned:

- scripts and launchers
- manifests and config templates
- tool wrapper scripts
- package metadata
- documentation

Ignored locally:

- installed apps in `apps/`
- runtime state in `state/`
- caches and logs
- local overrides such as `config/local.ps1`
- installed dependencies such as `tools/*/node_modules/`

## Documentation

- [Architecture](docs/architecture.md)
- [Publishing Checklist](docs/publishing-checklist.md)

## License

[MIT](LICENSE)
