# Integration smoke: acquire the manifest-pinned decoder in a scratch profile,
# then exercise native Windows tar on XZ without relying on a host decoder.
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
    $decoder = Get-ManagedXzDecoder
    if (-not $decoder.StartsWith($env:USERPROFILE, [StringComparison]::OrdinalIgnoreCase)) { throw 'Decoder escaped scratch profile' }
    # No acquisition on the second call: the same real, version-checked decoder.
    function Invoke-WebRequest { throw 'Healthy decoder attempted a second download' }
    if ((Get-ManagedXzDecoder) -ne $decoder) { throw 'Decoder rerun changed location' }
    $tree = Join-Path $scratch 'source'
    New-Item -ItemType Directory -Path (Join-Path $tree 'release\bin') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $tree 'release\bin\libclang.dll'), 'fixture payload')
    $tar = Join-Path $scratch 'fixture.tar'
    & "$env:SystemRoot\System32\tar.exe" -cf $tar -C $tree release
    if ($LASTEXITCODE -ne 0) { throw 'Fixture tar creation failed' }
    & $decoder -k $tar
    if ($LASTEXITCODE -ne 0) { throw 'Fixture XZ compression failed' }
    $destination = Join-Path $scratch 'output'
    New-Item -ItemType Directory -Path $destination | Out-Null
    $env:PATH = "$env:SystemRoot\System32"
    Expand-ManagedTarXz -Archive ($tar + '.xz') -Destination $destination -Members @('*/bin/libclang.dll')
    if ([IO.File]::ReadAllText((Join-Path $destination 'bin\libclang.dll')) -ne 'fixture payload') { throw 'Native XZ extraction failed' }
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
