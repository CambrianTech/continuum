# Engine application-input receipts. System/driver DLLs remain a platform contract.
# No receipt is synthesized from an old HEAD:backend stamp.
. (Join-Path $PSScriptRoot 'windows-prepared.ps1')

function Get-CoreEngineCandidates {
    param([string]$Directory, [switch]$IncludeStamp)
    Assert-CorePreparedPath -Path $Directory -Expected $Directory
    foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Force | Where-Object {
        $_.Name -eq 'llama-server.exe' -or $_.Extension -ieq '.dll' -or ($IncludeStamp -and $_.Name -eq '.llama-server.stamp')
    } | Sort-Object Name)) {
        if ($item.PSIsContainer) { throw 'Non-file engine application candidate.' }
        Assert-CorePreparedPath -Path $item.FullName -Expected $item.FullName -File
        $item
    }
}

function Get-CoreEngineFiles {
    param([string]$Directory, [switch]$IncludeStamp)
    $files = [ordered]@{}
    $bytes = [long]0
    foreach ($item in @(Get-CoreEngineCandidates -Directory $Directory -IncludeStamp:$IncludeStamp)) {
        $bytes += $item.Length
        if ($files.Count -ge 256 -or $bytes -gt 4294967296) { throw 'Engine application inputs exceed verification bounds.' }
        $files[$item.Name.ToLowerInvariant()] = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    if (-not $files.Contains('llama-server.exe')) { throw 'Engine executable missing.' }
    return $files
}

function Start-CoreEnginePublication {
    param([string]$Directory)
    $pending = Join-Path $Directory 'engine-install.pending'
    Assert-CorePreparedPath -Path $pending -Expected $pending
    # Written before changing application bytes; an interrupted first publication
    # cannot masquerade as an ordinary unreceipted legacy install.
    [IO.File]::WriteAllText($pending, 'pending', (New-Object Text.UTF8Encoding $false))
}

function Get-CoreEngineReceipt {
    param([string]$Directory, [switch]$Publishing)
    if (-not $Publishing -and (Test-Path -LiteralPath (Join-Path $Directory 'engine-install.pending'))) { throw 'Engine publication is incomplete.' }
    $path = Join-Path $Directory 'engine-install.json'
    Assert-CorePreparedPath -Path $path -Expected $path -File
    if ((Get-Item -LiteralPath $path).Length -gt 1048576) { throw 'Engine receipt is oversized.' }
    $receipt = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    $fields = @('schema', 'source_revision', 'backend', 'build_contract', 'runtime_origin', 'platform_contract', 'files')
    $names = @($receipt.PSObject.Properties.Name)
    if ($names.Count -ne $fields.Count -or @($names | Where-Object { $_ -notin $fields }).Count -or
        $receipt.schema -ne 1 -or $receipt.source_revision -cnotmatch '^[0-9a-f]{40}$' -or
        $receipt.backend -notin @('cpu', 'cuda') -or $receipt.build_contract -cne 'static-local-backends-v1' -or
        $receipt.platform_contract -cne 'windows-system32-nvidia-driver-v1' -or
        $receipt.runtime_origin -cne 'installed-toolkit-bin-snapshot') { throw 'Unsupported engine receipt contract.' }
    $actual = Get-CoreEngineFiles -Directory $Directory -IncludeStamp:('.llama-server.stamp' -in @($receipt.files.PSObject.Properties.Name))
    $expected = @($receipt.files.PSObject.Properties)
    if ($expected.Count -ne $actual.Count) { throw 'Engine application candidate membership changed.' }
    foreach ($entry in $expected) {
        if (-not $actual.Contains($entry.Name) -or $entry.Value -cnotmatch '^[0-9a-f]{64}$' -or
            $actual[$entry.Name] -cne $entry.Value) { throw "Engine application input changed: $($entry.Name)" }
    }
    return $receipt
}

function Save-CoreEngineReceipt {
    param([string]$Directory, [string]$SourceRevision, [string]$Backend)
    if ($SourceRevision -cnotmatch '^[0-9a-f]{40}$' -or $Backend -notin @('cpu', 'cuda')) { throw 'Invalid fresh engine build identity.' }
    $receipt = [ordered]@{ schema = 1; source_revision = $SourceRevision; backend = $Backend;
        build_contract = 'static-local-backends-v1'; runtime_origin = 'installed-toolkit-bin-snapshot';
        platform_contract = 'windows-system32-nvidia-driver-v1'; files = (Get-CoreEngineFiles -Directory $Directory -IncludeStamp) }
    $path = Join-Path $Directory 'engine-install.json'
    Assert-CorePreparedPath -Path $path -Expected $path
    $temporary = Join-Path $Directory ([guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
        if (Test-Path -LiteralPath $path) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
    Get-CoreEngineReceipt -Directory $Directory -Publishing | Out-Null
    $pending = Join-Path $Directory 'engine-install.pending'
    if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending }
}

function Copy-CoreEnginePublication {
    # The installer holds install.lock and selects an idle managed slot before
    # entering here. Pending prevents admission while replacing its whole DLL
    # namespace; a failed copy is deliberately repairable by the same operation.
    param([string]$SourceDirectory, [string]$Directory)
    $receipt = Get-CoreEngineReceipt -Directory $SourceDirectory
    $stamp = Get-CorePublishedEngineStamp -Directory $SourceDirectory -Receipt $receipt
    Assert-CorePreparedPath -Path $Directory -Expected $Directory
    if ([IO.Path]::GetFullPath($SourceDirectory) -ieq [IO.Path]::GetFullPath($Directory)) { throw 'Engine publication source equals destination.' }
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $existing = @(Get-CoreEngineCandidates -Directory $Directory -IncludeStamp)
    # Validate every old leaf (including obsolete DLLs) before touching any byte.
    foreach ($item in $existing) {
        $probe = [IO.File]::Open($item.FullName, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $probe.Dispose()
    }
    Start-CoreEnginePublication -Directory $Directory
    foreach ($entry in $receipt.files.PSObject.Properties) {
        $target = Join-Path $Directory $entry.Name
        Assert-CorePreparedPath -Path $target -Expected $target
        Copy-Item -LiteralPath (Join-Path $SourceDirectory $entry.Name) -Destination $target -Force -ErrorAction Stop
    }
    foreach ($item in $existing) {
        if ($item.Name -notin @($receipt.files.PSObject.Properties.Name)) {
            Assert-CorePreparedPath -Path $item.FullName -Expected $item.FullName -File
            Remove-Item -LiteralPath $item.FullName -ErrorAction Stop
        }
    }
    if ((Get-CorePublishedEngineStamp -Directory $Directory -Receipt $receipt) -cne $stamp) { throw 'Published engine stamp changed during copy.' }
    Copy-CoreEngineReceipt -SourceDirectory $SourceDirectory -Directory $Directory
}

function Get-CorePublishedEngineStamp {
    param([string]$Directory, [Parameter(Mandatory = $true)]$Receipt)
    $path = Join-Path $Directory '.llama-server.stamp'
    Assert-CorePreparedPath -Path $path -Expected $path -File
    if ('.llama-server.stamp' -notin @($Receipt.files.PSObject.Properties.Name) -or
        (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $Receipt.files.'.llama-server.stamp') { throw 'Published engine stamp is not bound by its receipt.' }
    if ((Get-Item -LiteralPath $path).Length -gt 256) { throw 'Published engine stamp is oversized.' }
    $stamp = (Get-Content -LiteralPath $path -Raw).Trim()
    $parts = $stamp.Split(':')
    if ($stamp -cnotmatch '^[0-9a-f]{7,40}:(cpu|cuda)(:[a-zA-Z0-9_.;-]+)*$' -or
        -not $Receipt.source_revision.StartsWith($parts[0], [StringComparison]::Ordinal) -or
        $parts[1] -cne $Receipt.backend) { throw 'Published engine stamp differs from verified receipt.' }
    return $stamp
}

function Copy-CoreEngineReceipt {
    param([string]$SourceDirectory, [string]$Directory)
    $receipt = Get-CoreEngineReceipt -Directory $SourceDirectory
    $actual = Get-CoreEngineFiles -Directory $Directory -IncludeStamp:('.llama-server.stamp' -in @($receipt.files.PSObject.Properties.Name))
    if ($actual.Count -ne @($receipt.files.PSObject.Properties).Count) { throw 'Copied engine candidate membership differs.' }
    foreach ($entry in $receipt.files.PSObject.Properties) {
        if (-not $actual.Contains($entry.Name) -or $actual[$entry.Name] -cne $entry.Value) { throw 'Copied engine bytes differ.' }
    }
    $path = Join-Path $Directory 'engine-install.json'
    Assert-CorePreparedPath -Path $path -Expected $path
    $temporary = Join-Path $Directory ([guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
        if (Test-Path -LiteralPath $path) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary } }
    Get-CoreEngineReceipt -Directory $Directory -Publishing | Out-Null
    $pending = Join-Path $Directory 'engine-install.pending'
    if (Test-Path -LiteralPath $pending) { Remove-Item -LiteralPath $pending }
}
