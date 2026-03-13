# Portable AI Dev Kit

Windows-first portable AI workstation for a USB drive, external SSD, or synced folder.

The goal is simple: plug the drive into a Windows machine, run `Setup.cmd` once, then use `Start.cmd` / `Login.cmd` / tool wrappers to recover a familiar AI-assisted development environment without assuming admin rights.

## What This Solves

- portable runtimes such as Git, Node, Python, terminal, and VS Code
- portable AI CLI wrappers for tools like Codex, Gemini, and iFlow
- per-drive state isolation so auth/session/config stay off the host machine
- manifest-driven setup and bootstrap instead of one-off machine scripting
- a default workspace that follows the drive, not the current PC

## Current Status

Implemented now:

- bootstrap flow from `Start.cmd`
- guided install flow from `Setup.cmd`
- single-tool login flow from `Login.cmd`
- manifest-driven tool detection in `config/tool-manifest.json`
- package source profiles in `config/package-sources.json`
- portable wrapper layer for `codex`, `gemini`, and `iflow`
- local configuration override via `config/local.ps1`

Partially implemented:

- portable app/runtime download and install flow
- staged architecture for workspace recovery and hardening
- shared CLI/login orchestration for AI tools

Planned later:

- stronger recovery/update flows
- more tools and health checks
- secret/log hardening
- workspace sync helpers

## Quick Start

1. Put this project on a removable drive or stable folder.
2. Run `Setup.cmd` on Windows.
3. Pick a network mode and install profile.
4. Run `Login.cmd` if you want to authenticate a hosted AI CLI.
5. Run `Start.cmd` to bootstrap the environment.
6. Launch individual tools from `tools\<name>\<name>.cmd` when needed.

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
docs/       architecture and design notes
logs/       bootstrap/runtime logs
scripts/    setup, bootstrap, and shared PowerShell logic
state/      portable user state kept on the drive
tools/      AI tool wrappers and tool-local package metadata
workspace/  default working directory
```

See [architecture.md](/F:/docs/architecture.md) for the staged design and [publishing-checklist.md](/F:/docs/publishing-checklist.md) for release prep.

## Publishing Notes

This repo is intended to version the framework, not your live portable environment.

Before pushing to GitHub, keep these categories out of version control:

- `state/`, `logs/`, `cache/`, and `workspace/`
- downloaded portable apps under `apps/`
- installed dependencies under `tools/*/node_modules/`
- machine-local config such as `config/local.ps1`
- shortcuts, temp folders, and partial installs

The included `.gitignore` is set up with that model in mind.

## Design Principles

- Windows-first, because the target environment is Windows
- portable by default, with minimal host-machine assumptions
- explicit install and login flows instead of hidden side effects
- local state isolation so the host PC stays clean
- manifest-driven orchestration so adding tools stays systematic

## Good Next Steps For GitHub

- add a license
- add screenshots or a short demo GIF
- add a release checklist for first-time setup validation
- split “framework files” from “installed artifacts” even more aggressively over time
