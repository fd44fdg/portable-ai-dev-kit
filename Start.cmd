@echo off
setlocal
set "KIT_ROOT=%~dp0"
set "PORTABLE_AI_KIT_ROOT=%KIT_ROOT:~0,-1%"

if exist "%KIT_ROOT%Portable-AI-Dev-Kit.exe" (
  start "" "%KIT_ROOT%Portable-AI-Dev-Kit.exe"
  exit /b 0
)

if exist "%KIT_ROOT%src-tauri\target\release\portable-ai-dev-kit.exe" (
  start "" "%KIT_ROOT%src-tauri\target\release\portable-ai-dev-kit.exe"
  exit /b 0
)

if exist "%KIT_ROOT%node_modules\.bin\tauri.cmd" (
  call "%KIT_ROOT%node_modules\.bin\tauri.cmd" dev
  exit /b %ERRORLEVEL%
)

echo Portable AI Dev Kit 尚未完成构建或依赖安装。
echo 请先运行: npm install
echo 然后运行: npm run tauri:dev
exit /b 1
