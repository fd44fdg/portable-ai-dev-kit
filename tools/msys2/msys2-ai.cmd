@echo off
setlocal EnableDelayedExpansion
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..\..") do set "USB_ROOT=%%~fI"
set "MSYS2_DIR=%USB_ROOT%\apps\msys64"
set "BASH_EXE=%MSYS2_DIR%\usr\bin\bash.exe"

if not exist "%BASH_EXE%" (
    echo MSYS2 not found at %MSYS2_DIR%
    echo Please place MSYS2 in the apps\msys64 folder
    pause
    exit /b 1
)

set "MSYS2_BIN=%MSYS2_DIR%\usr\bin"
set "MINGW64_BIN=%MSYS2_DIR%\mingw64\bin"
set "NODE_DIR=%USB_ROOT%\apps\node"
set "STATE_ROOT=%USB_ROOT%\state"
set "TOOL_DIR=%USB_ROOT%\tools"
set "HOME=%STATE_ROOT%\home"

if not exist "%HOME%" mkdir "%HOME%" >nul 2>&1

set "PATH=%MSYS2_BIN%;%MINGW64_BIN%;%NODE_DIR%;%PATH%"
set "HOME=%HOME%"
set "MSYSTEM=MINGW64"
set "MSYS2_PATH_TYPE=unix"
set "CHERE_INVOKING=1"
set "TERM=vt100"

for %%I in ("%NODE_DIR%") do set "NODE_BIN_DIR=%%~dpI"

for /f "delims=" %%I in ("%CD%") do set "CURRENT_DIR=%%~fI"
set "LINUX_CWD=/%CURRENT_DIR:~0,1%%CURRENT_DIR:~2%"
set "LINUX_CWD=!LINUX_CWD:\=/!"

if "%~1"=="" (
    "%BASH_EXE%" -l -c "cd '%LINUX_CWD%'; exec /bin/bash -i"
) else (
    set "CMD_LINE="
    :next_arg
    if "%~1"=="" goto run_cmd
    set "ARG=%~1"
    set "ARG=!ARG:\=/!"
    set "ARG=!/!%ARG:~2%!"
    if defined CMD_LINE (
        set "CMD_LINE=!CMD_LINE! '!ARG!'"
    ) else (
        set "CMD_LINE='!ARG!'"
    )
    shift
    goto next_arg

    :run_cmd
    "%BASH_EXE%" -l -c "cd '%LINUX_CWD%'; !CMD_LINE!"
)

endlocal & exit /b %ERRORLEVEL%