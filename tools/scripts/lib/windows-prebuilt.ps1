# Fresh-install transport only. Compatibility and complete artifact preparation are
# owned by the published CLI's prepare-prebuilt verb, also used by deploy consumption.
. (Join-Path $PSScriptRoot '../generated/manifest.windows.ps1')

function Get-CoreBootstrapRoot {
    # Same env -> config.env -> home contract as paths::artifacts::root.
    $storage = $env:CONTINUUM_STORAGE_PATH
    if ([string]::IsNullOrEmpty($storage)) {
        $config = Join-Path $env:USERPROFILE '.continuum/config.env'
        if (Test-Path -LiteralPath $config) {
            foreach ($line in [IO.File]::ReadAllLines($config)) {
                if ($line.Trim() -cmatch '^CONTINUUM_STORAGE_PATH\s*=(.*)$') {
                    $storage = $Matches[1].Trim()
                    if ($storage.Length -ge 2 -and (($storage.StartsWith('"') -and $storage.EndsWith('"')) -or ($storage.StartsWith("'") -and $storage.EndsWith("'")))) { $storage = $storage.Substring(1, $storage.Length - 2) }
                }
            }
        }
    }
    if (-not [string]::IsNullOrEmpty($storage)) {
        if ($storage -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+(?:\\|$))') { throw 'Configured artifact storage must be absolute.' }
        if (-not (Test-Path -LiteralPath $storage -PathType Container)) { throw 'Configured artifact storage is unavailable; bootstrap will not fall back to home.' }
        return (Join-Path $storage 'artifacts/bootstrap')
    }
    return (Join-Path $env:USERPROFILE '.continuum/cache/artifacts/bootstrap')
}

function Get-CoreBootstrapVolume {
    param([string]$Directory)
    if (-not ('Continuum.Setup.BootstrapVolume' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
namespace Continuum.Setup {
public static class BootstrapVolume {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern SafeFileHandle CreateFileW(string path, uint access, uint share, IntPtr security, uint creation, uint flags, IntPtr template);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern uint GetFinalPathNameByHandleW(SafeFileHandle handle, StringBuilder path, uint size, uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool GetDiskFreeSpaceExW(string path, out ulong available, out ulong total, out ulong free);
    public static ulong[] Read(string path) {
        using (var handle=CreateFileW(path,0,7,IntPtr.Zero,3,0x02000000,IntPtr.Zero)) {
            if (handle.IsInvalid) throw new Win32Exception();
            var physical=new StringBuilder(32768);
            uint n=GetFinalPathNameByHandleW(handle,physical,(uint)physical.Capacity,0);
            if (n==0 || n>=physical.Capacity) throw new Win32Exception();
            ulong available,total,free;
            if (!GetDiskFreeSpaceExW(physical.ToString()+"\\",out available,out total,out free)) throw new Win32Exception();
            if (free>total) throw new InvalidOperationException("Invalid physical volume sample");
            return new ulong[]{total-free,total};
        }
    }
}}
'@
    }
    return ,([Continuum.Setup.BootstrapVolume]::Read($Directory))
}

function Assert-CoreBootstrapCapacity {
    param([string]$Directory, [uint64]$Bytes)
    $volume = Get-CoreBootstrapVolume $Directory
    if (-not (Test-CoreProjectedDiskCapacity $volume[0] $volume[1] $Bytes)) {
        throw "Bootstrap resource deferred: $Directory needs $Bytes additional bytes on its observed volume (used $($volume[0]), capacity $($volume[1]))."
    }
    return ,$volume
}

# BEGIN GENERATED PREBUILT INPUTS
$script:CorePrebuiltInputs = @('core/', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tools/scripts/lib/', 'tools/scripts/shared/cargo-features.sh', 'tools/scripts/start-livekit-windows.ps1', 'tools/scripts/generated/manifest.windows.ps1')
# END GENERATED PREBUILT INPUTS

function Write-CoreBootstrapStream {
    param($inputStream, [string]$OutFile, $Length, [long]$Maximum, $Token)
    $output = $null
    try {
        if ($Length -gt $Maximum) { throw 'Published transport exceeds its byte limit.' }
        $directory = Split-Path $OutFile -Parent
        $declared = $Length
        $projected = if ($null -eq $declared) { [uint64]0 } else { [uint64]$declared }
        $volume = Assert-CoreBootstrapCapacity $directory $projected
        $charged = [uint64]0
        $output = [IO.File]::Create($OutFile)
        $buffer = New-Object byte[] 65536
        $total = [long]0
        while (($count = $inputStream.ReadAsync($buffer, 0, $buffer.Length, $Token).GetAwaiter().GetResult()) -gt 0) {
            $total += $count
            if ($total -gt $Maximum) { throw 'Published transport exceeds its byte limit.' }
            if (-not (Test-CoreProjectedDiskCapacity $volume[0] $volume[1] ($charged + [uint64]$count))) {
                $volume = Assert-CoreBootstrapCapacity $directory ([uint64]$count)
                $charged = 0
            }
            $output.Write($buffer, 0, $count)
            $charged += [uint64]$count
        }
    } finally { if ($output) { $output.Dispose() } }
}

function Save-CorePrebuiltArchive {
    param([string]$Uri, [string]$OutFile, [long]$Maximum = 4294967296, [int]$TimeoutSeconds = 1800)
    Add-Type -AssemblyName System.Net.Http
    $client = New-Object Net.Http.HttpClient
    $cancel = New-Object Threading.CancellationTokenSource
    $cancel.CancelAfter([TimeSpan]::FromSeconds($TimeoutSeconds))
    $response = $null
    try {
        $response = $client.GetAsync($Uri, [Net.Http.HttpCompletionOption]::ResponseHeadersRead, $cancel.Token).GetAwaiter().GetResult()
        $response.EnsureSuccessStatusCode() | Out-Null
        $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        Write-CoreBootstrapStream $inputStream $OutFile $response.Content.Headers.ContentLength $Maximum $cancel.Token
    } finally {
        if ($response) { $response.Dispose() }
        $cancel.Dispose(); $client.Dispose()
    }
}

function Assert-CoreBootstrapManifest {
    param($Manifest, [string]$Tip, [string]$Platform)
    if ($Manifest.git_sha -cne $Tip -or $Manifest.platform -cne $Platform -or
        $Manifest.archive -cne "continuum-core-$Platform.tar.gz" -or
        $Manifest.sha256 -cnotmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid published bootstrap transport identity.' }
    if ('bootstrap_runtime_libs' -notin $Manifest.PSObject.Properties.Name) { throw 'Publication does not declare its bootstrap runtime closure.' }
    $names = @($Manifest.bootstrap_runtime_libs)
    $unique = New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    if ($names.Count -gt 128) { throw 'Invalid bootstrap runtime closure.' }
    foreach ($name in $names) {
        if (-not $unique.Add($name) -or $name -cnotmatch '^[A-Za-z0-9_.-]+\.[dD][lL][lL]$' -or $name -notin @($Manifest.runtime_libs)) { throw 'Unsafe or undeclared bootstrap runtime member.' }
    }
}

function Expand-CoreBootstrap {
    param([string]$Archive, $Manifest, [string]$Directory)
    $pin = [IO.File]::Open($Archive, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Assert-Sha256 -Path $Archive -Expected $Manifest.sha256 -Name 'published release'
        $prefix = "continuum-core-$($Manifest.platform)/"
        $names = @('continuum.exe') + @($Manifest.bootstrap_runtime_libs)
        $selected = @($names | ForEach-Object { $prefix + $_ })
        $members = @(& tar.exe -tzf $Archive)
        if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect bootstrap archive.' }
        foreach ($member in $selected) {
            if (@($members | Where-Object { $_ -ieq $member }).Count -ne 1 -or $member -cnotin $members) { throw 'Bootstrap archive contains a missing or duplicate member.' }
        }
        $entries = @(& tar.exe -tvzf $Archive @selected)
        if ($LASTEXITCODE -ne 0 -or $entries.Count -ne $selected.Count -or @($entries | Where-Object { -not $_.StartsWith('-') }).Count) { throw 'Bootstrap members must be regular archive files.' }
        $expanded = [decimal]0
        foreach ($entry in $entries) {
            $fields = $entry -split '\s+'
            # The same GNU/BSD regular-member layouts accepted by archive_expanded_bytes.
            if ($fields.Count -ge 6 -and $fields[1].Contains('/')) { $size = $fields[2] }
            elseif ($fields.Count -ge 9 -and $fields[1] -match '^\d+$') { $size = $fields[4] }
            else { throw 'Unknown bootstrap archive size layout.' }
            if ($size -notmatch '^\d+$') { throw 'Invalid bootstrap member size.' }
            $expanded += [uint64]$size
            if ($expanded -gt [decimal][uint64]::MaxValue) { throw 'Bootstrap expanded size overflow.' }
        }
        # Compressed archive is already on this volume; charge only the new selected closure.
        $null = Assert-CoreBootstrapCapacity (Split-Path $Directory -Parent) ([uint64]$expanded)
        if (Test-Path -LiteralPath $Directory) { throw 'Bootstrap extraction destination must be new.' }
        New-Item -ItemType Directory -Path $Directory -ErrorAction Stop | Out-Null
        # Exact regular members only; strip the one fixed publisher directory.
        Invoke-InstallerProcess 'tar.exe' (@('-xzf', $Archive, '--strip-components=1', '-C', $Directory) + $selected) -OwnProcessTree
        if ($LASTEXITCODE -ne 0) { throw 'Bootstrap extraction failed.' }
        foreach ($name in $names) {
            $file = Join-Path $Directory $name
            Assert-CorePreparedPath -Path $file -Expected $file -File
            if ((Get-Item -LiteralPath $file).Length -eq 0) { throw 'Bootstrap member is empty.' }
        }
        return (Join-Path $Directory 'continuum.exe')
    } finally { $pin.Dispose() }
}

function Get-CorePrebuiltRelease {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
        throw 'No compatible Windows prebuilt is published for this architecture.'
    }
    $tip = (& git -C $RepoRoot log -1 --format=%H HEAD -- @script:CorePrebuiltInputs | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $tip -cnotmatch '^[0-9a-f]{40}$') { throw 'Cannot resolve the checkout artifact build identity.' }
    $platform = 'windows-x86_64'
    $base = 'https://github.com/CambrianTech/continuum/releases/download/canary-' + $tip.Substring(0, 12)
    $bootstrapRoot = Get-CoreBootstrapRoot
    $download = Join-Path $bootstrapRoot ([guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $download -Force | Out-Null
    try {
    $manifestPath = Join-Path $download "continuum-core-$platform.json"
    Write-Step "Downloading published release $($tip.Substring(0, 12)); no developer toolchain is required."
    Save-CorePrebuiltArchive -Uri "$base/continuum-core-$platform.json" -OutFile $manifestPath -Maximum 65536 -TimeoutSeconds 60
    if ((Get-Item -LiteralPath $manifestPath).Length -gt 65536) { throw 'Published manifest exceeds 64 KiB.' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    # Only transport identity and checksum are interpreted before the first CLI runs.
    Assert-CoreBootstrapManifest -Manifest $manifest -Tip $tip -Platform $platform
    $archive = Join-Path $download $manifest.archive
    Save-CorePrebuiltArchive -Uri "$base/$($manifest.archive)" -OutFile $archive
    $cli = Expand-CoreBootstrap -Archive $archive -Manifest $manifest -Directory (Join-Path $download 'bootstrap')
    $prepared = @(Invoke-InstallerProcess $cli @('prepare-prebuilt', $RepoRoot, $download) -OwnProcessTree)
    if ($LASTEXITCODE -ne 0) { throw 'Published release preparation refused; no source build will be attempted.' }
    $result = ($prepared -join "`n") | ConvertFrom-Json
    if ($result.git_sha -cne $tip -or -not (Test-Path -LiteralPath $result.core -PathType Leaf)) { throw 'Invalid prepared artifact result.' }
    return (Split-Path $result.core -Parent)
    } finally {
        $cacheRoot = [IO.Path]::GetFullPath($bootstrapRoot).TrimEnd('\') + '\'
        $owned = [IO.Path]::GetFullPath($download)
        if ($owned.StartsWith($cacheRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $owned -Leaf) -cmatch '^[a-f0-9]{32}$') {
            Remove-Item -LiteralPath $owned -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Copy-CorePublishedEngine {
    param([string]$RepoRoot, [string]$ArtifactDirectory, [string]$InstallDirectory)
    # The core's deploy passes the extended \\?\ form, which Join-Path refuses (see New-CoreServiceRelease).
    $ArtifactDirectory = ConvertTo-CoreImagePath $ArtifactDirectory
    $source = Join-Path $ArtifactDirectory 'engine'
    $receipt = Get-CoreEngineReceipt -Directory $source
    $revision = (& git -C $RepoRoot rev-parse HEAD:core/vendor/llama.cpp | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $receipt.source_revision -cne $revision -or $receipt.backend -cne 'cuda') {
        throw 'Published engine differs from the selected checkout/backend.'
    }
    # Preparation owns the install lease. Recheck the selected target immediately
    # before publication, including current lane/process and registered-slot guards.
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    $descriptor = if ($task) { Get-CoreRegisteredRelease -Task $task } else { $null }
    # The release kept the registered engine (Get-CoreKeptEngineSlot): it is already installed,
    # so there is nothing to copy. Re-proven here, under the install lease, before trusting it.
    $kept = Get-CoreKeptEngineSlot -ArtifactDirectory $ArtifactDirectory -Descriptor $descriptor
    if ($kept -and (ConvertTo-CoreImagePath $kept) -ieq (ConvertTo-CoreImagePath $InstallDirectory)) { return }
    $idle = Select-CoreEngineSlot -InstallRoot (Join-Path $env:USERPROFILE '.continuum') -Descriptor $descriptor -Cli (Join-Path $ArtifactDirectory 'continuum.exe')
    if ((ConvertTo-CoreImagePath $idle) -ine (ConvertTo-CoreImagePath $InstallDirectory)) { throw 'Selected engine slot is no longer idle.' }
    $engineSources = @($receipt.files.PSObject.Properties | ForEach-Object { Join-Path $source $_.Name })
    $receiptBytes = [Text.Encoding]::UTF8.GetByteCount(($receipt | ConvertTo-Json -Depth 5))
    $pendingBytes = [Text.Encoding]::UTF8.GetByteCount('pending')
    Assert-CoreArtifactCapacity -ArtifactDirectory $ArtifactDirectory -Destination $InstallDirectory -Sources $engineSources -AdditionalBytes ($receiptBytes + $pendingBytes)
    Copy-CoreEnginePublication -SourceDirectory $source -Directory $InstallDirectory
}
