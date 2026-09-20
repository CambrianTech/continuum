# Explicit prepared-release deployment, not a source/config cache hit.
# Receipts capture artifact integrity at preparation, never historical build inputs.
function Assert-CorePreparedPath {
    param([string]$Path, [string]$Expected, [switch]$File)
    if ($Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\)' -or
        (ConvertTo-CoreImagePath $Path) -ne (ConvertTo-CoreImagePath $Expected)) {
        throw "Prepared release path is outside its expected installed layout: $Path"
    }
    $cursor = [IO.Path]::GetFullPath($Expected)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force -ErrorAction Stop
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Prepared release path is redirected: $cursor"
            }
        }
        $parent = Split-Path $cursor -Parent
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    if ($File -and -not (Test-Path -LiteralPath $Expected -PathType Leaf)) { throw "Prepared release file is missing: $Expected" }
}

function Assert-CorePreparedRelease {
    param($Release, [string]$InstallRoot)
    $fields = @('artifact', 'cli', 'launcher', 'engine', 'socket', 'logDirectory')
    $names = @($Release.PSObject.Properties.Name)
    if ($names.Count -ne $fields.Count -or @($names | Where-Object { $_ -notin $fields }).Count) {
        throw 'Prepared release descriptor has unexpected or missing fields.'
    }
    foreach ($name in $fields) {
        $value = $Release.$name
        if ($value -isnot [string] -or -not $value -or
            $value.IndexOfAny([char[]]@('"', "`r", "`n", [char]0)) -ge 0 -or $value.EndsWith('\')) {
            throw "Prepared release field $name is not a safe absolute argument."
        }
    }
    $slot = Split-Path $Release.artifact -Parent
    $slotName = Split-Path $slot -Leaf
    if ($slotName -notin @('service-a', 'service-b')) { throw 'Prepared release has an unknown service slot.' }
    $expectedSlot = Join-Path $InstallRoot "bin\$slotName"
    foreach ($pair in @(@('artifact', 'continuum-core-server.exe'), @('cli', 'continuum.exe'), @('launcher', 'run-service-hidden.ps1'))) {
        Assert-CorePreparedPath -Path $Release.($pair[0]) -Expected (Join-Path $expectedSlot $pair[1]) -File
    }
    $engineSlot = Split-Path (Split-Path $Release.engine -Parent) -Leaf
    if ($engineSlot -notin @('engine-a', 'engine-b', 'engine-c')) { throw 'Prepared release has an unknown engine slot.' }
    Assert-CorePreparedPath -Path $Release.engine -Expected (Join-Path $InstallRoot "bin\$engineSlot\llama-server.exe") -File
    Assert-CorePreparedPath -Path $Release.logDirectory -Expected (Join-Path $InstallRoot 'logs')
    if ($Release.socket -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\)') { throw 'Prepared release socket must be absolute.' }
}

function Save-CorePreparedRelease {
    param($Release, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    Assert-CorePreparedRelease -Release $Release -InstallRoot $InstallRoot
    $path = Join-Path $InstallRoot 'install-prepared.json'
    Assert-CorePreparedPath -Path $path -Expected $path
    $hashes = @{}
    foreach ($field in @('artifact', 'cli', 'launcher', 'engine')) {
        $hashes[$field] = (Get-FileHash -LiteralPath $Release.$field -Algorithm SHA256 -ErrorAction Stop).Hash
    }
    $receipt = @{ schema = 1; userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value;
        release = $Release; hashes = $hashes }
    $temporary = $path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
        if (Test-Path -LiteralPath $path) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}

function Get-CorePreparedRelease {
    param([string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $path = Join-Path $InstallRoot 'install-prepared.json'
    Assert-CorePreparedPath -Path $path -Expected $path
    if (Test-Path -LiteralPath $path) {
        if ((Get-Item -LiteralPath $path).Length -gt 65536) { throw 'Prepared release receipt is oversized.' }
        $receipt = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -ErrorAction Stop
        $receiptFields = @($receipt.PSObject.Properties.Name)
        if ($receiptFields.Count -ne 4 -or @($receiptFields | Where-Object { $_ -notin @('schema', 'userSid', 'release', 'hashes') }).Count -or
            $receipt.schema -ne 1 -or $receipt.userSid -ne $userSid) { throw 'Prepared release receipt schema or owner differs.' }
        $release = $receipt.release
        Assert-CorePreparedRelease -Release $release -InstallRoot $InstallRoot
        if (@($receipt.hashes.PSObject.Properties).Count -ne 4) { throw 'Prepared release receipt has an invalid hash set.' }
        foreach ($field in @('artifact', 'cli', 'launcher', 'engine')) {
            if ($receipt.hashes.$field -isnot [string] -or $receipt.hashes.$field -notmatch '^[0-9a-fA-F]{64}$' -or
                (Get-FileHash -LiteralPath $release.$field -Algorithm SHA256 -ErrorAction Stop).Hash -ne $receipt.hashes.$field) {
                throw "Prepared release $field changed since preparation; refusing resume."
            }
        }
        Write-Step 'Selected the saved prepared release and verified its artifact hashes.'
    } else {
        $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
        if (-not (Test-CoreTaskUser -UserId $task.Principal.UserId -ExpectedSid $userSid)) { throw 'Prepared startup task owner differs or cannot be resolved.' }
        if (-not $task.Description -or $task.Description.Length -gt 65536) { throw 'Prepared startup task has no bounded descriptor.' }
        $release = $task.Description | ConvertFrom-Json -ErrorAction Stop
        Assert-CorePreparedRelease -Release $release -InstallRoot $InstallRoot
        Write-Step 'Selected the existing startup task release. No historical artifact/configuration receipt exists for this older preparation.'
    }
    return $release
}

function Resume-CorePreparedRelease {
    param([string]$RepoRoot, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    $release = Get-CorePreparedRelease -InstallRoot $InstallRoot
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    if ($task -and -not (Test-CoreTaskUser -UserId $task.Principal.UserId -ExpectedSid ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value))) {
        throw 'Existing startup task owner differs or cannot be resolved; refusing resume.'
    }
    $workingDirectory = Split-Path $release.artifact -Parent
    if ($env:GIT_DIR -or $env:GIT_WORK_TREE) { throw 'Prepared resume requires a shell without GIT_DIR/GIT_WORK_TREE overrides.' }
    $cursor = $workingDirectory
    while ($cursor) {
        if (Test-Path -LiteralPath (Join-Path $cursor '.git')) { throw 'Installed release is inside a Git checkout; refusing ambiguous prepared provenance.' }
        $parent = Split-Path $cursor -Parent
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
    Write-Step "Deploying the already-prepared release at $($release.artifact); this does not build or deploy the current checkout."
    # The CLI's preflight prints the selected core's actual build SHA, then its
    # ordinary prebuilt handoff retains leases, old-PID death and SHA checks.
    $oldSocket = $env:CONTINUUM_CORE_SOCKET
    try {
        $env:CONTINUUM_CORE_SOCKET = $release.socket
        Register-CoreServiceRelease -Release $release -RepoRoot $RepoRoot -WorkingDirectory $workingDirectory
        Invoke-CoreServiceRelease -Release $release -RepoRoot $RepoRoot -WorkingDirectory $workingDirectory
    } finally {
        if ($null -eq $oldSocket) { Remove-Item Env:CONTINUUM_CORE_SOCKET -ErrorAction SilentlyContinue }
        else { $env:CONTINUUM_CORE_SOCKET = $oldSocket }
    }
}
