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
            try {
                # Installed slots carry the same payload resolver as the installer.
                # A source invocation uses its original lib location; neither path
                # derives placement from the script's directory or a fixed drive.
                $payloadPaths = Join-Path $PSScriptRoot 'payload-paths.ps1'
                if (-not (Test-Path -LiteralPath $payloadPaths -PathType Leaf)) {
                    $payloadPaths = Join-Path $PSScriptRoot 'lib/payload-paths.ps1'
                }
                . $payloadPaths
                $payloadRoot = Get-ManagedPayloadRoot -HomeRoot (Join-Path $env:USERPROFILE '.continuum')
                & $mediaScript -BridgeBinary $mediaBridge -LivekitServerBinary (Join-Path $payloadRoot 'bin/livekit-server.exe')
            }
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
    # Start-Process truncates its redirect targets, so a relaunch would erase the
    # stderr of the run that just died: Rust's allocation-failure and runtime
    # aborts print their one line there and nowhere else (three core aborts,
    # 10-02/10-08/10-13, lost theirs this way). launchd appends; here the previous
    # run moves to .prev first. An empty log never replaces a kept one, so a fast
    # failing relaunch cannot erase the crash it followed. Two files, bounded.
    foreach ($name in 'service.out.log', 'service.err.log') {
        $current = Join-Path $LogDirectory $name
        if ((Test-Path -LiteralPath $current) -and (Get-Item -LiteralPath $current).Length -gt 0) {
            Move-Item -LiteralPath $current -Destination ($current -replace '\.log$', '.prev.log') -Force
        }
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
