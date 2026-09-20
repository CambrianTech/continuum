# start.ps1 -- headless-Rust start (Windows). Twin of tools/scripts/start-server.sh.
#
# Repeatable start: ENSURES the grid invariants that need admin (the airc inbound
# firewall) on EVERY start -- not just at install -- then launches the core. The
# checks are free (no admin), so a healthy box starts with ZERO prompts; a box
# that somehow lost the rule gets ONE batched UAC (the same gsudo credential cache
# the installer uses -- never a second prompt). Re-run is safe and idempotent.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\start.ps1
#   .\start.ps1 -NoGrid          # local-only, skip the grid-inbound ensure
#
# The heavy setup (toolchain, build, model provisioning) lives in install.ps1;
# this is the everyday start. Run install.ps1 first on a fresh box.

[CmdletBinding()]
param([switch]$NoGrid)

$ErrorActionPreference = 'Stop'
$RepoRoot = $PSScriptRoot
. (Join-Path $RepoRoot 'tools\scripts\lib\install-common.ps1')
. (Join-Path $RepoRoot 'tools\scripts\lib\win-modules.ps1')

# Config: source ~/.continuum/config.env so we launch against the cold-storage
# routing (CONTINUUM_STORAGE_PATH / HF_HOME) the installer wrote.
$configEnv = Join-Path $env:USERPROFILE '.continuum\config.env'
if (Test-Path $configEnv) {
    Get-Content $configEnv | ForEach-Object {
        if ($_ -match '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+?)\s*$' -and $_ -notmatch '^\s*#') {
            Set-Item "Env:$($Matches[1])" $Matches[2]
        }
    }
}
# CARGO_TARGET_DIR follows cold-storage routing: the installer relocates the
# build cache to <CONTINUUM_STORAGE_PATH>\cargo-target on a big drive, so the
# binary lives there, not on the system drive.
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = if ($env:CONTINUUM_STORAGE_PATH) {
        Join-Path $env:CONTINUUM_STORAGE_PATH 'cargo-target'
    } else {
        Join-Path $env:USERPROFILE '.continuum\cache\cargo-target'
    }
}

# ── Ensure grid invariants (admin, batched, idempotent) ──────────────────────
# A grid box = airc (the transport) is installed. If so, GUARANTEE inbound
# reachability every start. Mod-AircFirewall checks first (no admin) and only
# elevates (one shared gsudo UAC) if the rule is actually missing. This is the
# "make sure it's set on any start/update, one prompt for all" contract.
$aircPresent = (Get-Command airc -ErrorAction SilentlyContinue) -or
               (Test-Path (Join-Path $env:USERPROFILE '.local\bin\airc.exe'))
try {
    Mod-AircFirewall -WantsGrid:((-not $NoGrid) -and $aircPresent)
    # Future admin-requiring start invariants call Ensure-Elevated too — they all
    # share the ONE gsudo cache warmed above, so it stays a single prompt.
}
finally {
    Clear-Elevation   # drop the cached elevation so admin never outlives start
}

# ── Launch the core ──────────────────────────────────────────────────────────
$bin = Join-Path $env:CARGO_TARGET_DIR 'release\continuum-core-server.exe'
if (-not (Test-Path $bin)) {
    Write-Fail "continuum-core-server not built at $bin -- run install.ps1 first."
    exit 1
}
$sock = if ($env:CONTINUUM_SOCKET) { $env:CONTINUUM_SOCKET } else { Join-Path $env:TEMP 'continuum-core.sock' }
Write-Ok "starting continuum-core-server"
Write-Host "    binary: $bin"
Write-Host "    socket: $sock"
& $bin $sock
