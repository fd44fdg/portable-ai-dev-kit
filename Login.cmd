@echo off
setlocal
for %%I in ("%~dp0.") do set "USB_ROOT=%%~fI\"

powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%USB_ROOT%scripts\login-menu.ps1" -UsbRoot "%USB_ROOT%"
set "EXIT_CODE=%ERRORLEVEL%"

if not "%EXIT_CODE%"=="0" (
    echo.
    echo Login menu exited with code %EXIT_CODE%.
    pause
)

endlocal & exit /b %EXIT_CODE%
