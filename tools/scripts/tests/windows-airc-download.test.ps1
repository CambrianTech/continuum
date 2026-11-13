$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../lib/windows-elevation.ps1')
. (Join-Path $PSScriptRoot '../lib/win-modules.ps1')

# A real loopback HTTP response exercises body acquisition without external
# network, administrator rights, firewall changes, or installing any software.
Add-Type -TypeDefinition @'
using System;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
public sealed class InstallerHttpFixture : IDisposable {
    readonly TcpListener listener;
    readonly Task worker;
    public string Url { get; private set; }
    public InstallerHttpFixture(int status, string body, int delay, int length)
        : this(status, Encoding.UTF8.GetBytes(body), delay, length) { }
    public InstallerHttpFixture(int status, byte[] bytes, int delay, int length) {
        listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        Url = "http://127.0.0.1:" + ((IPEndPoint)listener.LocalEndpoint).Port + "/install.ps1";
        worker = Task.Run(() => {
            try {
                using (var client = listener.AcceptTcpClient())
                using (var stream = client.GetStream()) {
                    stream.ReadTimeout = 5000;
                    int tail = 0;
                    while (tail != 0x0d0a0d0a) {
                        int b = stream.ReadByte(); if (b < 0) return;
                        tail = (tail << 8) | b;
                    }
                    byte[] header = Encoding.ASCII.GetBytes("HTTP/1.1 " + status + " Fixture\r\nContent-Length: " + (length < 0 ? bytes.Length : length) + "\r\nConnection: close\r\n\r\n");
                    stream.Write(header, 0, header.Length); stream.Flush();
                    if (delay > 0) Thread.Sleep(delay);
                    stream.Write(bytes, 0, bytes.Length);
                }
            } catch (System.IO.IOException) { }
              catch (SocketException) { }
        });
    }
    public void Dispose() { listener.Stop(); worker.Wait(6000); }
}
'@
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('airc-download-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $path = Join-Path $scratch 'entry.ps1'
    $body = 'param([string]$Proof); Write-Output $Proof; exit 23'
    $server = New-Object InstallerHttpFixture(200, $body, 0, -1)
    try {
        Save-InstallerSmallFile -Uri $server.Url -OutFile $path
        if ([IO.File]::ReadAllText($path) -cne $body) { throw 'Downloaded entry bytes changed.' }
    } finally { $server.Dispose() }
    # Binary archives use the same bounded downloader as entry scripts. This
    # catches accidental text decoding that corrupts Ninja ZIP bytes.
    $binary = [byte[]](0..255)
    $server = New-Object InstallerHttpFixture(200, $binary, 0, -1)
    try {
        Save-InstallerSmallFile -Uri $server.Url -OutFile $path
        if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($path)) -cne [Convert]::ToBase64String($binary)) {
            throw 'Downloaded binary archive bytes changed.'
        }
    } finally { $server.Dispose() }
    # Exercise the real small-file download and Ninja extraction together. The
    # prior Expand-Archive call spun without producing a file under PS5.
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zipPath = Join-Path $scratch 'ninja-fixture.zip'
    $zip = [IO.Compression.ZipFile]::Open($zipPath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $entry = $zip.CreateEntry('ninja.exe')
        $entryStream = $entry.Open()
        try { $entryStream.Write($binary, 0, $binary.Length) }
        finally { $entryStream.Dispose() }
    } finally { $zip.Dispose() }
    $downloadedZip = Join-Path $scratch 'ninja-win.zip'
    $server = New-Object InstallerHttpFixture(200, [IO.File]::ReadAllBytes($zipPath), 0, -1)
    try { Save-InstallerSmallFile -Uri $server.Url -OutFile $downloadedZip }
    finally { $server.Dispose() }
    $ninjaDir = Join-Path $scratch 'ninja'
    New-Item -ItemType Directory -Path $ninjaDir | Out-Null
    Expand-InstallerNinjaArchive -Archive $downloadedZip -Destination $ninjaDir
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $ninjaDir 'ninja.exe'))) -cne [Convert]::ToBase64String($binary)) {
        throw 'Extracted Ninja bytes changed.'
    }
    $badZipPath = Join-Path $scratch 'ninja-bad.zip'
    $zip = [IO.Compression.ZipFile]::Open($badZipPath, [IO.Compression.ZipArchiveMode]::Create)
    try { $null = $zip.CreateEntry('../outside.exe') }
    finally { $zip.Dispose() }
    $rejected = $false
    try { Expand-InstallerNinjaArchive -Archive $badZipPath -Destination $ninjaDir }
    catch { $rejected = $_ -match 'only ninja.exe' }
    if (-not $rejected -or (Test-Path -LiteralPath (Join-Path $scratch 'outside.exe'))) {
        throw 'Unexpected Ninja archive entry was published.'
    }
    $oversizeZipPath = Join-Path $scratch 'ninja-oversize.zip'
    $zip = [IO.Compression.ZipFile]::Open($oversizeZipPath, [IO.Compression.ZipArchiveMode]::Create)
    try {
        $entry = $zip.CreateEntry('ninja.exe')
        $entryStream = $entry.Open()
        try {
            $oversize = New-Object byte[] 1048577
            $entryStream.Write($oversize, 0, $oversize.Length)
        } finally { $entryStream.Dispose() }
    } finally { $zip.Dispose() }
    $rejected = $false
    try { Expand-InstallerNinjaArchive -Archive $oversizeZipPath -Destination $ninjaDir }
    catch { $rejected = $_ -match 'unexpected size' }
    if (-not $rejected -or @(Get-ChildItem -LiteralPath $ninjaDir -Filter 'ninja.exe.*.tmp').Count -ne 0) {
        throw 'Oversize Ninja archive left a published or staged executable.'
    }
    $rejected = $false
    try { Expand-InstallerNinjaArchive -Archive $downloadedZip -Destination $ninjaDir }
    catch { $rejected = $true }
    if (-not $rejected -or [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $ninjaDir 'ninja.exe'))) -cne [Convert]::ToBase64String($binary)) {
        throw 'Repeated Ninja extraction replaced an existing executable.'
    }
    foreach ($case in @(@(404,0,-1), @(200,0,1048577), @(200,3000,-1))) {
        Remove-Item -LiteralPath $path -Force
        $server = New-Object InstallerHttpFixture($case[0], 'bad', $case[1], $case[2])
        try {
            $failed = $false
            $elapsed = [Diagnostics.Stopwatch]::StartNew()
            try { Save-InstallerSmallFile -Uri $server.Url -OutFile $path -TimeoutSeconds 1 }
            catch { $failed = $true }
            $elapsed.Stop()
            if (-not $failed -or (Test-Path -LiteralPath $path)) { throw 'Failed/incomplete response was published.' }
            if ($case[1] -gt 0 -and $elapsed.Elapsed.TotalSeconds -ge 2.5) { throw 'Body timeout did not bound the complete response.' }
        } finally { $server.Dispose() }
        # Keep each next case's cleanup deterministic without accepting partial data.
        [IO.File]::WriteAllText($path, 'fixture')
    }
    $server = New-Object InstallerHttpFixture(200, $body, 0, -1)
    try {
        function Get-ManifestModule { param($Id) @{source=@{url=$server.Url}} }
        $failed = $false
        try { Invoke-AircSetup -SetupArguments @('-Proof','download-handoff-proof') }
        catch { $failed = $_ -match 'AIRC setup failed \(exit 23\)' }
        if (-not $failed) { throw 'Actual downloaded child exit was lost.' }
    } finally { $server.Dispose() }
    Write-Host 'PASS: exact entry bytes, Ninja archive, HTTP/size/timeout refusal, downloaded child exit propagation'
} finally { Remove-Item -LiteralPath $scratch -Recurse -Force }
exit 0
