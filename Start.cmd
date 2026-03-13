@echo off
setlocal
for %%I in ("%~dp0.") do set "USB_ROOT=%%~fI\"

powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%USB_ROOT%scripts\bootstrap.ps1" -UsbRoot "%USB_ROOT%" -EntryPoint Start
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
    echo.
    echo Bootstrap failed with exit code %EXIT_CODE%.
    pause
)

endlocal & exit /b %EXIT_CODE%
