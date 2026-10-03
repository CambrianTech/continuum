# Integration smoke: acquire the manifest-pinned decoder in a scratch profile,
# then exercise native Windows tar on XZ without relying on a host decoder.
param([string]$DecoderArchive, [switch]$PreseedDecoder)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
. (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$scratch = Join-Path $tempRoot ('continuum-xz-test-' + [guid]::NewGuid().ToString('N'))
if (-not ([IO.Path]::GetFullPath($scratch)).StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe scratch directory' }
$savedProfile = $env:USERPROFILE; $savedTemp = $env:TEMP; $savedPath = $env:PATH; $savedLibclang = $env:LIBCLANG_PATH
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $env:USERPROFILE = Join-Path $scratch 'profile'
    $env:TEMP = $scratch
    $payloadRoot = Initialize-ManagedPayloadRoot -ColdRoot (Join-Path $env:USERPROFILE 'cold [fixture]')
    if ($DecoderArchive) {
        $pin = (Get-ManifestModule 'xz-decoder').source
        Copy-Item -LiteralPath $DecoderArchive -Destination (Join-Path $scratch "xz-$($pin.version)-windows.zip")
    }
    if ($PreseedDecoder) {
        # Publication-only PS5 coverage when its unrelated Expand-Archive
        # acquisition hangs. Default CI still exercises the production decoder
        # acquisition. This mode only seeds verified scratch decoder bytes.
        if (-not $DecoderArchive) { throw 'PreseedDecoder requires the pinned decoder archive.' }
        Assert-Sha256 -Path $DecoderArchive -Expected $pin.sha256 -Name 'fixture decoder'
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $decoderDirectory = Join-Path $payloadRoot 'tools\xz'
        New-Item -ItemType Directory -Path $decoderDirectory -Force | Out-Null
        [IO.Compression.ZipFile]::ExtractToDirectory($DecoderArchive, $decoderDirectory)
    }
    $decoder = Get-ManagedXzDecoder
    if (-not $decoder.StartsWith($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase)) { throw 'Decoder escaped scratch profile' }
    # No acquisition on the second call: the same real, version-checked decoder.
    function Invoke-WebRequest { throw 'Healthy decoder attempted a second download' }
    if ((Get-ManagedXzDecoder) -ne $decoder) { throw 'Decoder rerun changed location' }
    $tree = Join-Path $scratch 'source'
    New-Item -ItemType Directory -Path (Join-Path $tree 'release\bin') -Force | Out-Null
    # Incompressible input larger than the Windows external-filter pipe buffers:
    # the former tiny fixture missed the live LLVM decode stall.
    $payload = New-Object byte[] (2MB)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $random.GetBytes($payload) } finally { $random.Dispose() }
    $sourceFile = Join-Path $tree 'release\bin\libclang.dll'
    [IO.File]::WriteAllBytes($sourceFile, $payload)
    $include = Join-Path $tree 'release\lib\clang\18\include'
    New-Item -ItemType Directory -Path $include -Force | Out-Null
    foreach ($header in @('stddef.h', 'stdint.h', 'extra.h')) { [IO.File]::WriteAllText((Join-Path $include $header), "fixture $header") }
    $expectedHash = (Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash
    $tar = Join-Path $scratch 'fixture.tar'
    Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\tar.exe" @('-cf', $tar, '-C', $tree, 'release')
    if ($LASTEXITCODE -ne 0) { throw 'Fixture tar creation failed' }
    Invoke-InstallerProcess -OwnProcessTree $decoder @('-k', $tar)
    if ($LASTEXITCODE -ne 0) { throw 'Fixture XZ compression failed' }
    if ((Get-Item -LiteralPath ($tar + '.xz')).Length -le 1MB) { throw 'Fixture no longer exceeds pipe capacity' }
    $destination = Join-Path $scratch 'output'
    New-Item -ItemType Directory -Path $destination | Out-Null
    $env:PATH = "$env:SystemRoot\System32"
    Expand-ManagedTarXz -Archive ($tar + '.xz') -Destination $destination -Members @('*/bin/libclang.dll')
    if ((Get-FileHash -LiteralPath (Join-Path $destination 'bin\libclang.dll') -Algorithm SHA256).Hash -ne $expectedHash) { throw 'Native XZ extraction changed binary bytes' }
    if (Get-ChildItem -LiteralPath $destination -Filter '*.tar') { throw 'Decoded tar was not cleaned up' }
    if ($env:PATH -ne "$env:SystemRoot\System32") { throw 'Extractor leaked decoder PATH' }
    $bad = Join-Path $scratch 'bad.tar.xz'
    [IO.File]::WriteAllText($bad, 'corrupt')
    $refused = $false
    try { Expand-ManagedTarXz -Archive $bad -Destination $destination -Members @('*/bin/libclang.dll') } catch { $refused = $_ -match 'Archive extraction failed' }
    if (-not $refused) { throw 'Corrupt archive was accepted' }
    if ($env:PATH -ne "$env:SystemRoot\System32") { throw 'Failed extractor leaked decoder PATH' }
    # Interrupted publication used to leave a DLL that the normal installer
    # accepted even though bindgen's resource headers had never been copied.
    & {
        $manifestLookup = ${function:Get-ManifestModule}
        $fixtureSource = @{ url='https://fixture.invalid/fixture.tar.xz'; version='18.1.8';
            sha256=(Get-FileHash -LiteralPath ($tar + '.xz') -Algorithm SHA256).Hash.ToLowerInvariant();
            extract='members:*/bin/libclang.dll,*/lib/clang/*' }
        function Get-ManifestModule { param($Id) if ($Id -eq 'llvm-libclang') { return @{source=$fixtureSource} }; & $manifestLookup $Id }
        # Exercise the real module without writing the operator's user registry.
        function Set-LlvmEnvironment { param($Bin,[switch]$ExistingOnly) $env:LIBCLANG_PATH=$Bin }
        $llvm = Join-Path $payloadRoot 'tools\llvm'
        New-Item -ItemType Directory -Path (Join-Path $llvm 'bin') -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $llvm 'bin\libclang.dll'), 'legacy DLL only')
        [IO.File]::WriteAllText((Join-Path $llvm 'unrelated.txt'), 'preserve me')
        $refused = $false
        try { Mod-LLVM -ExistingOnly } catch { $refused = $_ -match 'complete LLVM' }
        if (-not $refused) { throw 'DLL-only legacy install was accepted' }
        $script:llvmCopies = 0; $script:llvmFailCopy = $true
        function Copy-Item {
            [CmdletBinding()] param($LiteralPath,$Destination,[switch]$Force)
            $script:llvmCopies++
            if ($script:llvmFailCopy -and $script:llvmCopies -eq 2) { throw 'simulated interrupted LLVM copy' }
            Microsoft.PowerShell.Management\Copy-Item -LiteralPath $LiteralPath -Destination $Destination -Force:$Force -ErrorAction Stop
        }
        $refused = $false
        try { Mod-LLVM } catch { $refused = $_ -match 'simulated interrupted LLVM copy' }
        if (-not $refused -or -not (Test-Path -LiteralPath (Join-Path $llvm 'llvm-install.pending'))) { throw 'Interrupted publication lost intent' }
        if (Test-Path -LiteralPath (Join-Path $llvm 'llvm-install.json')) { throw 'Partial publication received a receipt' }
        $refused = $false
        try { Mod-LLVM -ExistingOnly } catch { $refused = $_ -match 'incomplete' }
        if (-not $refused) { throw 'Preparation accepted interrupted publication' }
        $script:llvmFailCopy = $false
        Mod-LLVM
        $receipt = Get-LlvmReceipt -Directory $llvm -Source $fixtureSource
        if (@($receipt.files.PSObject.Properties).Count -ne 4) { throw 'Receipt omitted selected resource files' }
        if ($env:LIBCLANG_PATH -ne (Join-Path $llvm 'bin')) { throw 'LLVM ignored managed cold payload root' }
        $copies = $script:llvmCopies
        Mod-LLVM; Mod-LLVM -ExistingOnly
        if ($script:llvmCopies -ne $copies) { throw 'Healthy rerun copied LLVM again' }
        foreach ($damage in @('header', 'receipt', 'pending', 'source')) {
            switch ($damage) {
                'header' { [IO.File]::WriteAllText((Join-Path $llvm 'lib\clang\18\include\extra.h'), 'corrupt') }
                'receipt' { [IO.File]::WriteAllText((Join-Path $llvm 'llvm-install.json'), 'incomplete json') }
                'pending' { [IO.File]::WriteAllText((Join-Path $llvm 'llvm-install.pending'), 'interrupted after receipt publication') }
                'source' { $fixtureSource.version = '18.1.9' }
            }
            $refused = $false
            try { Mod-LLVM -ExistingOnly } catch { $refused = $_ -match 'complete LLVM' }
            if (-not $refused) { throw "Preparation accepted damaged LLVM: $damage" }
            Mod-LLVM
            Get-LlvmReceipt -Directory $llvm -Source $fixtureSource | Out-Null
        }
        if ([IO.File]::ReadAllText((Join-Path $llvm 'unrelated.txt')) -cne 'preserve me') { throw 'Repair changed unrelated data' }
        Write-Output 'PASS: LLVM interruption/recovery, complete hashes, unchanged rerun, cold paths and unrelated-file preservation'
    }
    if ($PreseedDecoder) { Write-Output 'PASS: preseeded verified XZ reuse, native tar extraction, corrupt refusal and PATH restoration (decoder acquisition not exercised)' }
    else { Write-Output 'PASS: pinned XZ acquisition/reuse, native tar extraction, corrupt refusal and PATH restoration' }
} finally {
    $env:USERPROFILE = $savedProfile; $env:TEMP = $savedTemp; $env:PATH = $savedPath; $env:LIBCLANG_PATH = $savedLibclang
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
# GitHub's PowerShell wrapper forwards LASTEXITCODE. The deliberate corrupt
# archive above sets it nonzero; succeed only after every assertion and cleanup.
exit 0
