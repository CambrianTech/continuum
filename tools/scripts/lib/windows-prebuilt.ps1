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
    $manifestPath = Join-Path $download "continuum-core-$platform.json"
    Write-Step "Downloading published release $($tip.Substring(0, 12)); no developer toolchain is required."
    Save-InstallerSmallFile -Uri "$base/continuum-core-$platform.json" -OutFile $manifestPath
    if ((Get-Item -LiteralPath $manifestPath).Length -gt 65536) { throw 'Published manifest exceeds 64 KiB.' }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    # Only transport identity and checksum are interpreted before the first CLI runs.
    Assert-CoreBootstrapManifest -Manifest $manifest -Tip $tip -Platform $platform
    $archive = Join-Path $download $manifest.archive
    Save-CorePrebuiltArchive -Uri "$base/$($manifest.archive)" -OutFile $archive
    Assert-Sha256 -Path $archive -Expected $manifest.sha256 -Name 'published release'
    $members = @(& tar.exe -tzf $archive)
    if ($LASTEXITCODE -ne 0 -or @($members | Where-Object { $_ -ceq "continuum-core-$platform/continuum.exe" }).Count -ne 1) {
        throw 'Published archive must contain exactly one bootstrap CLI.'
    }
    $cliEntry = @(& tar.exe -tvzf $archive "continuum-core-$platform/continuum.exe")
    if ($LASTEXITCODE -ne 0 -or $cliEntry.Count -ne 1 -or -not $cliEntry[0].StartsWith('-')) { throw 'Bootstrap CLI must be a regular archive file.' }
    # Read one exact member as bytes, never extract archive-controlled paths or links.
    $cli = Join-Path $download 'continuum.exe'
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = (Get-Command tar.exe -CommandType Application -ErrorAction Stop).Source
    $start.WorkingDirectory = $download
    $start.Arguments = "-xOf $($manifest.archive) continuum-core-$platform/continuum.exe"
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $start
    $output = [IO.File]::Create($cli)
    try {
        if (-not $process.Start()) { throw 'Cannot extract the published bootstrap CLI.' }
        $copy = $process.StandardOutput.BaseStream.CopyToAsync($output)
        $errors = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(180000)) { $process.Kill(); throw 'Bootstrap extraction timed out.' }
        $copy.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { throw "Bootstrap extraction failed: $($errors.GetAwaiter().GetResult())" }
    } finally { $output.Dispose(); $process.Dispose() }
    if ((Get-Item -LiteralPath $cli).Length -eq 0) { throw 'Published archive lacks a bootstrap CLI.' }
    $prepared = @(Invoke-InstallerProcess $cli @('prepare-prebuilt', $RepoRoot, $download))
    if ($LASTEXITCODE -ne 0) { throw 'Published release preparation refused; no source build will be attempted.' }
    $result = ($prepared -join "`n") | ConvertFrom-Json
    if ($result.git_sha -cne $tip -or -not (Test-Path -LiteralPath $result.core -PathType Leaf)) { throw 'Invalid prepared artifact result.' }
    return (Split-Path $result.core -Parent)
}

function Copy-CorePublishedEngine {
    param([string]$RepoRoot, [string]$ArtifactDirectory, [string]$InstallDirectory)
    $source = Join-Path $ArtifactDirectory 'engine'
    $receipt = Get-CoreEngineReceipt -Directory $source
    $revision = (& git -C $RepoRoot rev-parse HEAD:core/vendor/llama.cpp | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $receipt.source_revision -cne $revision -or $receipt.backend -cne 'cuda') {
        throw 'Published engine differs from the selected checkout/backend.'
    }
    New-Item -ItemType Directory -Path $InstallDirectory -Force | Out-Null
    Start-CoreEnginePublication -Directory $InstallDirectory
    foreach ($entry in $receipt.files.PSObject.Properties) {
        Copy-Item -LiteralPath (Join-Path $source $entry.Name) -Destination (Join-Path $InstallDirectory $entry.Name) -Force -ErrorAction Stop
    }
    Copy-CoreEngineReceipt -SourceDirectory $source -Directory $InstallDirectory
}
