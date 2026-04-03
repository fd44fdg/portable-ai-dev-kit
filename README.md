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
- AI CLI wrappers for `codex`, `gemini`, `iflow`, and `openclaude`
- manifest-driven tool detection and setup
- per-drive config and state isolation
- default portable workspace under `workspace/`
- optional MSYS2 integration for Linux-like shell experience (auto-detected)

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
powershell -File scripts\setup.ps1 -Profile full -NetworkMode china -IncludeCodex -IncludeGemini -IncludeOpenClaude
powershell -File scripts\ai-tool.ps1 -Tool codex -Action status
powershell -File scripts\ai-tool.ps1 -Tool iflow -Action login
powershell -File scripts\health-check.ps1
```

## Repository Layout

```
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

## Troubleshooting

### 常见问题

**Q: 工具显示 "missing" 但已安装**
A: 检查工具路径是否正确，运行 `scripts\health-check.ps1` 诊断

**Q: npm install 超时**
A: 使用 `-NetworkMode china` 或手动设置镜像:
```powershell
$env:npm_config_registry="https://registry.npmmirror.com/"
```

**Q: 磁盘空间不足**
A: 清理 `cache\downloads` 目录，健康检查会提示缓存大小

**Q: openclaude 启动无响应**
A: 首次运行需要配置模型:
```powershell
$env:CLAUDE_CODE_USE_OPENAI="1"
$env:OPENAI_API_KEY="sk-your-key"
F:\tools\openclaude\openclaude.cmd
```

**Q: 文件夹选择对话框不显示**
A: 确保系统支持 Windows Forms，或检查 PowerShell 执行策略

**Q: 想要 Linux 风格的终端体验**
A: 安装 MSYS2 后可以使用 `-UseMsys2` 参数运行 AI 工具:
```powershell
# 需要先手动安装 MSYS2 到 apps/msys64
.\scripts\ai-tool.ps1 -Tool iflow -Action run -UseMsys2 "hello"
```

### 手动安装 MSYS2

MSYS2 提供类 Linux 终端体验，AI 工具在此环境下运行更稳定:

1. 从 https://www.msys2.org/ 下载安装程序
2. 安装到本地后，将 `msys64` 文件夹复制到 U 盘的 `apps/msys64`
3. 或下载 portable 版本解压到对应位置

使用方式:
```powershell
# 直接运行 bash
F:\tools\msys2\msys2.cmd

# AI 工具使用 MSYS2 环境运行
.\scripts\ai-tool.ps1 -Tool gemini -Action run -UseMsys2 "你的问题"
```

### 诊断命令

```powershell
# 健康检查
.\scripts\health-check.ps1

# 查看工具状态
.\scripts\bootstrap.ps1 -EntryPoint Status

# 查看 AI 工具状态
.\scripts\ai-tool.ps1 -Tool iflow -Action status
.\scripts\ai-tool.ps1 -Tool openclaude -Action status

# 清理缓存
.\scripts\portable-kit.psm1 (手动调用 Clear-PortableCache)
```

## Documentation

- [Architecture](docs/architecture.md)
- [Publishing Checklist](docs/publishing-checklist.md)

## License

[MIT](LICENSE)
