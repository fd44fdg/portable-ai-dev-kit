@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
set "MSYS2_DIR=%~dp0..\..\apps\msys64"
set "BASH_EXE=%MSYS2_DIR%\usr\bin\bash.exe"

if exist "%BASH_EXE%" (
    call "%SCRIPT_DIR%codex-msys2.cmd" %*
) else (
    echo MSYS2 not found, using fallback mode...
    call "%SCRIPT_DIR%codex-run.cmd" %*
)
set "EXIT_CODE=%ERRORLEVEL%"
endlocal & exit /b %EXIT_CODE%