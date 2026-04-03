@echo off
setlocal
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
set "STATE_ROOT=%USB_ROOT%\state"
set "HOME=%STATE_ROOT%\home"

if not exist "%HOME%" mkdir "%HOME%" >nul 2>&1

set "PATH=%MSYS2_BIN%;%MINGW64_BIN%;%PATH%"
set "HOME=%HOME%"
set "MSYSTEM=MINGW64"
set "MSYS2_PATH_TYPE=unix"
set "CHERE_INVOKING=1"

if "%~1"=="" (
    "%BASH_EXE%" -l -i
) else (
    "%BASH_EXE%" -l -c "%*"
)

endlocal & exit /b %ERRORLEVEL%