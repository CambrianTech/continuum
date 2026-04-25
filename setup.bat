@echo off
REM setup.bat -- back-compat redirect to install.ps1.
REM Continuum's canonical Windows installer is now install.ps1.
REM See docs/INSTALL-ARCHITECTURE.md for the design.
echo.
echo   setup.bat is now a redirect to install.ps1 (the canonical Windows
echo   installer). Forwarding ...
echo.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
exit /b %errorlevel%
