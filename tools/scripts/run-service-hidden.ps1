# Keep Task Scheduler attached to the foreground service, without allocating a
# console. Propagate its exit code so RestartOnFailure still works.
[CmdletBinding(DefaultParameterSetName = 'Bash')]
param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Bash')][string]$BashPath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Bash')][string]$WrapperPath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Native')][string]$ExecutablePath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Native')][string]$CorePath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Native')][string]$SocketPath,
    [Parameter(Mandatory = $true, ParameterSetName = 'Native')][string]$EnginePath,
    [Parameter(Mandatory = $true)][string]$LogDirectory
)
$ErrorActionPreference = 'Stop'
try {
    if ($PSCmdlet.ParameterSetName -eq 'Native') {
        $program = $ExecutablePath
        $childArguments = @('service-host', ('"' + $CorePath + '"'), ('"' + $SocketPath + '"'), ('"' + $EnginePath + '"'))
    } else {
        $program = $BashPath
        $childArguments = @('--', ('"' + $WrapperPath + '"'))
    }
    $process = Start-Process -FilePath $program `
        -ArgumentList $childArguments `
        -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $LogDirectory 'service.out.log') `
        -RedirectStandardError (Join-Path $LogDirectory 'service.err.log')
    # Start-Process -Wait waits the entire descendant tree on Windows. Warm
    # inference lanes deliberately survive a core handoff. Wait for this host
    # alone, pinning its handle so PowerShell 5.1 retains the true exit code.
    $null = $process.Handle
    $process.WaitForExit()
    exit $process.ExitCode
} catch {
    Write-Error $_
    exit 1
}
