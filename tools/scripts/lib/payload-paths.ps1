# Shared deployment-path record. Runtime paths::payload_root reads the same
# UTF-8 absolute-path file; cache/model placement is deliberately independent.
function Get-ManagedPayloadRoot {
    param([string]$HomeRoot = (Join-Path $env:USERPROFILE '.continuum'))
    $record = Join-Path $HomeRoot 'payload-root'
    if (-not (Test-Path -LiteralPath $record)) { return $HomeRoot }
    $root = [IO.File]::ReadAllText($record).TrimEnd("`r", "`n")
    if (-not $root -or $root -match "[`r`n]" -or $root -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)') {
        throw "$record must name one absolute payload directory."
    }
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw "Selected payload directory $root is unavailable." }
    return $root
}

function Initialize-ManagedPayloadRoot {
    param([string]$HomeRoot = (Join-Path $env:USERPROFILE '.continuum'), [string]$ColdRoot)
    $record = Join-Path $HomeRoot 'payload-root'
    if (Test-Path -LiteralPath $record) { return Get-ManagedPayloadRoot -HomeRoot $HomeRoot }
    # Existing installs keep their running images and rollback pointers. Merely
    # routing models to cold storage never relocates a live installed engine.
    $legacy = @('bin', 'tools', 'lib', 'cuda-toolkit' | Where-Object {
        Test-Path -LiteralPath (Join-Path $HomeRoot $_)
    }).Count -gt 0
    $root = $HomeRoot
    if ($ColdRoot -and -not $legacy) {
        $hash = [Security.Cryptography.SHA256]::Create()
        try {
            $scope = ([BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes([IO.Path]::GetFullPath($HomeRoot))))).Replace('-', '').ToLowerInvariant().Substring(0, 16)
        } finally { $hash.Dispose() }
        $root = Join-Path (Join-Path $ColdRoot 'payloads') $scope
        if ((Test-Path -LiteralPath $root) -and @(Get-ChildItem -LiteralPath $root -Force -ErrorAction Stop).Count) {
            throw "Unowned payload directory $root is not empty; refusing to adopt another install."
        }
    }
    New-Item -ItemType Directory -Force -Path $HomeRoot, $root | Out-Null
    $root = [IO.Path]::GetFullPath($root)
    $temporary = $record + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, ($root + "`n"), (New-Object Text.UTF8Encoding $false))
        [IO.File]::Move($temporary, $record)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
    return $root
}
