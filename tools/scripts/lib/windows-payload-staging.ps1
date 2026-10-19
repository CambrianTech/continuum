# Preparation belongs to the installed payload owner. This does NOT change
# payload-root, selection receipts, scheduled tasks, or retire source files.
# A staged journal is evidence for a later custody transition, not permission
# to activate it. In particular protocol-4 maintenance is still required.
. (Join-Path $PSScriptRoot 'windows-service.ps1')

function Open-CorePayloadDirectory {
    param([string]$Path)
    if (-not ('Continuum.Setup.PayloadDirectory' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
namespace Continuum.Setup {
public static class PayloadDirectory {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string path,uint access,uint share,IntPtr security,uint creation,uint flags,IntPtr template);
    public static SafeFileHandle Open(string path) {
        var handle=CreateFileW(path,0,3,IntPtr.Zero,3,0x02000000,IntPtr.Zero);
        if(handle.IsInvalid) { handle.Dispose(); throw new Win32Exception(); }
        return handle;
    }
}}
'@
    }
    Assert-CorePreparedPath -Path $Path -Expected $Path
    $pin = [Continuum.Setup.PayloadDirectory]::Open($Path)
    try { Assert-CorePreparedPath -Path $Path -Expected $Path; return $pin }
    catch { $pin.Dispose(); throw }
}

function Get-CorePayloadStageFiles {
    param([string]$Root, $DirectoryPins)
    $files = @()
    foreach ($name in @('bin', 'tools', 'lib', 'cuda-toolkit')) {
        $directory = Join-Path $Root $name
        if (-not (Test-Path -LiteralPath $directory)) { continue }
        Assert-CorePreparedPath -Path $directory -Expected $directory
        # Inspect each directory BEFORE descending, never follow junctions.
        $pending = New-Object 'Collections.Generic.Stack[string]'
        $pending.Push($directory)
        while ($pending.Count) {
            $current = $pending.Pop()
            if ($null -ne $DirectoryPins) { $DirectoryPins.Add((Open-CorePayloadDirectory $current)) }
            foreach ($entry in Get-ChildItem -LiteralPath $current -Force -ErrorAction Stop) {
                if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Payload staging refuses redirected members.' }
                if ($entry.PSIsContainer) { $pending.Push($entry.FullName) }
                else { $files += $entry.FullName }
            }
        }
    }
    return @($files | Sort-Object)
}

function New-CorePayloadStage {
    param([Parameter(Mandatory=$true)][string]$HomeRoot,
          [Parameter(Mandatory=$true)][string]$ColdRoot,
          [Parameter(Mandatory=$true)][string]$ArtifactDirectory)
    # Same no-sharing install.lock used by the installer/lifecycle owner. Take
    # it before reading placement or absent records, not after the snapshot.
    $leasePath = Join-Path $HomeRoot 'install.lock'
    Assert-CorePreparedPath -Path $leasePath -Expected $leasePath
    $lease = [IO.File]::Open($leasePath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    $directories = New-Object 'Collections.Generic.List[IDisposable]'
    try {
    $source = [IO.Path]::GetFullPath((Get-ManagedPayloadRoot -HomeRoot $HomeRoot)).TrimEnd('\')
    Assert-CorePreparedPath -Path $source -Expected $source
    $directories.Add((Open-CorePayloadDirectory $source))
    Assert-CorePreparedPath -Path $ColdRoot -Expected $ColdRoot
    if (-not (Test-Path -LiteralPath $ColdRoot -PathType Container)) { throw 'Configured cold storage is unavailable.' }
    $destination = Get-ManagedColdPayloadRoot -HomeRoot $HomeRoot -ColdRoot $ColdRoot
    Assert-CorePreparedPath -Path $destination -Expected $destination
    # Pin every existing ancestor against rename/deletion through admission and
    # writes. Missing payloads is created only beneath the retained cold root.
    $cursor = [IO.Path]::GetFullPath($ColdRoot)
    while ($cursor) {
        $directories.Add((Open-CorePayloadDirectory $cursor))
        $parent = Split-Path $cursor -Parent
        if ($parent -eq $cursor) { break }; $cursor = $parent
    }
    if ([IO.Path]::GetFullPath($destination).StartsWith($source + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Cold staging must be outside the source payload.' }
    if (Test-Path -LiteralPath $destination) { throw 'Cold payload destination already exists; refusing unowned replacement.' }
    $files = @(Get-CorePayloadStageFiles $source $directories)
    if (-not $files.Count) { throw 'Installed payload closure is empty.' }
    $pins = New-Object 'Collections.Generic.List[IO.FileStream]'
    try {
        $members = @()
        foreach ($file in $files) {
            $pin = [IO.File]::Open($file, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            $pins.Add($pin)
            $hash = [Security.Cryptography.SHA256]::Create()
            try { $digest = ([BitConverter]::ToString($hash.ComputeHash($pin))).Replace('-', '').ToLowerInvariant() } finally { $hash.Dispose() }
            $pin.Position = 0
            $members += [ordered]@{ relative = $file.Substring($source.Length + 1); bytes = $pin.Length; sha256 = $digest }
        }
        # Capture old records verbatim. Do not reseal or rewrite them as though
        # the proposed destination were already an installed selection.
        $records = @{}
        foreach ($name in @('payload-root', 'install-active.json', 'install-previous.json', 'install-prepared.json')) {
            $path = Join-Path $HomeRoot $name
            if (Test-Path -LiteralPath $path) {
                Assert-CorePreparedPath -Path $path -Expected $path -File
                $pin = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
                $pins.Add($pin)
                if ($pin.Length -gt 65536) { throw 'Installed selection record exceeds receipt limit.' }
                $bytes = New-Object byte[] ([int]$pin.Length)
                $read = 0
                while ($read -lt $bytes.Length) { $n = $pin.Read($bytes, $read, $bytes.Length-$read); if (-not $n) { throw 'Short selection record read.' }; $read += $n }
                $records[$name] = [Convert]::ToBase64String($bytes)
            } else { $records[$name] = $null }
        }
        $journal = [ordered]@{ schema=1; phase='copying'; source=$source; proposed_payload=$destination; original_records=$records; members=$members }
        $text = $journal | ConvertTo-Json -Depth 8
        $journal.phase = 'verified-staging-only'
        $verifiedText = $journal | ConvertTo-Json -Depth 8
        $journal.phase = 'copying'
        $journalBytes = [Text.Encoding]::UTF8.GetByteCount($text) + [Text.Encoding]::UTF8.GetByteCount($verifiedText)
        # Both the initial and completed journal coexist; no overwrite credit.
        Assert-CoreArtifactCapacity -ArtifactDirectory $ArtifactDirectory -Destination $ColdRoot -Sources $files -AdditionalBytes ([uint64]$journalBytes)
        $parent = Split-Path $destination -Parent
        Assert-CorePreparedPath -Path $parent -Expected $parent
        if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent -ErrorAction Stop | Out-Null }
        $directories.Add((Open-CorePayloadDirectory $parent))
        $staging = $destination + '.staging-' + [guid]::NewGuid().ToString('N')
        Assert-CorePreparedPath -Path $staging -Expected $staging
        New-Item -ItemType Directory -Path $staging -ErrorAction Stop | Out-Null
        $directories.Add((Open-CorePayloadDirectory $staging))
        $journalPath = Join-Path $staging 'placement-copying.json'
        $stream = [IO.File]::Open($journalPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        try { $bytes = [Text.Encoding]::UTF8.GetBytes($text); $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
        for ($index=0; $index -lt $members.Count; $index++) {
            $target = Join-Path (Join-Path $staging 'payload') $members[$index].relative
            New-Item -ItemType Directory -Path (Split-Path $target) -Force | Out-Null
            $out = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
            try { $pins[$index].CopyTo($out); $out.Flush($true) } finally { $out.Dispose() }
            if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ine $members[$index].sha256) { throw 'Staged payload hash mismatch; source preserved.' }
        }
        if (Compare-Object $files @(Get-CorePayloadStageFiles $source)) { throw 'Payload membership changed during staging; source preserved.' }
        foreach ($name in $records.Keys) {
            $path = Join-Path $HomeRoot $name
            $exists = Test-Path -LiteralPath $path
            if ($null -eq $records[$name]) {
                if ($exists) { throw 'Previously absent selection record appeared during staging.' }
            } else {
                Assert-CorePreparedPath -Path $path -Expected $path -File
                if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($path)) -cne $records[$name]) { throw 'Selection record changed during staging.' }
            }
        }
        $journal.phase = 'verified-staging-only'
        $complete = [IO.File]::Open((Join-Path $staging 'placement-verified.json'), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
        try { $bytes=[Text.Encoding]::UTF8.GetBytes($verifiedText); $complete.Write($bytes,0,$bytes.Length); $complete.Flush($true) } finally { $complete.Dispose() }
        return $staging
    } finally { foreach ($pin in $pins) { $pin.Dispose() } }
    } finally { foreach ($directory in $directories) { $directory.Dispose() }; $lease.Dispose() }
}
