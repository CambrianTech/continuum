# Shared Windows installer elevation implementation. No application install is required.
# Imported by Continuum; standalone artifact boundary for AIRC integration.
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
    Write-Host 'Installing gsudo (per-user, no admin) -- the one-prompt elevation helper ...'
    & winget install --id gerardog.gsudo --exact --silent `
        --accept-package-agreements --accept-source-agreements --scope user
    Update-SessionPath
    if (-not (Get-Command gsudo -ErrorAction SilentlyContinue)) {
        Write-Host 'gsudo is not on PATH after install. Open a NEW shell and re-run (PATH refresh), or: winget install gerardog.gsudo'
        exit 1
    }
}

# Warm the single UAC. First call prompts once (unless already admin or already
# warmed); later calls are no-ops. Machine-scope installs run via `gsudo ...`
# after this. Idempotent + lazy: only the first module that actually needs admin
# triggers it.
function Ensure-Elevated {
    param([string]$Reason = 'the current installer operation')
    if ($script:ElevationWarmed) { return }
    if (Test-IsAdmin) { $script:ElevationWarmed = $true; return }  # already elevated -- gsudo not needed
    Ensure-Gsudo
    Write-Host "Admin access needed for $Reason -- requesting the shared elevation cache."
    Write-Host 'gsudo is a third-party elevation helper. Windows may show its publisher, not Continuum, in the consent prompt.'
    Write-Host 'Approval lets the installer continue its admin steps; it does not mean installation is complete. The build and core stay unelevated.'
    # PS5 represents redirected native stderr as ErrorRecords. Capture it even
    # under the installer's Stop preference, then judge the native exit code.
    $savedErrorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $diagnostic = @(& gsudo cache on 2>&1)
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $savedErrorPreference }
    if ($code -ne 0) {
        $detail = ($diagnostic | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        if ([string]::IsNullOrWhiteSpace($detail)) { $detail = 'gsudo returned no diagnostic output.' }
        throw "Elevation failed while $Reason (gsudo cache on exit $code).$([Environment]::NewLine)$detail"
    }
    $script:ElevationWarmed = $true
}

# Tear down the cached elevation at the end of the run. Call from install.ps1's
# finally so a cached admin session never outlives the installer.
function Clear-Elevation {
    if ($script:ElevationWarmed -and -not (Test-IsAdmin)) {
        # PS5 turns informational native stderr into ErrorRecords under Stop.
        # Judge cleanup by its exit code, just like cache acquisition above.
        $savedErrorPreference = $ErrorActionPreference
        try {
            $ErrorActionPreference = 'Continue'
            $PSNativeCommandUseErrorActionPreference = $false
            $diagnostic = @(& gsudo cache off 2>&1)
            $code = $LASTEXITCODE
        } finally { $ErrorActionPreference = $savedErrorPreference }
        if ($code -ne 0) {
            throw "Elevation cache cleanup failed (exit $code): $($diagnostic -join [Environment]::NewLine)"
        }
        $script:ElevationWarmed = $false
    }
}

# Run one command elevated, reusing the warmed cache (no extra prompt). Already
# admin -> run directly.
function Invoke-Elevated {
    param([Parameter(Mandatory = $true)][string[]]$CommandLine,
        [string]$Reason = 'running the elevated installer command')
    if (Test-IsAdmin) { & $CommandLine[0] @($CommandLine[1..($CommandLine.Length - 1)]); return }
    Ensure-Elevated -Reason $Reason
    & gsudo @CommandLine
}

