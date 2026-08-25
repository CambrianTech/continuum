# install.ps1 -- Continuum native installer for Windows.
#
# ONE approach, every platform: provision the toolchain (modular, idempotent,
# ONE prompt, auto-updating) -> build native -> run. This is the Windows shim of
# the same contract Unix implements in tools/scripts/install.sh. Re-running picks
# up new/updated deps; it asks for elevation AT MOST ONCE (via gsudo's credential
# cache -- the Windows twin of ensure_sudo_warmed). See
# docs/infrastructure/INSTALL-ARCHITECTURE.md.
#
# Usage:
#   # Remote one-liner (bootstraps: clones the repo, then builds native):
#   irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex
#
#   # From a checkout:
#   powershell -ExecutionPolicy Bypass -File .\install.ps1          # local-only
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -Grid    # + GitHub login for grid
#
# Docker remains available as a RUNTIME for grid nodes (docker compose up); it is
# NOT a second install path. COUNTERPART: tools/scripts/install.sh (Unix). A
# change to the install CONTRACT belongs in both.

[CmdletBinding()]
param(
    [switch]$Grid
)

$ErrorActionPreference = 'Stop'

#  Bootstrap: make the remote `irm | iex` one-liner work for the native build 
# When piped, $PSScriptRoot is empty and there is no repo yet. Inline the minimum
# to get one (winget + git, both per-user / no admin), clone, then re-invoke the
# cloned install.ps1 which has a real $PSScriptRoot. Mirrors the root install.sh
# bootstrapper.
if (-not $PSScriptRoot) {
    Write-Host '  Continuum installer (bootstrap) -- fetching the repo for a native build ...'
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Host '  winget not found. Install App Installer from the Microsoft Store, then re-run.' -ForegroundColor Red
        Write-Host '    https://www.microsoft.com/store/productId/9NBLGGH4NNS1'
        exit 1
    }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host '  -> Installing Git (per-user) ...'
        & winget install --id Git.Git --exact --silent --accept-package-agreements --accept-source-agreements --scope user
        $m = [Environment]::GetEnvironmentVariable('PATH', 'Machine'); $u = [Environment]::GetEnvironmentVariable('PATH', 'User')
        $env:PATH = "$m;$u"
    }
    $target = Join-Path $env:USERPROFILE 'continuum'
    if (-not (Test-Path (Join-Path $target '.git'))) {
        & git clone https://github.com/CambrianTech/continuum.git $target
    }
    $bootArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $target 'install.ps1'))
    if ($Grid) { $bootArgs += '-Grid' }
    & (Get-Process -Id $PID).Path @bootArgs
    exit $LASTEXITCODE
}

#  From-checkout path 
$RepoRoot = $PSScriptRoot
$LibDir = Join-Path $RepoRoot 'tools\scripts\lib'
. (Join-Path $LibDir 'install-common.ps1')
. (Join-Path $LibDir 'win-modules.ps1')

$WantsGrid = $Grid -or ($env:CONTINUUM_GRID -eq '1')

Write-Host ''
Write-Host '  Continuum installer (Windows, native)'
Write-Host '  -------------------------------------'
Write-Host "  Repo:  $RepoRoot"
if ($WantsGrid) { Write-Host '  Grid:  yes (GitHub login)' } else { Write-Host '  Grid:  no (local-only)' }
Write-Host ''

Test-WingetAvailable

try {
    # Git + vendored submodules (llama.cpp, whisper.cpp) -- the native build needs
    # them. Per-user, no elevation.
    Install-IfMissing -Name 'Git' -WingetId 'Git.Git' `
        -TestCmd { Get-Command git -ErrorAction SilentlyContinue } -UserScope
    if (Get-Command git -ErrorAction SilentlyContinue) {
        Push-Location $RepoRoot
        try { & git submodule update --init --recursive } finally { Pop-Location }
    }

    # Toolchain. Per-user tools first (rustup -- no prompt); machine-scope tools
    # (VS Build Tools, CMake, LLVM, CUDA, gh) share the SINGLE gsudo UAC.
    Mod-Rust
    Mod-VSBuildTools
    Mod-CMake
    # Beside CMake, not buried in the llama-server build: ninja is what makes the
    # cmake CONFIGURE step deterministic across Visual Studio versions (cmake
    # auto-picks the newest VS, and "Visual Studio 18 2026" is a generator cmake
    # 3.30.x cannot name). Provisioning it here means a plain `cargo build` works
    # from a fresh terminal; provisioning it lazily meant it only existed on boxes
    # that had already built llama-server with CUDA.
    Mod-Ninja
    Mod-LLVM
    Mod-CUDA
    Mod-GhAuth -WantsGrid:$WantsGrid

    # Grid transport reachability: Windows Firewall silently drops inbound peer
    # dials to the airc daemon unless it's allowed -- an asymmetric route failure
    # that breaks cross-grid delivery + peer-dialed inference. Grid-only; one gsudo
    # UAC (shared). A fresh grid box must not need a manual firewall click.
    Mod-AircFirewall -WantsGrid:$WantsGrid

    # Cold storage: auto-detect a large drive and route models + build cache there
    # (migrating what's on the system drive) BEFORE the build, so cargo builds into
    # the relocated cache. No-op on single-drive machines. Reconfigurable later.
    Mod-ColdStorage

    # Build + run as the invoking user (never elevated -- keeps the cargo cache
    # user-owned so a later non-elevated `npm start` can rebuild).
    Mod-BuildCore -RepoRoot $RepoRoot

    # Build llama-server.exe (the serving daemon's GPU-backend child) from the same
    # vendored llama.cpp. Windows twin of install-llama-server.sh. Without this the
    # serving daemon has no binary to spawn -> no local inference -> no persona can
    # speak. Needs CUDA + MSVC env (already provisioned above).
    Mod-LlamaServer -RepoRoot $RepoRoot

    Mod-Run
}
finally {
    # Always drop the cached elevation so an admin session never outlives install.
    Clear-Elevation
}

Write-Host ''
Write-Ok 'Continuum native install complete.'
Write-Host '  Start:  .\start.ps1     (ensures grid inbound every start, then launches)'
Write-Host '  Test:   cu ping'
Write-Host ''
