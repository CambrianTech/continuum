# Continuum Bootstrap for Windows — One command to install and launch.
#
# Usage (from PowerShell):
#   irm https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.ps1 | iex
#
# Or with options:
#   $env:CONTINUUM_MODE="headless"; irm ... | iex
#   $env:CONTINUUM_MODE="cli"; irm ... | iex
#   $env:CONTINUUM_MODE="browser"; irm ... | iex   (default)
#
# What it does:
#   1. Ensures WSL2 + Ubuntu are installed (GPU passthrough for CUDA)
#   2. Hands off to bootstrap.sh inside WSL — same path as Linux
#
# Why WSL2:
#   Continuum uses Unix sockets, Rust workers, and Metal/CUDA GPU compute.
#   Native Windows cannot provide these. WSL2 runs a real Linux kernel with
#   full CUDA passthrough via nvidia-smi — same performance as bare metal.

$ErrorActionPreference = "Stop"

$Mode = if ($env:CONTINUUM_MODE) { $env:CONTINUUM_MODE } else { "browser" }

Write-Host ""
Write-Host "  Continuum Bootstrap (Windows)" -ForegroundColor Cyan
Write-Host "  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Mode: $Mode" -ForegroundColor Green
Write-Host ""

# ============================================================================
# Step 1: Check if WSL2 + Ubuntu are ready
# ============================================================================

$wslExe = Get-Command wsl.exe -ErrorAction SilentlyContinue

if ($wslExe) {
    # WSL exists — check for Ubuntu distro
    $distros = wsl.exe --list --quiet 2>$null
    $hasUbuntu = $distros | Where-Object { $_ -match "Ubuntu" }

    if ($hasUbuntu) {
        # WSL2 + Ubuntu ready — run bootstrap inside it
        Write-Host "  WSL2 + Ubuntu detected" -ForegroundColor Green
        Write-Host "  Launching Continuum install inside Linux..." -ForegroundColor Yellow
        Write-Host ""

        wsl.exe bash -ic "curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.sh | bash -s -- --mode=$Mode"

        if ($LASTEXITCODE -eq 0) {
            Write-Host ""
            Write-Host "  Continuum is running!" -ForegroundColor Green
            Write-Host "  UI: http://localhost:9000" -ForegroundColor Green
            Write-Host ""
        }
        exit $LASTEXITCODE
    }
}

# ============================================================================
# Step 2: Install WSL2 + Ubuntu
# ============================================================================

Write-Host "  WSL2 not found — installing..." -ForegroundColor Yellow
Write-Host ""
Write-Host "  This requires administrator privileges." -ForegroundColor Yellow
Write-Host "  Windows will install WSL2 + Ubuntu (full Linux with GPU passthrough)." -ForegroundColor Gray
Write-Host ""

# Check if running as admin
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdmin) {
    # Re-launch as admin, passing this script
    Write-Host "  Requesting administrator access..." -ForegroundColor Yellow

    # Save continuation script that runs after WSL install + restart
    $continuationScript = @"
# Auto-continue Continuum install after WSL2 restart
`$env:CONTINUUM_MODE = "$Mode"
irm https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.ps1 | iex
"@
    $continuationPath = "$env:USERPROFILE\.continuum-bootstrap-continue.ps1"
    $continuationScript | Out-File -FilePath $continuationPath -Encoding UTF8

    # Schedule RunOnce to auto-continue after restart
    $runOnceCmd = "powershell.exe -ExecutionPolicy Bypass -File `"$continuationPath`""
    New-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce" `
        -Name "ContinuumBootstrap" `
        -Value $runOnceCmd `
        -PropertyType String `
        -Force | Out-Null

    # Elevate to install WSL
    Start-Process -Verb RunAs -FilePath "wsl.exe" -ArgumentList "--install --distribution Ubuntu" -Wait

    Write-Host ""
    Write-Host "  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
    Write-Host "  WSL2 + Ubuntu installed!" -ForegroundColor Green
    Write-Host "  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Restart your computer to finish WSL2 kernel setup." -ForegroundColor Yellow
    Write-Host "  After restart, Continuum install will continue automatically." -ForegroundColor Gray
    Write-Host ""
    Write-Host "  (A RunOnce task has been scheduled — you don't need to" -ForegroundColor Gray
    Write-Host "   remember any commands. Just restart and wait.)" -ForegroundColor Gray
    Write-Host ""

    exit 0
} else {
    # Already admin — install directly
    wsl.exe --install --distribution Ubuntu

    Write-Host ""
    Write-Host "  WSL2 + Ubuntu installed!" -ForegroundColor Green
    Write-Host ""
    Write-Host "  Restart your computer to finish WSL2 kernel setup." -ForegroundColor Yellow
    Write-Host "  After restart, Continuum install will continue automatically." -ForegroundColor Gray
    Write-Host ""

    # Schedule RunOnce
    $continuationScript = @"
`$env:CONTINUUM_MODE = "$Mode"
irm https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.ps1 | iex
"@
    $continuationPath = "$env:USERPROFILE\.continuum-bootstrap-continue.ps1"
    $continuationScript | Out-File -FilePath $continuationPath -Encoding UTF8

    $runOnceCmd = "powershell.exe -ExecutionPolicy Bypass -File `"$continuationPath`""
    New-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce" `
        -Name "ContinuumBootstrap" `
        -Value $runOnceCmd `
        -PropertyType String `
        -Force | Out-Null

    exit 0
}
