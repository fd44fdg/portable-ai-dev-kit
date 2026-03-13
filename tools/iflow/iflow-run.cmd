@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..") do set "USB_ROOT=%%~fI"
set "WORKSPACE=%USB_ROOT%\workspace"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "NODE_EXE=%NODE_DIR%\node.exe"
set "NODE_BIN_DIR="
set "HOST_IFLOW="
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
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for iFlow'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do (
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
set "PORTABLEKIT_WORKSPACE=%WORKSPACE%"

if exist "%NODE_EXE%" (
  set "PATH=%NODE_BIN_DIR%;%PATH%"
)

for /f "delims=" %%I in ('where iflow 2^>nul') do (
  if /I not "%%~fI"=="%~f0" if /I not "%%~dpI"=="%SCRIPT_DIR%" if /I not "%%~fI"=="%USB_ROOT%\iflow.cmd" if /I "%%~fI" neq "%USB_ROOT%\tools\iflow\iflow.cmd" (
    set "HOST_IFLOW=%%~fI"
    goto run_host_iflow
  )
)

:run_host_iflow
if defined HOST_IFLOW (
  "%HOST_IFLOW%" %*
  goto finish
)

if exist "%NODE_EXE%" (
  "%NODE_EXE%" "%SCRIPT_DIR%portable-entry.mjs" %*
) else (
  node "%SCRIPT_DIR%portable-entry.mjs" %*
)

:finish
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo.
  echo iflow exited with code %EXIT_CODE%.
  if defined INTERACTIVE_LAUNCH pause
)
endlocal & exit /b %EXIT_CODE%
