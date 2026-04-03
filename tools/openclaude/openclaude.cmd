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
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for OpenClaude'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do set "SELECTED_DIR=%%I"
  if not defined SELECTED_DIR endlocal & exit /b 0
)

if defined SELECTED_DIR set "WORKSPACE=%SELECTED_DIR%"

set "HOME=%STATE_ROOT%\home"
if not exist "%HOME%" mkdir "%HOME%" >nul 2>&1

for %%I in ("%WORKSPACE%") do set "WORKDIR=%%~fI"
for /f "delims=" %%I in ('powershell -NoLogo -NoProfile -Command "$w='%WORKDIR%'; $d=$w[0]; $p=$w.Substring(2).Replace('\','/'); Write-Host \"/$d$p\""') do set "LINUX_CWD=%%I"

set "TEMP_SCRIPT=%TEMP%\openclaude_msys2.bat"
set "BASH_SCRIPT=%TEMP%\openclaude_msys2.sh"

echo export PATH=/f/apps/msys64/usr/bin:/f/apps/msys64/mingw64:/d/npm-global:/f/apps/node:'$PATH' > "%BASH_SCRIPT%"
echo export HOME=/f/state/home >> "%BASH_SCRIPT%"
echo export MSYSTEM=MINGW64 >> "%BASH_SCRIPT%"
echo export MSYS2_PATH_TYPE=unix >> "%BASH_SCRIPT%"
echo cd "%LINUX_CWD%" >> "%BASH_SCRIPT%"
echo openclaude %* >> "%BASH_SCRIPT%"

echo Starting OpenClaude in MSYS2 environment...
"%BASH_EXE%" -l "%BASH_SCRIPT%"

set "EXIT_CODE=%ERRORLEVEL%"
del /q "%BASH_SCRIPT%" 2>nul

echo.
echo OpenClaude exited with code %EXIT_CODE%.
if defined INTERACTIVE_LAUNCH pause
endlocal & exit /b %EXIT_CODE%

:fallback_mode
set "WORKSPACE=%USB_ROOT%\workspace"
set "STATE_ROOT=%USB_ROOT%\state"
set "SELECTED_DIR="
set "INTERACTIVE_LAUNCH="

if "%~1"=="" (
  set "INTERACTIVE_LAUNCH=1"
  for /f "usebackq delims=" %%I in (`powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Windows.Forms; $dialog = New-Object System.Windows.Forms.FolderBrowserDialog; $dialog.Description = 'Select a project folder for OpenClaude'; $dialog.SelectedPath = [Environment]::GetFolderPath('Desktop'); if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::WriteLine($dialog.SelectedPath) }"`) do set "SELECTED_DIR=%%I"
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

for %%I in ("%HOME%" "%APPDATA%" "%LOCALAPPDATA%" "%XDG_CONFIG_HOME%" "%XDG_CACHE_HOME%" "%XDG_STATE_HOME%") do if not exist "%%~I" mkdir "%%~I" >nul 2>&1

openclaude %*
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
  echo.
  echo openclaude exited with code %EXIT_CODE%.
  if defined INTERACTIVE_LAUNCH pause
)
endlocal & exit /b %EXIT_CODE%