# Publishing Checklist

Use this checklist before creating or updating the public GitHub repository.

## Keep In The Repo

- `scripts/`
- `config/tool-manifest.json`
- `config/package-sources.json`
- `config/local.ps1.example`
- `docs/`
- tool wrapper scripts under `tools/`
- package metadata such as `package.json` and `package-lock.json`
- root launchers such as `Start.cmd`, `Setup.cmd`, and `Login.cmd`

## Keep Out Of The Repo

- `config/local.ps1`
- `state/`
- `logs/`
- `cache/`
- `workspace/`
- downloaded apps under `apps/`
- installed dependencies under `tools/*/node_modules/`
- temp folders such as `node_modules.partial-*`
- local shortcuts like `*.lnk`

## Pre-Push Validation

1. Run `Setup.cmd` and confirm the guided install flow still opens.
2. Run `Start.cmd` and confirm bootstrap still works with ignored files absent.
3. Run `Login.cmd` and confirm the AI tool picker still opens.
4. Run `tools\codex\codex.cmd --version`.
5. Run `tools\gemini\gemini.cmd --version`.
6. Run `tools\iflow\iflow.cmd` and confirm directory selection still works.
7. Verify `config/local.ps1` is not tracked.
8. Verify no installed binaries or secrets are staged.

## First Public Repo TODO

- choose a license
- add screenshots or demo media
- decide whether to keep tool-specific `package-lock.json` files
- add a short changelog once the structure stabilizes
