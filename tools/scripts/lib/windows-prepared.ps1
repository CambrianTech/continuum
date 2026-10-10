. (Join-Path $PSScriptRoot 'payload-paths.ps1')
# Canonical image spelling shared by prepared receipts and service inspection.
function ConvertTo-CoreImagePath {
    param([Parameter(Mandatory = $true)][string]$Path)
    # Windows process inspection can report the same image with an extended
    # path prefix while installer paths use the ordinary drive/UNC spelling.
    $normalized = $Path.Replace('/', '\')
    if ($normalized.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) {
        $normalized = '\\' + $normalized.Substring(8)
    } elseif ($normalized.StartsWith('\\?\', [StringComparison]::OrdinalIgnoreCase)) {
        $normalized = $normalized.Substring(4)
    }
    return [IO.Path]::GetFullPath($normalized)
}

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
    $allowed = $fields + @('eyeRoot')
    if (@($fields | Where-Object { $_ -notin $names }).Count -or @($names | Where-Object { $_ -notin $allowed }).Count) {
        throw 'Prepared release descriptor has unexpected or missing fields.'
    }
    foreach ($name in $names) {
        $value = $Release.$name
        if ($value -isnot [string] -or -not $value -or
            $value.IndexOfAny([char[]]@('"', "`r", "`n", [char]0)) -ge 0 -or $value.EndsWith('\')) {
            throw "Prepared release field $name is not a safe absolute argument."
        }
    }
    $slot = Split-Path $Release.artifact -Parent
    $slotName = Split-Path $slot -Leaf
    if ($slotName -notin @('service-a', 'service-b')) { throw 'Prepared release has an unknown service slot.' }
    $expectedSlot = Join-Path (Get-ManagedPayloadRoot -HomeRoot $InstallRoot) "bin\$slotName"
    foreach ($pair in @(@('artifact', 'continuum-core-server.exe'), @('cli', 'continuum.exe'), @('launcher', 'run-service-hidden.ps1'))) {
        Assert-CorePreparedPath -Path $Release.($pair[0]) -Expected (Join-Path $expectedSlot $pair[1]) -File
    }
    $engineSlot = Split-Path (Split-Path $Release.engine -Parent) -Leaf
    if ($engineSlot -notin @('engine-a', 'engine-b', 'engine-c')) { throw 'Prepared release has an unknown engine slot.' }
    Assert-CorePreparedPath -Path $Release.engine -Expected (Join-Path (Get-ManagedPayloadRoot -HomeRoot $InstallRoot) "bin\$engineSlot\llama-server.exe") -File
    Assert-CorePreparedPath -Path $Release.logDirectory -Expected (Join-Path $InstallRoot 'logs')
    if ($Release.socket -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\)') { throw 'Prepared release socket must be absolute.' }
    # Old installed releases have no browser root. New releases carry an explicit
    # source asset root; it is not an installed binary slot or an inferred cwd.
    if ('eyeRoot' -in $names -and $Release.eyeRoot -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+\\)') {
        throw 'Prepared release eyeRoot must be absolute.'
    }
}

function Get-CoreReleaseHashes {
    param($Release)
    $hashes = @{}
    foreach ($field in @('artifact', 'cli', 'launcher', 'engine')) {
        $hashes[$field] = (Get-FileHash -LiteralPath $Release.$field -Algorithm SHA256 -ErrorAction Stop).Hash
    }
    $slot = Split-Path $Release.cli -Parent
    $manifest = Join-Path $slot 'runtime-libs.txt'
    if (Test-Path -LiteralPath $manifest) {
        $hashes['runtime-manifest'] = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256 -ErrorAction Stop).Hash
        foreach ($line in @(Get-Content -LiteralPath $manifest -ErrorAction Stop)) {
            $name = $line.Trim()
            if (-not $name) { continue }
            if ($name -notmatch '^[A-Za-z0-9_.-]+\.dll$' -or $hashes.ContainsKey("runtime:$name")) {
                throw 'Runtime manifest has an unsafe or duplicate library name.'
            }
            Assert-CorePreparedPath -Path (Join-Path $slot $name) -Expected (Join-Path $slot $name) -File
            $hashes["runtime:$name"] = (Get-FileHash -LiteralPath (Join-Path $slot $name) -Algorithm SHA256 -ErrorAction Stop).Hash
        }
    }
    return $hashes
}

# The caller owns install.lock. Reusing an inactive slot supersedes any pending
# preparation naming that slot BEFORE its bytes change; a crash must not leave a
# normal resume believing an old receipt still describes the replacement bytes.
function Clear-CorePreparedSelectionForSlot {
    param([string]$InstallRoot, [string]$Slot)
    $path = Join-Path $InstallRoot 'install-prepared.json'
    Assert-CorePreparedPath -Path $path -Expected $path
    if (-not (Test-Path -LiteralPath $path)) { return }
    if ((Get-Item -LiteralPath $path).Length -gt 65536) { throw 'Pending preparation is oversized.' }
    $receipt = Get-Content -LiteralPath $path -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    if ($receipt.schema -ne 1 -or $receipt.userSid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) { throw 'Pending preparation owner/schema differs.' }
    if ((ConvertTo-CoreImagePath (Split-Path $receipt.release.artifact -Parent)) -eq (ConvertTo-CoreImagePath $Slot)) {
        Remove-Item -LiteralPath $path -ErrorAction Stop
    }
}

function Save-CorePreparedRelease {
    param($Release, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'),
        [ValidateSet('Prepared', 'Active')][string]$Selection = 'Prepared')
    Assert-CorePreparedRelease -Release $Release -InstallRoot $InstallRoot
    $path = Join-Path $InstallRoot $(switch ($Selection) { Active { 'install-active.json' } Previous { 'install-previous.json' } default { 'install-prepared.json' } })
    Assert-CorePreparedPath -Path $path -Expected $path
    $hashes = Get-CoreReleaseHashes -Release $Release
    $receipt = @{ schema = 1; userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value;
        release = $Release; hashes = $hashes }
    $temporary = $path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, ($receipt | ConvertTo-Json -Depth 5), [Text.UTF8Encoding]::new($false))
        if (Test-Path -LiteralPath $path) {
            $backup = [NullString]::Value
            if ($Selection -eq 'Active') {
                $previous = Get-CorePreparedRelease -InstallRoot $InstallRoot -Selection Active
                if (($previous | ConvertTo-Json -Compress) -cne ($Release | ConvertTo-Json -Compress)) {
                    $backup = Join-Path $InstallRoot 'install-previous.json'
                    Assert-CorePreparedPath -Path $backup -Expected $backup
                }
            }
            [IO.File]::Replace($temporary, $path, $backup)
        }
        else { [IO.File]::Move($temporary, $path) }
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force } }
}

function Read-CoreReleaseReceipt {
    param([string]$InstallRoot, [ValidateSet('Prepared', 'Active', 'Previous')][string]$Selection,
        [switch]$RecognizeDamagedLegacyPrevious)
    $path = Join-Path $InstallRoot ("install-{0}.json" -f $Selection.ToLowerInvariant())
    Assert-CorePreparedPath -Path $path -Expected $path -File
    if ((Get-Item -LiteralPath $path).Length -gt 65536) { throw 'Prepared release receipt is oversized.' }
    $bytes = [IO.File]::ReadAllBytes($path)
    $receipt = [Text.Encoding]::UTF8.GetString($bytes).TrimStart([char]0xfeff) | ConvertFrom-Json -ErrorAction Stop
    $names = @($receipt.PSObject.Properties.Name)
    if ($names.Count -ne 4 -or @($names | Where-Object { $_ -notin @('schema', 'userSid', 'release', 'hashes') }).Count -or
        $receipt.schema -ne 1 -or $receipt.userSid -ne [Security.Principal.WindowsIdentity]::GetCurrent().User.Value) {
        throw 'Prepared release receipt schema or owner differs.'
    }
    Assert-CorePreparedRelease -Release $receipt.release -InstallRoot $InstallRoot
    $actual = Get-CoreReleaseHashes -Release $receipt.release
    $legacyPending = $Selection -eq 'Prepared' -and @($receipt.hashes.PSObject.Properties).Count -eq 4
    # Historical Previous may predate DLL sealing. This is diagnostic recovery
    # only: it cannot become a valid rollback, and must prove engine-only damage.
    $legacyPrevious = $RecognizeDamagedLegacyPrevious -and $Selection -eq 'Previous' -and
        @($receipt.hashes.PSObject.Properties).Count -eq 4 -and $actual.Count -gt 4
    $fields = if ($legacyPending -or $legacyPrevious) { @('artifact', 'cli', 'launcher', 'engine') } else { @($actual.Keys) }
    if (@($receipt.hashes.PSObject.Properties).Count -ne $fields.Count) { throw 'Prepared release receipt has an invalid hash set.' }
    $changed = @()
    foreach ($field in $fields) {
        if ($receipt.hashes.$field -isnot [string] -or $receipt.hashes.$field -notmatch '^[0-9a-fA-F]{64}$') {
            throw 'Prepared release receipt has an invalid hash value.'
        }
        if ($actual[$field] -ne $receipt.hashes.$field) { $changed += $field }
    }
    if ($legacyPrevious -and ($changed.Count -ne 1 -or $changed[0] -ne 'engine')) {
        throw 'Legacy Previous lacks complete sealing and is not engine-only damaged; refusing recovery.'
    }
    return [pscustomobject]@{ Path = $path; Bytes = $bytes; Receipt = $receipt; Actual = $actual; Changed = $changed }
}

function Get-CorePreparedRelease {
    param([string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'),
        [ValidateSet('Prepared', 'Active', 'Previous')][string]$Selection = 'Prepared')
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $path = Join-Path $InstallRoot $(switch ($Selection) { Active { 'install-active.json' } Previous { 'install-previous.json' } default { 'install-prepared.json' } })
    Assert-CorePreparedPath -Path $path -Expected $path
    if (Test-Path -LiteralPath $path) {
        $snapshot = Read-CoreReleaseReceipt -InstallRoot $InstallRoot -Selection $Selection
        if ($snapshot.Changed.Count) { throw "$Selection release $($snapshot.Changed -join ', ') changed since preparation; refusing resume." }
        $release = $snapshot.Receipt.release
        Write-Step 'Selected the saved prepared release and verified its artifact hashes.'
    } else {
        if ($Selection -ne 'Prepared') { throw 'Provisioned supervisor has no committed release receipt; refusing legacy fallback.' }
        $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
        if (-not (Test-CoreTaskUser -UserId $task.Principal.UserId -ExpectedSid $userSid)) { throw 'Prepared startup task owner differs or cannot be resolved.' }
        if (-not $task.Description -or $task.Description.Length -gt 65536) { throw 'Prepared startup task has no bounded descriptor.' }
        $release = Get-CoreRegisteredRelease -Task $task -InstallRoot $InstallRoot
        Assert-CorePreparedRelease -Release $release -InstallRoot $InstallRoot
        Write-Step 'Selected the existing startup task release. No historical artifact/configuration receipt exists for this older preparation.'
    }
    return $release
}

# A failed old installer could reuse an engine still sealed by Active/Previous.
# Diagnose that one shape; malformed receipts and all other corruption still fail.
# This is not a claim that a release never served, or permission to reseal it.
function Get-CoreDamagedSelectionRecovery {
    param([string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    if (-not (Test-Path -LiteralPath (Join-Path $InstallRoot 'install-active.json'))) { return $null }
    $active = Read-CoreReleaseReceipt -InstallRoot $InstallRoot -Selection Active
    if (-not $active.Changed.Count) { return $null }
    $prepared = Read-CoreReleaseReceipt -InstallRoot $InstallRoot -Selection Prepared
    if ($prepared.Changed.Count -or @($prepared.Receipt.hashes.PSObject.Properties).Count -ne $prepared.Actual.Count) {
        throw 'Damaged selection recovery requires a complete, unchanged prepared release.'
    }
    $previous = $null
    if (Test-Path -LiteralPath (Join-Path $InstallRoot 'install-previous.json')) {
        $previous = Read-CoreReleaseReceipt -InstallRoot $InstallRoot -Selection Previous -RecognizeDamagedLegacyPrevious
    }
    foreach ($snapshot in @($active, $previous)) {
        if ($null -eq $snapshot -or -not $snapshot.Changed.Count) { continue }
        if ($snapshot.Changed.Count -ne 1 -or $snapshot.Changed[0] -ne 'engine' -or
            (ConvertTo-CoreImagePath $snapshot.Receipt.release.engine) -ne (ConvertTo-CoreImagePath $prepared.Receipt.release.engine) -or
            $snapshot.Actual.engine -ne $prepared.Receipt.hashes.engine) {
            throw 'Damaged selection differs beyond the prepared engine replacement; refusing recovery.'
        }
    }
    return [pscustomobject]@{ Active = $active; Previous = $previous; Prepared = $prepared }
}

function Assert-CoreRecoveryStopped {
    param([string]$InstallRoot)
    foreach ($name in @('ContinuumCore', 'ContinuumDeploy')) {
        $task = Get-ScheduledTask -TaskName $name -TaskPath '\' -ErrorAction Stop
        if ($task.State -notin @('Ready', 'Disabled')) { throw "Recovery refuses a running, queued or unknown $name task." }
        if ($name -eq 'ContinuumCore' -and -not (Test-CoreProvisionedTask -Task $task -InstallRoot $InstallRoot)) {
            throw 'Damaged selection recovery requires a verified fixed supervisor.'
        }
    }
    if (@(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.Name -like 'continuum-core-server*.exe' }).Count) {
        throw 'Damaged selection recovery refuses an existing core process.'
    }
}

function Save-CoreRecoveryEvidence {
    param($Snapshot, [string]$InstallRoot)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $digest = [BitConverter]::ToString($sha.ComputeHash($Snapshot.Bytes)).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
    $path = Join-Path $InstallRoot ("{0}.damaged-{1}.json" -f [IO.Path]::GetFileNameWithoutExtension($Snapshot.Path), $digest)
    Assert-CorePreparedPath -Path $path -Expected $path
    if (Test-Path -LiteralPath $path) {
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $digest) { throw 'Recovery evidence differs; refusing replacement.' }
        return
    }
    $temporary = $path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.Write($Snapshot.Bytes, 0, $Snapshot.Bytes.Length); $stream.Flush($true) }
        finally { $stream.Dispose() }
        [IO.File]::Move($temporary, $path)
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -ErrorAction Stop } }
}

# Called only at the normal registration commit point, after bootstrap validation.
# The caller retains install.lock. Old Active cannot pass the supervisor's full
# hash validation; a scheduler race AFTER the atomic switch may start the valid
# candidate. The lease alone does not exclude scheduler starts.
function Complete-CoreDamagedSelectionRecovery {
    param($Plan, $Release, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    Assert-CoreRecoveryStopped -InstallRoot $InstallRoot
    $registration = (Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop).Description
    $current = Get-CoreDamagedSelectionRecovery -InstallRoot $InstallRoot
    if (-not $current -or ($Release | ConvertTo-Json -Compress) -cne ($current.Prepared.Receipt.release | ConvertTo-Json -Compress)) {
        throw 'Recovery candidate changed; refusing selection.'
    }
    foreach ($name in @('Active', 'Previous', 'Prepared')) {
        if (($null -eq $Plan.$name) -ne ($null -eq $current.$name) -or
            ($null -ne $Plan.$name -and [Convert]::ToBase64String($Plan.$name.Bytes) -cne [Convert]::ToBase64String($current.$name.Bytes))) {
            throw 'Recovery receipts changed; refusing to replace a newer selection.'
        }
    }
    Save-CoreRecoveryEvidence -Snapshot $current.Active -InstallRoot $InstallRoot
    if ($current.Previous -and $current.Previous.Changed.Count) {
        Save-CoreRecoveryEvidence -Snapshot $current.Previous -InstallRoot $InstallRoot
    }
    # Recheck after evidence I/O, before removing rollback eligibility or selecting.
    Assert-CoreRecoveryStopped -InstallRoot $InstallRoot
    foreach ($name in @('Active', 'Previous', 'Prepared')) {
        $snapshot = $current.$name
        if ($snapshot -and [Convert]::ToBase64String([IO.File]::ReadAllBytes($snapshot.Path)) -cne [Convert]::ToBase64String($snapshot.Bytes)) {
            throw 'Recovery receipts changed during archival; refusing selection.'
        }
    }
    $temporary = $current.Active.Path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    $pins = [Collections.Generic.List[IDisposable]]::new()
    try {
        foreach ($field in $current.Prepared.Actual.Keys) {
            $file = if ($field -eq 'runtime-manifest') { Join-Path (Split-Path $Release.cli -Parent) 'runtime-libs.txt' }
                elseif ($field.StartsWith('runtime:')) { Join-Path (Split-Path $Release.cli -Parent) $field.Substring(8) }
                else { $Release.$field }
            $pins.Add([IO.File]::Open($file, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read))
        }
        $last = Get-CoreDamagedSelectionRecovery -InstallRoot $InstallRoot
        if (-not $last -or [Convert]::ToBase64String($last.Prepared.Bytes) -cne [Convert]::ToBase64String($current.Prepared.Bytes) -or
            [Convert]::ToBase64String($last.Active.Bytes) -cne [Convert]::ToBase64String($current.Active.Bytes) -or
            ($null -eq $last.Previous) -ne ($null -eq $current.Previous) -or
            ($last.Previous -and [Convert]::ToBase64String($last.Previous.Bytes) -cne [Convert]::ToBase64String($current.Previous.Bytes))) {
            throw 'Recovery receipts changed before commit; refusing selection.'
        }
        Assert-CoreRecoveryStopped -InstallRoot $InstallRoot
        if ((Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop).Description -cne $registration) {
            throw 'Supervisor registration changed during recovery; refusing selection.'
        }
        $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $stream.Write($current.Prepared.Bytes, 0, $current.Prepared.Bytes.Length); $stream.Flush($true) }
        finally { $stream.Dispose() }
        if ($current.Previous -and $current.Previous.Changed.Count) {
            # Already durably archived; leaving it in the rollback namespace would
            # falsely offer a release whose engine no longer exists.
            [IO.File]::Delete($current.Previous.Path)
        }
        [IO.File]::Replace($temporary, $current.Active.Path, [NullString]::Value)
    } finally {
        foreach ($pin in $pins) { $pin.Dispose() }
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -ErrorAction Stop }
    }
    Write-Step 'Recovered the verified prepared selection; damaged receipts were retained as evidence, not rollback candidates.'
}

# Recovery is a compare-and-restore under the same installation lease. It never
# changes task registration and never overrides a newer installer's selection.
function Restore-CoreActiveRelease {
    param([string]$ExpectedDescription, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if (-not (Test-CoreProvisionedTask -Task $task -InstallRoot $InstallRoot)) { throw 'Recovery requires the provisioned supervisor.' }
    if ($task.State -eq 'Running') { throw 'Recovery refuses to replace a running supervisor selection.' }
    $current = Get-CorePreparedRelease -InstallRoot $InstallRoot -Selection Active
    if (($current | ConvertTo-Json -Compress) -cne $ExpectedDescription) { throw 'Active release changed; refusing to replace a newer selection.' }
    $previous = Get-CorePreparedRelease -InstallRoot $InstallRoot -Selection Previous
    $activePath = Join-Path $InstallRoot 'install-active.json'
    $previousPath = Join-Path $InstallRoot 'install-previous.json'
    $temporary = $activePath + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllBytes($temporary, [IO.File]::ReadAllBytes($previousPath))
        [IO.File]::Replace($temporary, $activePath, [NullString]::Value)
    } finally { if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -ErrorAction Stop } }
    return $previous
}

function Initialize-CoreActiveSelection {
    param($Task, $Release, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    if (Test-Path -LiteralPath (Join-Path $InstallRoot 'install-active.json')) {
        Get-CorePreparedRelease -InstallRoot $InstallRoot -Selection Active | Out-Null
        return $false
    }
    if ($Task) {
        Save-CorePreparedRelease -Release (Get-CoreRegisteredRelease -Task $Task -InstallRoot $InstallRoot) -InstallRoot $InstallRoot -Selection Active
        return $false
    }
    Save-CorePreparedRelease -Release $Release -InstallRoot $InstallRoot -Selection Active
    return $true
}

# The supervisor's fixed registration describes its authority, while this receipt
# describes the selected release. Pending preparation never changes boot selection.
function Get-CoreRegisteredRelease {
    param($Task, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    if (-not $Task.Description -or $Task.Description.Length -gt 65536) { throw 'Startup task has no bounded descriptor.' }
    $descriptor = $Task.Description | ConvertFrom-Json -ErrorAction Stop
    if ($descriptor.schema -eq 2) {
        if (-not (Test-CoreProvisionedTask -Task $Task -InstallRoot $InstallRoot)) { throw 'Invalid fixed supervisor registration.' }
        Assert-CorePreparedPath -Path $descriptor.activeRelease -Expected (Join-Path $InstallRoot 'install-active.json') -File
        return Get-CorePreparedRelease -InstallRoot $InstallRoot -Selection Active
    }
    if ($descriptor.PSObject.Properties.Name -contains 'schema') { throw 'Unknown supervisor provision schema.' }
    return $descriptor
}

function Get-CoreSupervisorBootstrap {
    param([string]$UserSid = ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value), [string]$Generation = '')
    $canonical = [Security.Principal.SecurityIdentifier]::new($UserSid).Value
    if ($canonical -cne $UserSid) { throw 'Supervisor principal is not a canonical SID.' }
    if ($Generation -and $Generation -cnotmatch '^[0-9a-f]{64}$') { throw 'Invalid protected bootstrap generation.' }
    $directory = if ($Generation) { 'supervisor-' + $Generation } else { 'supervisor' }
    return Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)) "Continuum\$UserSid\$directory\continuum.exe"
}

function Assert-CoreSupervisorLocation {
    param([string]$Path, [string]$UserSid = ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value))
    $name = Split-Path (Split-Path $Path -Parent) -Leaf
    $generation = if ($name -ceq 'supervisor') { '' } elseif ($name -cmatch '^supervisor-([0-9a-f]{64})$') { $Matches[1] } else { throw 'Unknown protected bootstrap generation.' }
    if ($Path -cne (Get-CoreSupervisorBootstrap -UserSid $UserSid -Generation $generation)) { throw 'Unexpected supervisor bootstrap path.' }
}

# Called only by the installer registrar under its initial consent. Executable
# selection happens after token lowering, but loader code runs before main: the
# bootstrap and its DLL closure must therefore never be caller-writable.
function Install-CoreSupervisorBootstrap {
    param($Plan)
    $expected = $Plan.cli
    Assert-CoreSupervisorLocation -Path $expected -UserSid $Plan.userSid
    if ($Plan.cli -cne $expected -or $Plan.shell -cne $expected) { throw 'Bootstrap destination differs from its protected installation boundary.' }
    Assert-CorePreparedPath -Path $expected -Expected $expected
    if (Test-Path -LiteralPath $expected) {
        # The bootstrap is a stable authority boundary, not a rotating payload.
        # Repairing task drift must not replace DLLs under its running image.
        Assert-CoreSupervisorBootstrap -Path $expected -UserSid $Plan.userSid
        $protocol = (Invoke-InstallerProcess $expected @('installed-service', '--protocol') | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $protocol -cne '3') { throw 'Existing protected bootstrap has an incompatible protocol.' }
        return
    }
    if ($expected -cne (Get-CoreSupervisorBootstrap -UserSid $Plan.userSid -Generation ([string]$Plan.bootstrapHashes.cli).ToLowerInvariant())) { throw 'New bootstrap generation differs from candidate identity.' }
    $programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
    $finalDirectory = Split-Path $expected -Parent
    $stagingName = 'supervisor.prepare-' + [guid]::NewGuid().ToString('N')
    $directory = Join-Path (Split-Path $finalDirectory -Parent) $stagingName
    $acl = [Security.AccessControl.DirectorySecurity]::new()
    $acl.SetAccessRuleProtection($true, $false)
    $acl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    foreach ($sid in @('S-1-5-18', 'S-1-5-32-544')) {
        $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
            [Security.Principal.SecurityIdentifier]::new($sid), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    }
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
        [Security.Principal.SecurityIdentifier]::new($Plan.userSid), 'ReadAndExecute', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    $cursor = $programFiles
    foreach ($component in @('Continuum', $Plan.userSid, $stagingName)) {
        $cursor = Join-Path $cursor $component
        if (-not (Test-Path -LiteralPath $cursor)) { [IO.Directory]::CreateDirectory($cursor) | Out-Null }
        Assert-CorePreparedPath -Path $cursor -Expected $cursor
        Set-Acl -LiteralPath $cursor -AclObject $acl -ErrorAction Stop
    }
    $source = Split-Path $Plan.bootstrapSource -Parent
    $files = @{ 'continuum.exe' = $Plan.bootstrapHashes.cli }
    foreach ($property in $Plan.bootstrapHashes.PSObject.Properties) {
        if ($property.Name -eq 'runtime-manifest') { $files['runtime-libs.txt'] = $property.Value }
        elseif ($property.Name.StartsWith('runtime:')) {
            $name = $property.Name.Substring(8)
            if ($name -notmatch '^[A-Za-z0-9_.-]+\.dll$') { throw 'Unsafe bootstrap runtime library.' }
            $files[$name] = $property.Value
        }
    }
    foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)) {
        if ($item.PSIsContainer -or ($item.Name -ne 'bootstrap-hashes.json' -and -not $files.ContainsKey($item.Name))) {
            throw 'Protected bootstrap directory contains undeclared content; refusing to overwrite it.'
        }
    }
    Assert-CorePreparedPath -Path (Join-Path $directory 'bootstrap-hashes.json') -Expected (Join-Path $directory 'bootstrap-hashes.json')
    foreach ($name in $files.Keys) {
        $from = Join-Path $source $name
        if ($files[$name] -notmatch '^[0-9a-fA-F]{64}$' -or
            (Get-FileHash -LiteralPath $from -Algorithm SHA256 -ErrorAction Stop).Hash -ne $files[$name]) { throw 'Bootstrap input changed after preparation.' }
        $to = Join-Path $directory $name
        Assert-CorePreparedPath -Path $to -Expected $to
        Copy-Item -LiteralPath $from -Destination $to -Force -ErrorAction Stop
        if ((Get-FileHash -LiteralPath $to -Algorithm SHA256 -ErrorAction Stop).Hash -ne $files[$name]) { throw 'Protected bootstrap copy did not verify.' }
        # Existing files can retain a former explicit ACL across overwrite.
        $fileAcl = [Security.AccessControl.FileSecurity]::new()
        $fileAcl.SetAccessRuleProtection($false, $false)
        $fileAcl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
        Set-Acl -LiteralPath $to -AclObject $fileAcl -ErrorAction Stop
    }
    $manifestPath = Join-Path $directory 'bootstrap-hashes.json'
    Assert-CorePreparedPath -Path $manifestPath -Expected $manifestPath
    [IO.File]::WriteAllText($manifestPath, ($files | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
    Set-Acl -LiteralPath (Join-Path $directory 'bootstrap-hashes.json') -AclObject $fileAcl -ErrorAction Stop
    Assert-CoreSupervisorBootstrap -Path (Join-Path $directory 'continuum.exe') -UserSid $Plan.userSid -Staged
    # No task references the staged path. A failed copy/verification leaves the
    # fixed target untouched; only a fully verified closure becomes executable.
    if (Test-Path -LiteralPath $finalDirectory) { throw 'Bootstrap target appeared during preparation; protected selection was preserved.' }
    [IO.Directory]::Move($directory, $finalDirectory)
    Assert-CoreSupervisorBootstrap -Path $expected -UserSid $Plan.userSid
}

function Assert-CoreSupervisorBootstrap {
    param([string]$Path = (Get-CoreSupervisorBootstrap),
        [string]$UserSid = ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value), [switch]$Staged)
    $expected = Get-CoreSupervisorBootstrap -UserSid $UserSid
    if ($Staged) {
        if ((Split-Path $Path -Leaf) -cne 'continuum.exe' -or
            (Split-Path (Split-Path $Path -Parent) -Leaf) -notmatch '^supervisor\.prepare-[0-9a-f]{32}$' -or
            (Split-Path (Split-Path $Path -Parent) -Parent) -cne (Split-Path (Split-Path $expected -Parent) -Parent)) { throw 'Unexpected staged bootstrap path.' }
    } else { Assert-CoreSupervisorLocation -Path $Path -UserSid $UserSid }
    Assert-CorePreparedPath -Path $Path -Expected $Path -File
    $directory = Split-Path $Path -Parent
    $manifest = Join-Path $directory 'bootstrap-hashes.json'
    Assert-CorePreparedPath -Path $manifest -Expected $manifest -File
    if ((Get-Item -LiteralPath $manifest -ErrorAction Stop).Length -gt 65536) { throw 'Bootstrap integrity receipt is oversized.' }
    $hashes = Get-Content -LiteralPath $manifest -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    if (-not $hashes.'continuum.exe') { throw 'Bootstrap integrity receipt has no executable.' }
    $generationName = Split-Path $directory -Leaf
    if (-not $Staged -and $generationName -cmatch '^supervisor-([0-9a-f]{64})$' -and $Matches[1] -cne ([string]$hashes.'continuum.exe').ToLowerInvariant()) { throw 'Protected bootstrap generation identity differs from its manifest.' }
    foreach ($item in @(Get-ChildItem -LiteralPath $directory -Force -ErrorAction Stop)) {
        if ($item.PSIsContainer -or ($item.Name -ne 'bootstrap-hashes.json' -and $item.Name -notin @($hashes.PSObject.Properties.Name))) {
            throw 'Protected bootstrap directory contains undeclared content.'
        }
    }
    $protected = @($directory, (Split-Path $directory -Parent), (Split-Path (Split-Path $directory -Parent) -Parent),
        [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles), $manifest)
    foreach ($p in $hashes.PSObject.Properties) {
        if ($p.Name -ne 'continuum.exe' -and $p.Name -ne 'runtime-libs.txt' -and $p.Name -notmatch '^[A-Za-z0-9_.-]+\.dll$') { throw 'Unsafe bootstrap integrity filename.' }
        $file = Join-Path $directory $p.Name
        Assert-CorePreparedPath -Path $file -Expected $file -File
        if ($p.Value -notmatch '^[0-9a-fA-F]{64}$' -or (Get-FileHash -LiteralPath $file -Algorithm SHA256 -ErrorAction Stop).Hash -ne $p.Value) { throw 'Protected bootstrap integrity changed.' }
        $protected += $file
    }
    foreach ($item in $protected) {
        Assert-CoreBootstrapAccess -Security (Get-Acl -LiteralPath $item -ErrorAction Stop)
    }
}

function Assert-CoreBootstrapAccess {
    param([Security.AccessControl.FileSystemSecurity]$Security)
    $trusted = @('S-1-5-18', 'S-1-5-32-544', 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464') # System, administrators, Windows TrustedInstaller
    if ($Security.GetOwner([Security.Principal.SecurityIdentifier]).Value -notin $trusted) { throw 'Supervisor loader boundary has a non-administrative owner.' }
    foreach ($ace in $Security.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
        if ($ace.AccessControlType -eq 'Allow' -and
            ([int]$ace.PropagationFlags -band [int][Security.AccessControl.PropagationFlags]::InheritOnly) -eq 0 -and
            ([long]$ace.FileSystemRights -band 0xD0156) -ne 0 -and
            $ace.IdentityReference.Value -notin $trusted) { throw 'Supervisor loader boundary is writable outside administrators/System.' }
    }
}

function Test-CoreProvisionedTask {
    param($Task, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'))
    try { $d = $Task.Description | ConvertFrom-Json -ErrorAction Stop }
    catch {
        if ($Task.Description -match '"schema"') { throw 'Malformed supervisor provision descriptor.' }
        return $false # pre-descriptor Bash supervisor migration
    }
    try {
        if ($d.schema -ne 2) { return $false }
        if (@($d.PSObject.Properties).Count -ne 3 -or
            $d.activeRelease -cne (Join-Path $InstallRoot 'install-active.json')) { throw 'Supervisor provision descriptor differs from the installed boundary.' }
        Assert-CoreSupervisorLocation -Path $d.bootstrap
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        if (-not (Test-CoreTaskUser -UserId $Task.Principal.UserId -ExpectedSid $sid) -or
            @($Task.Actions).Count -ne 1 -or $Task.Actions[0].Execute -cne $d.bootstrap -or
            $Task.Actions[0].WorkingDirectory -cne (Split-Path $d.bootstrap -Parent) -or
            $Task.Actions[0].Arguments -cne ('installed-service core "{0}"' -f $d.activeRelease)) {
            throw 'Fixed supervisor action or principal differs from its descriptor.'
        }
        return $true
    } catch { throw "Cannot inspect supervisor provision: $_" }
}

function Resume-CorePreparedRelease {
    param([string]$RepoRoot, [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'), [IDisposable]$InstallLease)
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
        Invoke-CoreServiceRelease -Release $release -RepoRoot $RepoRoot -WorkingDirectory $workingDirectory -InstallLease $InstallLease
    } finally {
        if ($null -eq $oldSocket) { Remove-Item Env:CONTINUUM_CORE_SOCKET -ErrorAction SilentlyContinue }
        else { $env:CONTINUUM_CORE_SOCKET = $oldSocket }
    }
}
