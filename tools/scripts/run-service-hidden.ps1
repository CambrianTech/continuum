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
    [Parameter(Mandatory = $true)][string]$LogDirectory,
    [Parameter(ParameterSetName = 'Native')][string]$EyeRoot
)
$ErrorActionPreference = 'Stop'
try {
    if ($PSCmdlet.ParameterSetName -eq 'Native') {
        # Prebuilt handoff bypasses start-server.sh. Use this release's media
        # artifacts so normal boot and Scheduler recovery start the same rail.
        $mediaScript = Join-Path $PSScriptRoot 'start-livekit-windows.ps1'
        $mediaBridge = Join-Path $PSScriptRoot 'livekit-bridge.exe'
        if ((Test-Path -LiteralPath $mediaScript) -and (Test-Path -LiteralPath $mediaBridge)) {
            try { & $mediaScript -BridgeBinary $mediaBridge }
            catch { Write-Warning "Live media startup failed: $_" }
        } else {
            Write-Warning 'Installed release has no media bundle; run continuum install to prepare it.'
        }
        $program = $ExecutablePath
        $childArguments = @('service-host', ('"' + $CorePath + '"'), ('"' + $SocketPath + '"'), ('"' + $EnginePath + '"'))
        if ($EyeRoot) { $childArguments += ('"' + $EyeRoot + '"') }
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
