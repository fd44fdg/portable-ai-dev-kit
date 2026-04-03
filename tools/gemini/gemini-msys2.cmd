@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..") do set "USB_ROOT=%%~fI"
set "MSYS2_DIR=%USB_ROOT%\apps\msys64"
set "BASH_EXE=%MSYS2_DIR%\usr\bin\bash.exe"

if not exist "%BASH_EXE%" (
    echo MSYS2 not found at %MSYS2_DIR%
    echo Using fallback mode...
    goto fallback_mode
)

set "WORKSPACE=%USB_ROOT%\workspace"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "NODE_EXE=%NODE_DIR%\node.exe"
set "TARGET_JS=%SCRIPT_DIR%node_modules\@google\gemini-cli\bundle\gemini.js"
set "STATE_ROOT=%USB_ROOT%\state"
set "SELECTED_DIR="
set "INTERACTIVE_LAUNCH="

if not exist "%NODE_EXE%" (
  for /d %%I in ("%NODE_DIR%\node-v*-win-*") do (
    if exist "%%~fI\node.exe" set "NODE_EXE=%%~fI\node.exe"
  )
)

if "%~1"=="" (
  set "INTERACTIVE_LAUNCH=1"
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for Gemini'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do set "SELECTED_DIR=%%I"
  if not defined SELECTED_DIR endlocal & exit /b 0
)

if defined SELECTED_DIR set "WORKSPACE=%SELECTED_DIR%"

set "HOME=%STATE_ROOT%\home"
if not exist "%HOME%" mkdir "%HOME%" >nul 2>&1

if not exist "%TARGET_JS%" (
    echo gemini is not installed yet.
    endlocal & exit /b 1
)

set "NODE_CMD=/f/apps/node/node.exe"
set "TARGET_LINUX=/f/tools/gemini/node_modules/@google/gemini-cli/bundle/gemini.js"

for %%I in ("%WORKSPACE%") do set "WORKDIR=%%~fI"
for /f "delims=" %%I in ('powershell -NoLogo -NoProfile -Command "$w='%WORKDIR%'; $d=$w[0]; $p=$w.Substring(2).Replace('\','/'); Write-Host \"/$d$p\""') do set "LINUX_CWD=%%I"

echo Starting Gemini in MSYS2 environment...
set "PATH=C:\Windows\System32;%PATH%"
"%BASH_EXE%" --norc --noprofile -c "export PATH=/f/apps/msys64/usr/bin:/f/apps/msys64/mingw64:/f/apps/node:/c/Windows/System32:$PATH; export HOME=/f/state/home; export MSYSTEM=MINGW64; cd '%LINUX_CWD%'; %NODE_CMD% '%TARGET_LINUX%'"

set "EXIT_CODE=%ERRORLEVEL%"

echo.
echo Gemini exited with code %EXIT_CODE%.
if defined INTERACTIVE_LAUNCH pause
endlocal & exit /b %EXIT_CODE%

:fallback_mode
set "WORKSPACE=%USB_ROOT%\workspace"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "NODE_EXE=%NODE_DIR%\node.exe"
set "TARGET_JS=%SCRIPT_DIR%node_modules\@google\gemini-cli\bundle\gemini.js"
set "STATE_ROOT=%USB_ROOT%\state"
set "SELECTED_DIR="
set "INTERACTIVE_LAUNCH="

if not exist "%NODE_EXE%" (
  for /d %%I in ("%NODE_DIR%\node-v*-win-*") do (
    if exist "%%~fI\node.exe" set "NODE_EXE=%%~fI\node.exe"
  )
)

if "%~1"=="" (
  set "INTERACTIVE_LAUNCH=1"
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for Gemini'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do set "SELECTED_DIR=%%I"
  if not defined SELECTED_DIR endlocal & exit /b 0
)

if defined SELECTED_DIR set "WORKSPACE=%SELECTED_DIR%"

if exist "%WORKSPACE%" cd /d "%WORKSPACE%"

set "HOME=%STATE_ROOT%\home"
set "USERPROFILE=%STATE_ROOT%\home"
set "APPDATA=%STATE_ROOT%\appdata"
set "LOCALAPPDATA=%STATE_ROOT%\localappdata"
set "XDG_CONFIG_HOME=%STATE_ROOT%\xdg\config"
set "XDG_CACHE_HOME=%STATE_ROOT%\xdg\cache"
set "XDG_STATE_HOME=%STATE_ROOT%\xdg\state"
set "GEMINI_CLI_HOME=%STATE_ROOT%\gemini"

for %%I in ("%HOME%" "%APPDATA%" "%LOCALAPPDATA%" "%XDG_CONFIG_HOME%" "%XDG_CACHE_HOME%" "%XDG_STATE_HOME%" "%GEMINI_CLI_HOME%") do if not exist "%%~I" mkdir "%%~I" >nul 2>&1

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
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
  echo.
  echo gemini exited with code %EXIT_CODE%.
  if defined INTERACTIVE_LAUNCH pause
)
endlocal & exit /b %EXIT_CODE%