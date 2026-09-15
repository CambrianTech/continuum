# Keep Task Scheduler attached to the foreground service, without allocating a
# console. Propagate its exit code so RestartOnFailure still works.
param(
    [Parameter(Mandatory = $true)][string]$BashPath,
    [Parameter(Mandatory = $true)][string]$WrapperPath,
    [Parameter(Mandatory = $true)][string]$LogDirectory
)
$ErrorActionPreference = 'Stop'
try {
    $process = Start-Process -FilePath $BashPath `
        -ArgumentList @('--', ('"' + $WrapperPath + '"')) `
        -WindowStyle Hidden -Wait -PassThru `
        -RedirectStandardOutput (Join-Path $LogDirectory 'service.out.log') `
        -RedirectStandardError (Join-Path $LogDirectory 'service.err.log')
    exit $process.ExitCode
} catch {
    Write-Error $_
    exit 1
}
