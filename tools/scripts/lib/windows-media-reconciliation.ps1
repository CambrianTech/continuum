# Legacy single-client bridge retirement belongs to installed media lifecycle.
# This is an explicit graceful close, NEVER a health probe or forced termination.
param([string]$MediaPlanPath, [string]$MediaPlanSha)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-prepared.ps1')
if (-not (Get-Command Invoke-InstallerProcess -CommandType Function -ErrorAction SilentlyContinue)) {
    . (Join-Path $PSScriptRoot 'windows-elevation.ps1')
}

function Initialize-CoreMediaHandle {
    if ('Continuum.Setup.HeldMediaProcess' -as [type]) { return }
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
namespace Continuum.Setup {
public sealed class HeldMediaProcess : IDisposable {
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool QueryFullProcessImageNameW(IntPtr process, uint flags, StringBuilder image, ref uint length);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetProcessTimes(IntPtr process, out long created, out long exited, out long kernel, out long user);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    IntPtr handle;
    public string Image { get; private set; }
    public long CreatedUtcMicroseconds { get; private set; }
    public HeldMediaProcess(uint pid) {
        // QUERY_LIMITED_INFORMATION | SYNCHRONIZE. No terminate capability exists.
        handle=OpenProcess(0x101000, false, pid);
        if (handle==IntPtr.Zero) throw new Win32Exception();
        try {
            var image=new StringBuilder(32768); uint length=(uint)image.Capacity;
            if (!QueryFullProcessImageNameW(handle,0,image,ref length)) throw new Win32Exception();
            long created,exited,kernel,user;
            if (!GetProcessTimes(handle,out created,out exited,out kernel,out user)) throw new Win32Exception();
            Image=image.ToString();
            // CIM timestamps have microsecond precision, unlike FILETIME's 100ns.
            CreatedUtcMicroseconds=DateTime.FromFileTimeUtc(created).Ticks/10;
        } catch { Dispose(); throw; }
    }
    public bool Wait(int milliseconds) {
        uint result=WaitForSingleObject(handle,(uint)milliseconds);
        if (result==0xFFFFFFFF) throw new Win32Exception();
        return result==0;
    }
    public void Dispose() { if (handle!=IntPtr.Zero) { CloseHandle(handle); handle=IntPtr.Zero; } }
}}
'@
}

function Get-CoreMediaConnections {
    param([int]$ProcessId)
    # Enumerate without a filter so "no matching instances" is not confused
    # with a failed inspection. Failure must never become proof of idleness.
    $tcp = @(Get-NetTCPConnection -ErrorAction Stop | Where-Object { $_.OwningProcess -eq $ProcessId })
    $udp = @(Get-NetUDPEndpoint -ErrorAction Stop | Where-Object { $_.OwningProcess -eq $ProcessId })
    return @{ tcp = $tcp; udp = $udp }
}

function Close-CoreIdleLegacyMedia {
    param($Plan, [int]$ExitTimeoutMs = 5000)
    Initialize-CoreMediaHandle
    $held = [Continuum.Setup.HeldMediaProcess]::new([uint32]$Plan.processId)
    $imagePin = $null; $client = $null
    try {
        if ((ConvertTo-CoreImagePath $held.Image) -ine (ConvertTo-CoreImagePath $Plan.image) -or
            $held.CreatedUtcMicroseconds -ne [long]$Plan.createdUtcMicroseconds) {
            throw 'Legacy bridge identity changed; no connection was made.'
        }
        $imagePin = [IO.File]::Open($Plan.image, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        if ((Get-FileHash -LiteralPath $Plan.image -Algorithm SHA256).Hash -ine $Plan.imageSha256) {
            throw 'Legacy bridge image changed; no connection was made.'
        }
        $connections = Get-CoreMediaConnections -ProcessId $Plan.processId
        $listeners = @($connections.tcp | Where-Object {
            $_.State -eq 'Listen' -and $_.LocalAddress -eq '127.0.0.1' -and $_.LocalPort -eq $Plan.port
        })
        if ($listeners.Count -ne 1 -or $connections.tcp.Count -ne 1 -or $connections.udp.Count -ne 0 -or $held.Wait(0)) {
            throw 'Legacy bridge is active or its loopback endpoint changed; no connection was made.'
        }
        # A racing core may win accept after inspection. Its FIRST stream is
        # never ours to close: our queued EOF then cannot stop the bridge. The
        # bounded wait refuses migration, with no TerminateProcess fallback.
        $client = [Net.Sockets.TcpClient]::new()
        $connect = $client.ConnectAsync('127.0.0.1', [int]$Plan.port)
        if (-not $connect.Wait(2000)) { throw 'Legacy bridge close connection timed out.' }
        $client.Dispose(); $client = $null
        if (-not $held.Wait($ExitTimeoutMs)) {
            throw 'Legacy bridge retained a client; graceful close did not retire it. No process was terminated; retry after the active session ends.'
        }
        Write-Output "Retired idle legacy bridge PID $($Plan.processId), created $($Plan.createdUtcMicroseconds), image $($Plan.image), SHA256 $($Plan.imageSha256) by zero-command EOF."
    } finally {
        if ($client) { $client.Dispose() }
        if ($imagePin) { $imagePin.Dispose() }
        $held.Dispose()
    }
}

function Invoke-CoreLegacyMediaPlan {
    param([string]$Path, [string]$Sha)
    $pin = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        if ($pin.Length -gt 16384 -or $Sha -notmatch '^[0-9a-fA-F]{64}$' -or
            (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Sha) {
            throw 'Legacy media plan does not match the consented digest.'
        }
        $plan = [IO.File]::ReadAllText($Path) | ConvertFrom-Json -ErrorAction Stop
    } finally { $pin.Dispose() }
    $fields = @('schema','installRoot','image','imageSha256','processId','createdUtcMicroseconds','port')
    if (@($plan.PSObject.Properties).Count -ne $fields.Count -or
        @($plan.PSObject.Properties.Name | Where-Object { $_ -notin $fields }).Count -or
        $plan.schema -ne 1 -or $plan.port -ne 9101 -or $plan.processId -le 0 -or
        $plan.createdUtcMicroseconds -le 0 -or $plan.imageSha256 -notmatch '^[0-9a-fA-F]{64}$') {
        throw 'Invalid legacy media reconciliation plan.'
    }
    $slot = Split-Path (Split-Path $plan.image -Parent) -Leaf
    if ($slot -notin @('service-a','service-b')) { throw 'Unknown legacy media service slot.' }
    $expected = Join-Path (Get-ManagedPayloadRoot -HomeRoot $plan.installRoot) "bin\$slot\livekit-bridge.exe"
    Assert-CorePreparedPath -Path $plan.image -Expected $expected -File
    Close-CoreIdleLegacyMedia -Plan $plan
}

function Invoke-CoreLegacyMediaReconciliation {
    param([string]$Image, [string]$InstallRoot, [switch]$AllowElevation)
    if (-not $AllowElevation) {
        throw 'An installed legacy media bridge holds the inactive service slot. PrepareOnly preserves it and cannot elevate. Run the normal public installer to reconcile an idle bridge; active sessions are never terminated.'
    }
    $listeners = @(Get-NetTCPConnection -ErrorAction Stop | Where-Object {
        $_.State -eq 'Listen' -and $_.LocalAddress -eq '127.0.0.1' -and $_.LocalPort -eq 9101
    })
    if ($listeners.Count -ne 1) { throw 'Cannot identify one legacy bridge listener; installed files were preserved.' }
    $processId = [int]$listeners[0].OwningProcess
    $process = Get-CimInstance Win32_Process -Filter "ProcessId=$processId" -ErrorAction Stop
    if (-not $process -or $process.Name -ine 'livekit-bridge.exe' -or -not $process.CreationDate) {
        throw 'The legacy media endpoint is not a bridge; installed files were preserved.'
    }
    $connections = Get-CoreMediaConnections -ProcessId $processId
    if ($connections.tcp.Count -ne 1 -or $connections.udp.Count -ne 0) {
        throw 'Legacy bridge has consumers; finish its active session before retrying installation.'
    }
    $plan = [ordered]@{ schema=1; installRoot=$InstallRoot; image=$Image;
        imageSha256=(Get-FileHash -LiteralPath $Image -Algorithm SHA256).Hash;
        processId=$processId; createdUtcMicroseconds=[long][Math]::Floor($process.CreationDate.ToUniversalTime().Ticks / [decimal]10); port=9101 }
    $temporary = Join-Path ([IO.Path]::GetTempPath()) ('continuum-media-' + [guid]::NewGuid().ToString('N') + '.json')
    try {
        [IO.File]::WriteAllText($temporary, ($plan | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        $sha = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash
        Invoke-Elevated -Reason 'reconciling an idle legacy media process before preparing the inactive release' -CommandLine @(
            "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe", '-NoProfile', '-NonInteractive',
            '-ExecutionPolicy', 'RemoteSigned', '-File', $PSCommandPath, '-MediaPlanPath', $temporary, '-MediaPlanSha', $sha) | ForEach-Object { Write-Host $_ }
        if ($global:LASTEXITCODE -ne 0) { throw 'Legacy media reconciliation did not complete; no forced termination was attempted. Installed files were preserved.' }
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}

if ($MediaPlanPath -or $MediaPlanSha) {
    Invoke-InstallerEntryPoint { Invoke-CoreLegacyMediaPlan -Path $MediaPlanPath -Sha $MediaPlanSha }
}
