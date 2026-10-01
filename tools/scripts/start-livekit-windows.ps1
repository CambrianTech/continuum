param([Parameter(Mandatory=$true)][string]$BridgeBinary)
$ErrorActionPreference = 'Stop'
$logDirectory = Join-Path $env:USERPROFILE '.continuum/logs'
New-Item -ItemType Directory -Force -Path $logDirectory | Out-Null
function Start-LocalMediaProcess([string]$Binary, [string[]]$Arguments, [int]$Port, [string]$Name) {
    $resolved = (Resolve-Path -LiteralPath $Binary).Path
    $listener = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue
    if ($listener) {
        foreach ($owner in @($listener.OwningProcess | Select-Object -Unique)) {
            $process = Get-Process -Id $owner
            if ($process.Path -ne $resolved) { throw "Port $Port belongs to another executable; refusing duplicate $Name" }
        }
        Write-Output "$Name already listening on $Port"
        return
    }
    $existing = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $resolved })
    if ($existing.Count) { throw "$Name is running but not listening on $Port; inspect its logs before restarting" }
    $child = Start-Process -FilePath $resolved -ArgumentList $Arguments -WindowStyle Hidden -PassThru `
        -RedirectStandardOutput (Join-Path $logDirectory "$Name.out.log") `
        -RedirectStandardError (Join-Path $logDirectory "$Name.err.log")
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        $child.Refresh()
        if ($child.HasExited) { throw "$Name exited: $($child.ExitCode); see $logDirectory" }
        $ready = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
            Where-Object { $_.OwningProcess -eq $child.Id }
        if ($ready) { Write-Output "$Name ready: PID $($child.Id), port $Port"; return }
        Start-Sleep -Milliseconds 200
    }
    throw "$Name started as PID $($child.Id) but readiness timed out; no duplicate will be started"
}
$url = if ($env:LIVEKIT_URL) { [uri]$env:LIVEKIT_URL } else { [uri]'ws://localhost:7880' }
if ($url.Scheme -notin @('ws','wss')) { throw 'LIVEKIT_URL must use ws or wss' }
if ($url.IsLoopback) {
    if ($url.Port -ne 7880) { throw 'Local LiveKit startup expects port7880' }
    Start-LocalMediaProcess (Join-Path $env:USERPROFILE '.continuum/bin/livekit-server.exe') @('--dev','--bind','127.0.0.1') 7880 'livekit-server'
}
Start-LocalMediaProcess $BridgeBinary @('127.0.0.1:9101','--livekit-url',$url.AbsoluteUri) 9101 'livekit-bridge'
