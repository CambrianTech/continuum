# install-common.ps1 -- shared primitives for the Continuum Windows installer.
#
# The PowerShell twin of tools/scripts/lib/install-common.sh. Same contract,
# platform-native tools. Sourced (dot-sourced) by install.ps1, never executed.
#
# Defines:
#   - Log + module primitives: Write-Step/Ok/Warn2/Fail, Module-Skip/Start/Done/Fail
#   - Test-IsAdmin, Update-SessionPath, Test-WingetAvailable, Test-VCTools
#   - The ONE elevation prompt source: Ensure-Elevated (gsudo credential cache)
#   - Install-IfMissing -- idempotent, auto-updating, scope-aware winget install
#
# Contract (mirrors install-common.sh / docs/infrastructure/INSTALL-ARCHITECTURE.md):
#   - A fresh install that needs admin N times prompts for UAC EXACTLY ONCE.
#     Every subsequent elevated command reuses the gsudo cache. Re-runs that do
#     no work prompt zero times. This is the Windows twin of ensure_sudo_warmed.
#   - Per-user tools (rustup, the cargo build) run UN-elevated so ~/.cargo and
#     the build cache stay owned by the user -- building as admin would break a
#     later non-elevated `npm start`. Only machine-scope installs go through
#     gsudo. This is why we use gsudo's credential cache instead of relaunching
#     the whole script elevated.
#   - Every install step is a self-guarded module: skip when satisfied, upgrade
#     when below floor, no-op on re-run.

# Windows PowerShell 5.1 compatible (the default shell). Avoid pwsh-7-only
# syntax (ternary / ?? / Start-Process -Environment).

#  Log + module primitives 
function Write-Step($msg)  { Write-Host "  -> $msg" }
function Write-Ok($msg)    { Write-Host "  + $msg"  -ForegroundColor Green }
function Write-Warn2($msg) { Write-Host "  ! $msg"  -ForegroundColor Yellow }
function Write-Fail($msg)  { Write-Host "  x $msg"  -ForegroundColor Red }

function Module-Skip  { param($Name, $Why)  Write-Host ("  + [{0}] {1} (skipped)" -f $Name, $Why)  -ForegroundColor Green }
function Module-Start { param($Name, $What) Write-Host ("  > [{0}] {1}"           -f $Name, $What) -ForegroundColor Cyan  }
function Module-Done  { param($Name)        Write-Host ("  + [{0}] done"          -f $Name)        -ForegroundColor Green }
function Module-Fail  { param($Name, $Fix)  Write-Host ("  x [{0}] {1}"           -f $Name, $Fix)  -ForegroundColor Red; exit 1 }

#  Environment helpers 

# winget mutates the Machine/User PATH in the registry, but the current session
# inherits the old PATH. Pull both back so subsequent probes see freshly-
# installed binaries without opening a new shell.
function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('PATH', 'Machine')
    $user    = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $env:PATH = "$machine;$user"
}

function Test-IsAdmin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal($id)).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-WingetAvailable {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Fail 'winget not found. It ships with App Installer (Microsoft Store).'
        Write-Host '    Install/update App Installer, then re-run.'
        Write-Host '    Direct: https://www.microsoft.com/store/productId/9NBLGGH4NNS1'
        exit 1
    }
}

# Probe: is the MSVC C++ toolset (cl.exe / link.exe) installed? vswhere ships
# with any VS/Build Tools installer and reports whether VC.Tools is present,
# without needing a vcvars shell.
function Test-VCTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { return $false }
    $path = & $vswhere -products * -latest `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
        -property installationPath 2>$null
    return [bool]$path
}

# Import the MSVC build environment (cl.exe + INCLUDE/LIB) into THIS session so
# nvcc can find its host compiler. Pins VS 2022 (v17): CUDA 12.9's nvcc supports
# MSVC 14.4x (VS2022) but rejects the newer 14.5x (VS18 preview). No-op if VC
# tools are absent (the caller already ensured them).
function Enter-MsvcEnv {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path $vswhere)) { Write-Warn2 'vswhere not found; cannot load MSVC env for nvcc.'; return }
    $vsPath = & $vswhere -latest -products * -version '[17.0,18.0)' `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if (-not $vsPath) {
        $vsPath = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    }
    if (-not $vsPath) { Write-Warn2 'no VC.Tools install found for MSVC env.'; return }
    $vcvars = Join-Path $vsPath 'VC\Auxiliary\Build\vcvars64.bat'
    if (-not (Test-Path $vcvars)) { Write-Warn2 "vcvars64.bat not found at $vcvars"; return }
    # Run vcvars in a child cmd and import the resulting environment variables.
    & cmd /c "`"$vcvars`" >nul 2>&1 && set" | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') {
            Set-Item -Path "env:$($Matches[1])" -Value $Matches[2] -ErrorAction SilentlyContinue
        }
    }
    Write-Ok "MSVC env loaded (VS2022, nvcc-compatible): $vsPath"
}

#  The ONE elevation prompt source 
#
# gsudo (Microsoft's own docs point to it as the sudo-with-cache for Windows)
# gives us a single UAC prompt for many elevated commands. `gsudo cache on`
# opens a cached-credentials session; every later `gsudo <cmd>` reuses it with
# no new prompt, until Clear-Elevation. This is the Windows equivalent of the
# bash keepalive in ensure_sudo_warmed.
#
# gsudo itself installs PER-USER (no admin), so bootstrapping it costs no prompt.

$script:ElevationWarmed = $false

function Ensure-Gsudo {
    if (Get-Command gsudo -ErrorAction SilentlyContinue) { return }
    Write-Step 'Installing gsudo (per-user, no admin) -- the one-prompt elevation helper ...'
    & winget install --id gerardog.gsudo --exact --silent `
        --accept-package-agreements --accept-source-agreements --scope user
    Update-SessionPath
    if (-not (Get-Command gsudo -ErrorAction SilentlyContinue)) {
        Write-Fail 'gsudo is not on PATH after install. Open a NEW shell and re-run (PATH refresh), or: winget install gerardog.gsudo'
        exit 1
    }
}

# Warm the single UAC. First call prompts once (unless already admin or already
# warmed); later calls are no-ops. Machine-scope installs run via `gsudo ...`
# after this. Idempotent + lazy: only the first module that actually needs admin
# triggers it.
function Ensure-Elevated {
    if ($script:ElevationWarmed) { return }
    if (Test-IsAdmin) { $script:ElevationWarmed = $true; return }  # already elevated -- gsudo not needed
    Ensure-Gsudo
    Write-Step 'Admin access needed -- approve the UAC prompt once now; no further prompts this run.'
    & gsudo cache on 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Fail 'Elevation declined. The toolchain (VS Build Tools, CUDA, LLVM) needs admin once. Re-run and approve the prompt.'
        exit 1
    }
    $script:ElevationWarmed = $true
}

# Tear down the cached elevation at the end of the run. Call from install.ps1's
# finally so a cached admin session never outlives the installer.
function Clear-Elevation {
    if ($script:ElevationWarmed -and -not (Test-IsAdmin)) {
        & gsudo cache off 2>&1 | Out-Null
    }
}

# Run one command elevated, reusing the warmed cache (no extra prompt). Already
# admin -> run directly.
function Invoke-Elevated {
    param([Parameter(Mandatory = $true)][string[]]$CommandLine)
    if (Test-IsAdmin) { & $CommandLine[0] @($CommandLine[1..($CommandLine.Length - 1)]); return }
    Ensure-Elevated
    & gsudo @CommandLine
}

#  Install-IfMissing -- idempotent, auto-updating, scope-aware 
#
# guard -> (skip if satisfied) -> announce -> install (elevated only if machine
# scope) -> verify. -UserScope routes per-user tools (rustup, gh, gsudo) away
# from elevation so they land in the invoking user's profile.
#
# NOTE on flags (from winget-pkgs#123624): `--disable-interactivity` breaks the
# VS Build Tools package, so we DON'T pass it. Silence for packages that need it
# comes from a package-specific -Override (e.g. VS's "--quiet"). Exit code 3010
# ("reboot required") is a SUCCESS, not an error.
function Install-IfMissing {
    param(
        [string]$Name,
        [string]$WingetId,
        [scriptblock]$TestCmd,
        [string]$Override,     # winget --override payload (installer-specific args)
        [switch]$UserScope     # per-user install -- no elevation
    )
    if (& $TestCmd) { Module-Skip $Name 'already present'; return }
    Module-Start $Name "installing (winget: $WingetId)"

    $wingetArgs = @(
        'install', '--id', $WingetId, '--exact', '--silent',
        '--accept-package-agreements', '--accept-source-agreements'
    )
    if ($UserScope) { $wingetArgs += @('--scope', 'user') } else { $wingetArgs += @('--scope', 'machine') }
    if ($Override)  { $wingetArgs += @('--override', $Override) }

    if ($UserScope -or (Test-IsAdmin)) {
        & winget @wingetArgs
    } else {
        Ensure-Elevated
        & gsudo winget @wingetArgs
    }
    $code = $LASTEXITCODE
    Update-SessionPath

    # winget success codes: 0 = ok; 3010 = installed, reboot required (success).
    if ($code -eq 0 -or $code -eq 3010) {
        if ($code -eq 3010) { Write-Warn2 "$Name installed -- a reboot is needed to finalize (safe to continue)." }
        if (& $TestCmd) { Module-Done $Name }
        else { Write-Warn2 "$Name installed but its probe still fails -- likely a PATH refresh needed in a NEW shell." }
    } else {
        Write-Warn2 "$Name winget exited $code -- if the build later fails on this dep, install it manually and re-run."
    }
}
