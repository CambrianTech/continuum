# Integration smoke: acquire the manifest-pinned decoder in a scratch profile,
# then exercise native Windows tar on XZ without relying on a host decoder.
param([string]$DecoderArchive)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
. (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$scratch = Join-Path $tempRoot ('continuum-xz-test-' + [guid]::NewGuid().ToString('N'))
if (-not ([IO.Path]::GetFullPath($scratch)).StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe scratch directory' }
$savedProfile = $env:USERPROFILE; $savedTemp = $env:TEMP; $savedPath = $env:PATH
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    $env:USERPROFILE = Join-Path $scratch 'profile'
    $env:TEMP = $scratch
    if ($DecoderArchive) {
        $pin = (Get-ManifestModule 'xz-decoder').source
        Copy-Item -LiteralPath $DecoderArchive -Destination (Join-Path $scratch "xz-$($pin.version)-windows.zip")
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
    $expectedHash = (Get-FileHash -LiteralPath $sourceFile -Algorithm SHA256).Hash
    $tar = Join-Path $scratch 'fixture.tar'
    & "$env:SystemRoot\System32\tar.exe" -cf $tar -C $tree release
    if ($LASTEXITCODE -ne 0) { throw 'Fixture tar creation failed' }
    & $decoder -k $tar
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
    Write-Output 'PASS: pinned XZ acquisition/reuse, native tar extraction, corrupt refusal and PATH restoration'
} finally {
    $env:USERPROFILE = $savedProfile; $env:TEMP = $savedTemp; $env:PATH = $savedPath
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
# GitHub's PowerShell wrapper forwards LASTEXITCODE. The deliberate corrupt
# archive above sets it nonzero; succeed only after every assertion and cleanup.
exit 0
