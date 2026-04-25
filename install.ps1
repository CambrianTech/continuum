# install.ps1 -- Continuum installer for Windows.
#
# Usage (from any PowerShell prompt, including the default Windows
# PowerShell 5.1 -- pwsh 7 is bootstrapped if needed):
#
#   irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex
#
# Or with options:
#   $env:CONTINUUM_MODE = 'browser'   # 'browser' (default) | 'cli' | 'headless'
#   irm ... | iex
#
# COUNTERPART: install.sh. Any change to one needs a matching change in
# the other or the platforms diverge. The actual install body lives in
# bootstrap.sh; only platform-specific prereq install + Docker Desktop
# settings paths differ between this entry and the counterpart.
# See docs/INSTALL-ARCHITECTURE.md for the full design.

$ErrorActionPreference = 'Stop'

$Mode = if ($env:CONTINUUM_MODE) { $env:CONTINUUM_MODE } else { 'browser' }

function Write-Step($msg)  { Write-Host "  -> $msg" }
function Write-Ok($msg)    { Write-Host "  + $msg" -ForegroundColor Green }
function Write-Warn2($msg) { Write-Host "  ! $msg" -ForegroundColor Yellow }
function Write-Fail($msg)  { Write-Host "  x $msg" -ForegroundColor Red }

function Update-SessionPath {
    # winget mutates the User PATH in the registry but the current
    # session inherits the old PATH. Pull both Machine + User PATH
    # back from the registry so subsequent probes see freshly-
    # installed binaries.
    $machine = [Environment]::GetEnvironmentVariable('PATH', 'Machine')
    $user    = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $env:PATH = "$machine;$user"
}

Write-Host ''
Write-Host '  Continuum installer (Windows)'
Write-Host '  -----------------------------'
Write-Host "  Mode: $Mode"
Write-Host ''

# ── section: prereqs ────────────────────────────────────────────────────
# Same shape as install.sh ensure_prereqs. Auto-install the missing set
# via winget; fall through with a clear error if winget itself isn't
# available.

function Test-WingetAvailable {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Fail 'winget not found. winget ships with App Installer (Microsoft Store).'
        Write-Host '    Install/update App Installer from the Microsoft Store, then re-run.'
        Write-Host '    Direct: https://www.microsoft.com/store/productId/9NBLGGH4NNS1'
        exit 1
    }
}

function Install-IfMissing {
    param([string]$Name, [string]$WingetId, [scriptblock]$TestCmd)
    if (& $TestCmd) { Write-Ok "$Name already installed"; return }
    Write-Step "Installing $Name (winget: $WingetId) ..."
    & winget install --id $WingetId --exact --silent `
        --accept-package-agreements --accept-source-agreements `
        --disable-interactivity
    Update-SessionPath
    if (& $TestCmd) { Write-Ok "$Name installed" }
    else { Write-Warn2 "$Name install completed but probe still fails. Open a NEW shell to refresh PATH and re-run." }
}

Test-WingetAvailable

# Git: needed for the continuum.cmd shim's path resolution + dev paths.
Install-IfMissing -Name 'Git for Windows'    -WingetId 'Git.Git' `
    -TestCmd { Get-Command git -ErrorAction SilentlyContinue }

# Docker Desktop: the core runtime continuum's docker compose stack
# depends on. winget install registers + starts the service; first run
# may still require interactive accept on the EULA.
Install-IfMissing -Name 'Docker Desktop'     -WingetId 'Docker.DockerDesktop' `
    -TestCmd { Get-Command docker -ErrorAction SilentlyContinue }

# WSL2 + Ubuntu: continuum's runtime is Linux (Unix sockets, Rust
# workers, CUDA passthrough). Native Windows can't provide these.
# Install via wsl --install which requires admin + reboot the first
# time; subsequent runs are no-ops.
function Install-WSL2 {
    $wslExe = Get-Command wsl.exe -ErrorAction SilentlyContinue
    if ($wslExe) {
        $distros = & wsl.exe --list --quiet 2>$null
        $hasUbuntu = $distros | Where-Object { $_ -match 'Ubuntu' }
        if ($hasUbuntu) { Write-Ok 'WSL2 + Ubuntu already installed'; return }
    }
    Write-Step 'Installing WSL2 + Ubuntu (will require admin elevation + a reboot on first install) ...'
    $isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isAdmin) {
        Write-Warn2 'Not running as admin. WSL2 install needs admin -- relaunch this script in an elevated PowerShell:'
        Write-Host  '    Start-Process pwsh -Verb runAs -ArgumentList "-Command","irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex"'
        exit 1
    }
    & wsl.exe --install -d Ubuntu --no-launch
    Write-Warn2 'WSL2 install kicked off. Reboot when prompted, then re-run this installer.'
    exit 0
}
Install-WSL2

# ── section: docker desktop AI settings auto-toggle ─────────────────────
# Highest-leverage friction kill. Without these toggles continuum's
# personas run on CPU at ~10 tok/s instead of GPU at ~80-237 tok/s, OR
# the core container can't reach Docker Model Runner at all. Today the
# README has these as a "manual one-time step" and every fresh dev hits
# it. Programmatically write the keys + bounce Docker Desktop so the
# user never has to think about it.
#
# Key reference (from inspecting %APPDATA%\Docker\settings-store.json
# on a real Docker Desktop 4.x install with both toggles set):
#   EnableDockerAI            -- master toggle for the AI features
#   EnableInferenceGPUVariant -- "Enable GPU-backed inference" UI toggle
#   EnableInferenceTCP        -- "Enable host-side TCP support" UI toggle
#   InferenceCanUseGPUVariant -- capability flag (Docker sets, we don't)

function Set-DockerDesktopAISettings {
    $settingsPath = Join-Path $env:APPDATA 'Docker\settings-store.json'
    if (-not (Test-Path $settingsPath)) {
        Write-Warn2 "Docker Desktop settings-store.json not found at $settingsPath."
        Write-Warn2 "Docker Desktop hasn't run for the first time yet. Start Docker Desktop once, accept the EULA, then re-run this installer."
        return $false
    }
    try {
        $raw = Get-Content $settingsPath -Raw
        $cfg = $raw | ConvertFrom-Json
    } catch {
        Write-Fail "Failed to parse $settingsPath -- skipping AI toggle. Set them manually in Docker Desktop -> Settings -> AI."
        return $false
    }
    $changed = $false
    foreach ($key in @('EnableDockerAI', 'EnableInferenceGPUVariant', 'EnableInferenceTCP')) {
        if (-not $cfg.PSObject.Properties.Name.Contains($key) -or $cfg.$key -ne $true) {
            $cfg | Add-Member -NotePropertyName $key -NotePropertyValue $true -Force
            $changed = $true
        }
    }
    if (-not $changed) { Write-Ok 'Docker Desktop AI settings already enabled (GPU + host TCP)'; return $true }
    # Backup before write -- if Docker Desktop reformats the file we
    # don't want to clobber unrecoverably.
    Copy-Item $settingsPath "$settingsPath.continuum-bak" -Force -ErrorAction SilentlyContinue
    ($cfg | ConvertTo-Json -Depth 20) | Set-Content -Path $settingsPath -Encoding UTF8 -NoNewline
    Write-Ok 'Docker Desktop AI settings enabled (GPU-backed inference + host-side TCP)'
    Write-Step 'Restarting Docker Desktop so the toggles apply ...'
    try {
        Get-Process 'Docker Desktop' -ErrorAction Stop | Stop-Process -Force -ErrorAction SilentlyContinue
    } catch { }
    Start-Sleep -Seconds 2
    Start-Process "$env:ProgramFiles\Docker\Docker\Docker Desktop.exe" -ErrorAction SilentlyContinue
    return $true
}

Set-DockerDesktopAISettings | Out-Null

# Wait for Docker Desktop to be ready. If it's not running yet, start
# it and poll. Bounded wait so we never spin forever (vs setup.bat's
# old infinite wait_loop).
function Wait-DockerReady {
    param([int]$TimeoutSec = 120)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    if (-not (Get-Process 'Docker Desktop' -ErrorAction SilentlyContinue)) {
        Start-Process "$env:ProgramFiles\Docker\Docker\Docker Desktop.exe" -ErrorAction SilentlyContinue
    }
    while ((Get-Date) -lt $deadline) {
        & docker info 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) { Write-Ok 'Docker Desktop ready'; return $true }
        Start-Sleep -Seconds 3
    }
    Write-Fail "Docker Desktop didn't become ready within ${TimeoutSec}s. Open it manually and retry."
    return $false
}
Wait-DockerReady -TimeoutSec 180 | Out-Null

# ── section: continuum CLI shim ─────────────────────────────────────────
# Drops continuum.cmd into %LOCALAPPDATA%\Programs\continuum + adds
# that dir to user PATH so `continuum <verb>` works from PowerShell,
# cmd.exe, Run dialog, scheduled tasks. Same pattern as airc.cmd.

$shimDir = Join-Path $env:LOCALAPPDATA 'Programs\continuum'
$shimPath = Join-Path $shimDir 'continuum.cmd'
New-Item -ItemType Directory -Force -Path $shimDir | Out-Null
@'
@echo off
REM continuum.cmd -- Windows shim that delegates to the Linux runtime
REM inside WSL. Generated by continuum/install.ps1.
wsl bash -c "~/.local/bin/continuum %*"
'@ | Set-Content -Path $shimPath -Encoding ASCII

$userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
if (-not $userPath) { $userPath = '' }
if ($userPath -notlike "*$shimDir*") {
    $newPath = if ($userPath.Length -gt 0) { "$userPath;$shimDir" } else { $shimDir }
    [Environment]::SetEnvironmentVariable('PATH', $newPath, 'User')
    Write-Step "Added $shimDir to user PATH (open a NEW shell to pick up)"
}
Write-Ok "continuum CLI shim installed at $shimPath"

# ── section: delegate to bootstrap.sh inside WSL ────────────────────────
# bootstrap.sh is the canonical install body -- clones the repo, pulls
# docker compose images, brings the stack up, opens the browser. Runs
# inside WSL2 here on Windows.

Write-Step 'Handing off to bootstrap.sh inside WSL ...'
& wsl.exe bash -ic "curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.sh | bash -s -- --mode=$Mode"
$bootstrapExit = $LASTEXITCODE

# ── section: post-install guidance ──────────────────────────────────────
Write-Host ''
if ($bootstrapExit -eq 0) {
    Write-Ok 'Continuum is up.'
    Write-Host ''
    switch ($Mode) {
        'browser'  { Write-Host '  UI:        http://localhost:9003' }
        'cli'      { Write-Host '  CLI:       continuum   (from any new shell)' }
        'headless' { Write-Host '  Server:    http://localhost:9003 (API only)' }
    }
    Write-Host '  Verify:    continuum doctor'
    Write-Host ''
} else {
    Write-Fail "bootstrap.sh exited $bootstrapExit -- check the WSL output above for the actual failure."
    Write-Host '  Re-run any time:  irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex'
    Write-Host '  Diagnose:         continuum doctor'
}
exit $bootstrapExit
