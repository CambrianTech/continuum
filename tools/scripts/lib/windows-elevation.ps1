# Shared Windows installer elevation implementation. No application install is required.
# Imported by Continuum; standalone artifact boundary for AIRC integration.
param([System.Collections.IDictionary]$GsudoSource)
$script:ElevationGsudoSource = $GsudoSource
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

# One cache is bound to the outer installer's process and inherited by children.
# No global cache, persistent gsudo setting, or application installation is needed.
$script:ElevationWarmed = $false
$script:InstallElevationSession = $null
$script:GsudoExecutable = $null

function Initialize-ElevationSession {
    if ($script:InstallElevationSession) { return }
    $inherited = $env:CAMBRIAN_INSTALL_ELEVATION
    if ($inherited) {
        try {
            $context = $inherited | ConvertFrom-Json -ErrorAction Stop
            $ownerId = [int]$context.ownerPid
            if ($context.version -ne 1 -or $ownerId -le 0) { throw 'Invalid owner context.' }
            $owner = Get-Process -Id $ownerId -ErrorAction Stop
            if ($owner.StartTime.ToUniversalTime().Ticks.ToString() -ne $context.ownerStarted) {
                throw 'The installer owner process has changed.'
            }
            # Do not let stale/unrelated environment data widen the allowed tree.
            $ancestorId = $PID
            $seen = @{}
            while ($ancestorId -ne $ownerId) {
                if ($ancestorId -le 0 -or $seen.ContainsKey($ancestorId)) { throw 'Owner is not an ancestor.' }
                $seen[$ancestorId] = $true
                $ancestor = Get-CimInstance Win32_Process -Filter "ProcessId = $ancestorId" -ErrorAction Stop
                if (-not $ancestor) { throw 'Cannot resolve installer ancestry.' }
                $ancestorId = [int]$ancestor.ParentProcessId
            }
        } catch { throw "Cannot borrow installer elevation: $($_.Exception.Message)" }
        $script:InstallElevationSession = [pscustomobject]@{ OwnerPid = $ownerId; Borrowed = $true; ExistingCache = [bool]$context.existingCache }
    } else {
        $script:GsudoExecutable = Find-GsudoExecutable
        $existingCache = $false
        if ($script:GsudoExecutable -and -not (Test-IsAdmin)) { $existingCache = Test-ElevationCacheAvailable }
        $context = [ordered]@{
            version = 1
            existingCache = $existingCache
            ownerPid = $PID
            ownerStarted = (Get-Process -Id $PID).StartTime.ToUniversalTime().Ticks.ToString()
        }
        $env:CAMBRIAN_INSTALL_ELEVATION = $context | ConvertTo-Json -Compress
        $script:InstallElevationSession = [pscustomobject]@{ OwnerPid = $PID; Borrowed = $false; ExistingCache = $existingCache }
    }
}

function Find-GsudoExecutable {
    # An alias, PowerShell function or Git Bash wrapper can break cache ancestry.
    $command = Get-Command gsudo.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command -and [IO.Path]::IsPathRooted($command.Source) -and (Test-Path -LiteralPath $command.Source -PathType Leaf)) { return $command.Source }
}

# Probe only; never starts or extends someone else's credential cache.
function Test-ElevationCacheAvailable {
    $savedErrorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $answer = @(& $script:GsudoExecutable status CacheAvailable 2>&1)
        $code = $global:LASTEXITCODE
    } finally { $ErrorActionPreference = $savedErrorPreference }
    $value = ($answer -join [Environment]::NewLine).Trim()
    if ($code -eq 0 -and $value -eq 'true') { return $true }
    if ($code -eq 1 -and $value -eq 'false') { return $false }
    throw "Cannot inspect the existing elevation cache (exit $code): $value"
}

function Ensure-Gsudo {
    $script:GsudoExecutable = Find-GsudoExecutable
    if ($script:GsudoExecutable) { return }
    $source = $script:ElevationGsudoSource
    if (-not $source -or $source.type -ne 'winget' -or -not $source.id -or $source.scope -ne 'user') {
        throw 'The shared installer manifest must supply a per-user gsudo package source.'
    }
    Write-Host 'Installing gsudo (per-user) -- the shared elevation helper ...'
    & winget install --id $source.id --source winget --exact --silent `
        --accept-package-agreements --accept-source-agreements --scope $source.scope
    $code = $global:LASTEXITCODE
    if ($code -ne 0 -and $code -ne 3010) { throw "gsudo acquisition failed (winget exit $code)." }
    Update-SessionPath
    $script:GsudoExecutable = Find-GsudoExecutable
    if (-not $script:GsudoExecutable) { throw 'gsudo acquisition completed but the native executable is unavailable after PATH refresh.' }
}

# First actual admin operation acquires the cache, even if it is in a child.
# -p restricts authorization to the owner process and descendants. -d -1 avoids
# the default five-minute idle expiry during builds; owner finally/process exit
# ends this session. Never use pid 0, persistent CacheMode Auto, or global -k.
function Ensure-Elevated {
    param([string]$Reason = 'the current installer operation')
    Initialize-ElevationSession
    if ($script:InstallElevationSession.ExistingCache) {
        if (-not $script:GsudoExecutable) { Ensure-Gsudo }
        if (-not (Test-ElevationCacheAvailable)) { throw 'The pre-existing elevation cache expired; refusing to acquire a replacement without a new installer run.' }
        $script:ElevationWarmed = $true
        return
    }
    if ($script:ElevationWarmed) { return }
    if (Test-IsAdmin) { $script:ElevationWarmed = $true; return }
    Ensure-Gsudo
    Write-Host "Admin access needed for $Reason -- acquiring or reusing the installer elevation session."
    Write-Host 'gsudo is a third-party elevation helper. Windows may show its publisher in the consent prompt.'
    Write-Host 'Approval covers installer admin steps; authentication, builds and the core stay unelevated.'
    $savedErrorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $diagnostic = @(& $script:GsudoExecutable cache on -p $script:InstallElevationSession.OwnerPid -d -1 2>&1)
        $code = $global:LASTEXITCODE
    } finally { $ErrorActionPreference = $savedErrorPreference }
    if ($code -ne 0) {
        $detail = ($diagnostic | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        if ([string]::IsNullOrWhiteSpace($detail)) { $detail = 'gsudo returned no diagnostic output.' }
        throw "Elevation failed while $Reason (gsudo cache on exit $code).$([Environment]::NewLine)$detail"
    }
    $script:ElevationWarmed = $true
}

function Clear-Elevation {
    if (-not $script:InstallElevationSession) { return }
    if (-not $script:InstallElevationSession.Borrowed -and -not $script:InstallElevationSession.ExistingCache -and -not (Test-IsAdmin)) {
        # A child may have acquired the cache before the parent needed elevation.
        # Locate an installed helper without provisioning anything during cleanup.
        if (-not $script:GsudoExecutable) {
            $script:GsudoExecutable = Find-GsudoExecutable
            if (-not $script:GsudoExecutable) {
                Update-SessionPath
                $script:GsudoExecutable = Find-GsudoExecutable
            }
        }
        if ($script:GsudoExecutable) {
            $savedErrorPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $PSNativeCommandUseErrorActionPreference = $false
                $diagnostic = @(& $script:GsudoExecutable cache off -p $script:InstallElevationSession.OwnerPid 2>&1)
                $code = $global:LASTEXITCODE
            } finally { $ErrorActionPreference = $savedErrorPreference }
            if ($code -ne 0) { throw "Elevation cache cleanup failed (exit $code): $($diagnostic -join [Environment]::NewLine)" }
        } elseif ($script:ElevationWarmed) {
            throw 'Cannot close the installer elevation session: the native gsudo executable is missing.'
        }
    }
    if (-not $script:InstallElevationSession.Borrowed) { Remove-Item Env:CAMBRIAN_INSTALL_ELEVATION -ErrorAction SilentlyContinue }
    $script:InstallElevationSession = $null
    $script:ElevationWarmed = $false
}

function Invoke-Elevated {
    param([Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string[]]$CommandLine,
        [string]$Reason = 'running the elevated installer command')
    if (Test-IsAdmin) {
        $arguments = @($CommandLine | Select-Object -Skip 1)
        & $CommandLine[0] @arguments
        return
    }
    Ensure-Elevated -Reason $Reason
    & $script:GsudoExecutable @CommandLine
}
