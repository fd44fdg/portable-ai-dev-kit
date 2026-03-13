@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..") do set "USB_ROOT=%%~fI"
set "WORKSPACE=%USB_ROOT%\workspace"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "NODE_EXE=%NODE_DIR%\node.exe"
set "NODE_BIN_DIR="
set "TARGET_JS=%SCRIPT_DIR%node_modules\@google\gemini-cli\dist\index.js"
set "HOST_GEMINI="
if defined PORTABLEKIT_HOST_APPDATA set "HOST_GEMINI=%PORTABLEKIT_HOST_APPDATA%\npm\gemini.cmd"
if not defined HOST_GEMINI if defined APPDATA set "HOST_GEMINI=%APPDATA%\npm\gemini.cmd"
set "SELECTED_DIR="
set "STATE_ROOT=%USB_ROOT%\state"
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
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for Gemini'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do (
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

set "HOME=%STATE_ROOT%\home"
set "USERPROFILE=%STATE_ROOT%\home"
set "APPDATA=%STATE_ROOT%\appdata"
set "LOCALAPPDATA=%STATE_ROOT%\localappdata"
set "XDG_CONFIG_HOME=%STATE_ROOT%\xdg\config"
set "XDG_CACHE_HOME=%STATE_ROOT%\xdg\cache"
set "XDG_STATE_HOME=%STATE_ROOT%\xdg\state"
set "GEMINI_CLI_HOME=%STATE_ROOT%\gemini"

for %%I in ("%HOME%" "%APPDATA%" "%LOCALAPPDATA%" "%XDG_CONFIG_HOME%" "%XDG_CACHE_HOME%" "%XDG_STATE_HOME%" "%GEMINI_CLI_HOME%") do (
  if not exist "%%~I" mkdir "%%~I" >nul 2>&1
)

if exist "%TARGET_JS%" goto run_portable

if exist "%HOST_GEMINI%" goto run_host

echo gemini is not installed yet.
endlocal & exit /b 1

:run_portable
if exist "%NODE_EXE%" (
  if "%~1"=="" (
    "%NODE_EXE%" "%TARGET_JS%"
  ) else (
    "%NODE_EXE%" "%TARGET_JS%" %*
  )
) else (
  if "%~1"=="" (
    node "%TARGET_JS%"
  ) else (
    node "%TARGET_JS%" %*
  )
)
goto finish

:run_host
if "%~1"=="" (
  call "%HOST_GEMINI%"
) else (
  call "%HOST_GEMINI%" %*
)

:finish
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo.
  echo gemini exited with code %EXIT_CODE%.
  if defined INTERACTIVE_LAUNCH pause
)
endlocal & exit /b %EXIT_CODE%
