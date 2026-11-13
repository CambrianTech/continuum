# LLVM publication completeness. Dot-sourced by win-modules.ps1; uses its
# shared ancestor-link guard. Receipts cover toolchain bytes, not engine slots.
function Get-LlvmRequiredPaths {
    param($Source)
    $version = [regex]::Match($Source.version, '^([0-9]+)\.[0-9]+\.[0-9]+$')
    if (-not $version.Success -or
        $Source.sha256 -notmatch '^[0-9a-fA-F]{64}$' -or -not $Source.extract.StartsWith('members:')) {
        throw 'LLVM manifest must supply a pinned archive and resource version.'
    }
    @('bin/libclang.dll', "lib/clang/$($version.Groups[1].Value)/include/stddef.h", "lib/clang/$($version.Groups[1].Value)/include/stdint.h")
}

function Get-LlvmOwnedPath {
    param([string]$Directory, [string]$Relative)
    if ($Relative -ne 'bin/libclang.dll' -and $Relative -notmatch '^lib/clang/[0-9]+/.+') { throw 'Invalid LLVM receipt member.' }
    if ($Relative -match '[\\:*?"<>|]' -or @($Relative.Split('/') | Where-Object { -not $_ -or $_ -in @('.', '..') -or $_.EndsWith('.') -or $_.EndsWith(' ') }).Count) {
        throw 'Unsafe LLVM receipt member.'
    }
    $path = Join-Path $Directory $Relative
    # The existing ancestor guard also protects publication through a linked bin
    # or resource directory; an LLVM repair must not overwrite a foreign target.
    Assert-ColdMigrationPath -Path $path
    return $path
}

function Get-LlvmStagedReceipt {
    param([string]$Directory, $Source)
    $required = @(Get-LlvmRequiredPaths $Source)
    $files = [ordered]@{}
    foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Recurse -Force -ErrorAction Stop | Sort-Object FullName)) {
        Assert-ColdMigrationPath -Path $item.FullName
        if ($item.PSIsContainer) { continue }
        $relative = $item.FullName.Substring($Directory.TrimEnd('\', '/').Length + 1).Replace('\', '/')
        $path = Get-LlvmOwnedPath -Directory $Directory -Relative $relative
        if ($files.Count -ge 10000 -or $item.Length -le 0) { throw 'LLVM archive member count or size is invalid.' }
        $files[$relative] = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    foreach ($name in $required) { if (-not $files.Contains($name)) { throw "LLVM archive missing required file: $name" } }
    [pscustomobject]@{ schema = 1; url = $Source.url; version = $Source.version; sha256 = $Source.sha256; extract = $Source.extract; files = [pscustomobject]$files }
}

function Assert-LlvmReceiptFiles {
    param([string]$Directory, $Receipt, $Source)
    $required = @(Get-LlvmRequiredPaths $Source)
    if ($Receipt.schema -ne 1 -or $Receipt.url -cne $Source.url -or $Receipt.version -cne $Source.version -or
        $Receipt.sha256 -cne $Source.sha256 -or $Receipt.extract -cne $Source.extract) { throw 'LLVM receipt does not match the manifest source.' }
    $files = @($Receipt.files.PSObject.Properties)
    if ($files.Count -lt $required.Count -or $files.Count -gt 10000) { throw 'LLVM receipt file count is invalid.' }
    foreach ($name in $required) { if ($name -cnotin $files.Name) { throw "LLVM receipt missing required file: $name" } }
    foreach ($entry in $files) {
        $path = Get-LlvmOwnedPath -Directory $Directory -Relative $entry.Name
        if ($entry.Value -cnotmatch '^[0-9a-f]{64}$' -or -not (Test-Path -LiteralPath $path -PathType Leaf) -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ine $entry.Value) { throw "LLVM installed file missing or changed: $($entry.Name)" }
    }
}

function Get-LlvmReceipt {
    param([string]$Directory, $Source)
    $pending = Join-Path $Directory 'llvm-install.pending'
    $path = Join-Path $Directory 'llvm-install.json'
    Assert-ColdMigrationPath -Path $pending
    Assert-ColdMigrationPath -Path $path
    if (Test-Path -LiteralPath $pending) { throw 'LLVM publication is incomplete.' }
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -gt 2MB) { throw 'LLVM completeness receipt is missing or oversized.' }
    $receipt = [IO.File]::ReadAllText($path) | ConvertFrom-Json
    Assert-LlvmReceiptFiles -Directory $Directory -Receipt $receipt -Source $Source
    return $receipt
}

function Publish-LlvmStage {
    param([string]$Stage, [string]$Directory, $Source)
    $receipt = Get-LlvmStagedReceipt -Directory $Stage -Source $Source
    Assert-ColdMigrationPath -Path $Directory
    New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    $pending = Join-Path $Directory 'llvm-install.pending'
    $path = Join-Path $Directory 'llvm-install.json'
    Assert-ColdMigrationPath -Path $pending
    Assert-ColdMigrationPath -Path $path
    # Write intent before any destination byte. A crash, failed copy, or receipt
    # publication leaves this marker for a normal installer rerun to repair.
    [IO.File]::WriteAllText($pending, 'pending', (New-Object Text.UTF8Encoding $false))
    foreach ($entry in $receipt.files.PSObject.Properties) {
        $destination = Get-LlvmOwnedPath -Directory $Directory -Relative $entry.Name
        New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $Stage $entry.Name) -Destination $destination -Force -ErrorAction Stop
    }
    Assert-LlvmReceiptFiles -Directory $Directory -Receipt $receipt -Source $Source
    # Same atomic receipt publication as installed-engine verification, without
    # borrowing engine slots, engine identity, or engine lifecycle semantics.
    $temporary = Join-Path $Directory ([guid]::NewGuid().ToString('N') + '.tmp')
    try {
        [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding $false))
        if (Test-Path -LiteralPath $path) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
    Remove-Item -LiteralPath $pending -Force -ErrorAction Stop
}

