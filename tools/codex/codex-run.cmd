@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..") do set "USB_ROOT=%%~fI"
set "WORKSPACE=%USB_ROOT%\workspace"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "NODE_EXE=%NODE_DIR%\node.exe"
set "NODE_BIN_DIR="
set "TARGET_CMD=%SCRIPT_DIR%node_modules\.bin\codex.cmd"
set "TARGET_EXE=%SCRIPT_DIR%bin\codex.exe"
set "SELECTED_DIR="
set "INTERACTIVE_LAUNCH="

if not exist "%NODE_EXE%" (
  for /d %%I in ("%NODE_DIR%\node-v*-win-*") do (
    if exist "%%~fI\node.exe" (
      set "NODE_EXE=%%~fI\node.exe"
      goto node_ready
    )
  )
)

:node_ready
for %%I in ("%NODE_EXE%") do set "NODE_BIN_DIR=%%~dpI"

if "%~1"=="" (
  set "INTERACTIVE_LAUNCH=1"
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for Codex'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do (
    set "SELECTED_DIR=%%I"
  )

  if not defined SELECTED_DIR (
    endlocal & exit /b 0
  )
)

if defined SELECTED_DIR (
  set "WORKSPACE=%SELECTED_DIR%"
)

if exist "%WORKSPACE%" (
  cd /d "%WORKSPACE%"
)

if exist "%NODE_EXE%" (
  set "PATH=%NODE_BIN_DIR%;%PATH%"
)

if exist "%TARGET_CMD%" goto run_cmd

if exist "%TARGET_EXE%" goto run_exe

echo codex is not installed yet.
endlocal & exit /b 1

:run_cmd
if "%~1"=="" (
  call "%TARGET_CMD%"
) else (
  call "%TARGET_CMD%" %*
)
goto finish

:run_exe
if "%~1"=="" (
  "%TARGET_EXE%"
) else (
  "%TARGET_EXE%" %*
)

:finish
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo.
  echo codex exited with code %EXIT_CODE%.
  if defined INTERACTIVE_LAUNCH pause
)
endlocal & exit /b %EXIT_CODE%
