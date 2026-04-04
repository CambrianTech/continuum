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

echo.
echo   Continuum is running!
echo.
echo   Opening http://localhost:9003 ...
start http://localhost:9003
echo.
