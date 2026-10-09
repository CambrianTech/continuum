# Fresh-install transport only. Compatibility and complete artifact preparation are
# owned by the published CLI's prepare-prebuilt verb, also used by deploy consumption.

# BEGIN GENERATED PREBUILT INPUTS
$script:CorePrebuiltInputs = @('core/', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'tools/scripts/lib/', 'tools/scripts/shared/cargo-features.sh', 'tools/scripts/start-livekit-windows.ps1', 'tools/scripts/generated/manifest.windows.ps1')
# END GENERATED PREBUILT INPUTS

function Save-CorePrebuiltArchive {
    param([string]$Uri, [string]$OutFile)
    Add-Type -AssemblyName System.Net.Http
    $client = New-Object Net.Http.HttpClient
    $cancel = New-Object Threading.CancellationTokenSource
    $cancel.CancelAfter([TimeSpan]::FromMinutes(30))
    $response = $null; $output = $null
    try {
        $response = $client.GetAsync($Uri, [Net.Http.HttpCompletionOption]::ResponseHeadersRead, $cancel.Token).GetAwaiter().GetResult()
        $response.EnsureSuccessStatusCode() | Out-Null
        $maximum = [long]4294967296
        if ($response.Content.Headers.ContentLength -gt $maximum) { throw 'Published archive exceeds 4 GiB.' }
        $inputStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $output = [IO.File]::Create($OutFile)
        $buffer = New-Object byte[] 65536
        $total = [long]0
        while (($count = $inputStream.ReadAsync($buffer, 0, $buffer.Length, $cancel.Token).GetAwaiter().GetResult()) -gt 0) {
            $total += $count
            if ($total -gt $maximum) { throw 'Published archive exceeds 4 GiB.' }
            $output.Write($buffer, 0, $count)
        }
    } finally {
        if ($output) { $output.Dispose() }; if ($response) { $response.Dispose() }
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
    $download = Join-Path $env:USERPROFILE ('.continuum/cache/bootstrap/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $download -Force | Out-Null
    try {
    $manifestPath = Join-Path $download "continuum-core-$platform.json"
    Write-Step "Downloading published release $($tip.Substring(0, 12)); no developer toolchain is required."
    Save-InstallerSmallFile -Uri "$base/continuum-core-$platform.json" -OutFile $manifestPath
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
        $cacheRoot = [IO.Path]::GetFullPath((Join-Path $env:USERPROFILE '.continuum/cache/bootstrap')).TrimEnd('\') + '\'
        $owned = [IO.Path]::GetFullPath($download)
        if ($owned.StartsWith($cacheRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path $owned -Leaf) -cmatch '^[a-f0-9]{32}$') {
            Remove-Item -LiteralPath $owned -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Copy-CorePublishedEngine {
    param([string]$RepoRoot, [string]$ArtifactDirectory, [string]$InstallDirectory)
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
    $idle = Select-CoreEngineSlot -InstallRoot (Join-Path $env:USERPROFILE '.continuum') -Descriptor $descriptor -Cli (Join-Path $ArtifactDirectory 'continuum.exe')
    if ((ConvertTo-CoreImagePath $idle) -ine (ConvertTo-CoreImagePath $InstallDirectory)) { throw 'Selected engine slot is no longer idle.' }
    Copy-CoreEnginePublication -SourceDirectory $source -Directory $InstallDirectory
}
