@echo off
echo.
echo   Continuum Setup
echo.

:: Check Docker
docker version >nul 2>&1
if errorlevel 1 (
    echo   Docker not found. Install Docker Desktop:
    echo   https://www.docker.com/products/docker-desktop/
    start https://www.docker.com/products/docker-desktop/
    exit /b 1
)
echo   Docker found

:: Pull pre-built images
echo.
echo   Pulling pre-built images...
docker compose pull

:: Start
echo.
echo   Starting Continuum...
docker compose up -d

:: Wait for healthy
echo.
echo   Waiting for services...
:wait_loop
timeout /t 5 /nobreak >nul
docker compose ps widget-server 2>nul | findstr "healthy" >nul
if errorlevel 1 goto wait_loop

:: Install continuum CLI (WSL shim)
echo.
echo   Installing 'continuum' command...
(echo @wsl bash -c "~/.local/bin/continuum %%*") > "%USERPROFILE%\continuum.cmd"
wsl bash -c "mkdir -p ~/.local/bin && cp src/scripts/continuum.sh ~/.local/bin/continuum && chmod +x ~/.local/bin/continuum" 2>nul
echo   Done. Run 'continuum' from any terminal.

echo.
echo   Continuum is running!
echo.
echo   Opening http://localhost:9003 ...
start http://localhost:9003
echo.
