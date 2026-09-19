# Native installer lifecycle. Two installed slots bound disk usage and keep
# running images out of Cargo's output directory. Never overwrite an active slot.
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

function Test-CoreTaskUser {
    param([string]$UserId, [string]$ExpectedSid)
    if (-not $UserId) { return $false }
    try {
        # Scheduler CIM projections may turn a registered SID into an account
        # name. Compare identities, not their provider-specific spelling.
        $actual = if ($UserId.StartsWith('S-', [StringComparison]::OrdinalIgnoreCase)) {
            [Security.Principal.SecurityIdentifier]::new($UserId)
        } else {
            ([Security.Principal.NTAccount]::new($UserId)).Translate([Security.Principal.SecurityIdentifier])
        }
        return $actual.Equals([Security.Principal.SecurityIdentifier]::new($ExpectedSid))
    } catch { return $false } # Unresolvable identities never authorize a task.
}

# Task Scheduler can return inherited ACEs before its explicit principal ACE.
# Preserve that order: CommonSecurityDescriptor.AddAccess rejects this shape.
# This deliberately supports only ordinary allow/deny ACLs. Without the original
# caller token, overlapping denies (even for another SID) cannot prove access;
# refuse them rather than sorting, clearing policy, or claiming an effective grant.
function Get-CoreServiceSecurityDescriptor {
    param([Parameter(Mandatory = $true)][string]$Sddl)
    $security = [Security.AccessControl.RawSecurityDescriptor]::new($Sddl)
    if ($null -eq $security.DiscretionaryAcl) { throw 'Unsupported startup task ACL: no explicit DACL.' }
    foreach ($ace in $security.DiscretionaryAcl) {
        if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback -or
            $ace.AceQualifier -notin @([Security.AccessControl.AceQualifier]::AccessAllowed,
                [Security.AccessControl.AceQualifier]::AccessDenied)) {
            throw 'Unsupported startup task ACL: conditional/object or non-access ACE; no ACL changes made.'
        }
        # Include generic rights: their mapped masks can overlap FRFX.
        if ($ace.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessDenied -and
            (([long]$ace.AccessMask -band (4026531840L -bor 0x1200a9)) -ne 0)) {
            throw 'Unsupported startup task ACL: deny ACE overlaps caller read/execute; no ACL changes made.'
        }
    }
    return $security
}

function Test-CoreServiceCallerAccess {
    param([Parameter(Mandatory = $true)][string]$Sddl,
        [Parameter(Mandatory = $true)][string]$UserSid)
    $security = Get-CoreServiceSecurityDescriptor -Sddl $Sddl
    return @($security.DiscretionaryAcl | Where-Object {
        $_.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and
        ([int]$_.AceFlags -band [int][Security.AccessControl.AceFlags]::InheritOnly) -eq 0 -and
        $_.SecurityIdentifier.Value -eq $UserSid -and ($_.AccessMask -band 0x1200a9) -eq 0x1200a9
    }).Count -gt 0
}

function Grant-CoreServiceCallerAccess {
    param([Parameter(Mandatory = $true)][string]$Sddl,
        [Parameter(Mandatory = $true)][string]$UserSid)
    $security = Get-CoreServiceSecurityDescriptor -Sddl $Sddl
    $caller = [Security.Principal.SecurityIdentifier]::new($UserSid)
    if (-not (Test-CoreServiceCallerAccess -Sddl $Sddl -UserSid $UserSid)) {
        $found = $false
        for ($i = 0; $i -lt $security.DiscretionaryAcl.Count; $i++) {
            $ace = $security.DiscretionaryAcl[$i]
            if ($ace.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and
                $ace.AceFlags -eq [Security.AccessControl.AceFlags]::None -and
                $ace.SecurityIdentifier -eq $caller) {
                $ace.AccessMask = $ace.AccessMask -bor 0x1200a9
                $security.DiscretionaryAcl[$i] = $ace
                $found = $true
                break
            }
        }
        if (-not $found) {
            $security.DiscretionaryAcl.InsertAce($security.DiscretionaryAcl.Count,
                [Security.AccessControl.CommonAce]::new([Security.AccessControl.AceFlags]::None,
                    [Security.AccessControl.AceQualifier]::AccessAllowed, 0x1200a9, $caller, $false, $null))
        }
    }
    return $security.GetSddlForm([Security.AccessControl.AccessControlSections]::Access)
}

function Protect-CoreBuildOutput {
    param([Parameter(Mandatory = $true)][string]$TargetDirectory)
    $TargetDirectory = ConvertTo-CoreImagePath $TargetDirectory
    $artifact = Join-Path $TargetDirectory 'release\continuum-core-server.exe'
    if (-not (Test-Path -LiteralPath $artifact)) { return }
    $processes = @(Get-CimInstance Win32_Process -ErrorAction Stop |
        Where-Object { $_.Name -eq 'continuum-core-server.exe' })
    if (@($processes | Where-Object { -not $_.ExecutablePath }).Count) {
        throw 'Cannot inspect running core image paths before building.'
    }
    if (-not @($processes | Where-Object { (ConvertTo-CoreImagePath $_.ExecutablePath) -eq $artifact }).Count) { return }
    # CIM keeps the original image path after a rename. A later retry may see
    # that stale path while the newly linked output is already writable.
    try {
        $probe = [IO.File]::Open($artifact, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $probe.Dispose()
        return
    } catch [IO.IOException] {
        # The mapped output still needs preservation below.
    }
    # Older starts ran straight from Cargo output. Windows allows a mapped
    # executable to be renamed, but the linker cannot overwrite it. Keep that
    # image alive under a bounded sibling name while Cargo writes its replacement.
    $previous = Join-Path $TargetDirectory 'release\continuum-core-server.previous.exe'
    if (@(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object {
        $_.Name -eq 'continuum-core-server.previous.exe' -or
        ($_.ExecutablePath -and (ConvertTo-CoreImagePath $_.ExecutablePath) -eq $previous)
    }).Count) { throw 'A previous Cargo core image is still running; refusing to replace it.' }
    if (Test-Path -LiteralPath $previous) { Remove-Item -LiteralPath $previous -Force -ErrorAction Stop }
    Move-Item -LiteralPath $artifact -Destination $previous -ErrorAction Stop
    Write-Output 'Preserved the running Cargo image; the build can link its replacement without stopping the core.'
}

function New-CoreServiceRelease {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'),
        [string]$TargetDirectory = $env:CARGO_TARGET_DIR
    )
    $root = ConvertTo-CoreImagePath (Join-Path $InstallRoot 'bin')
    $liveProcesses = @(Get-CimInstance Win32_Process -ErrorAction Stop |
        Where-Object { $_.Name -in @('continuum.exe', 'continuum-core-server.exe') })
    if (@($liveProcesses | Where-Object { -not $_.ExecutablePath }).Count) {
        throw 'Cannot inspect all live Continuum image paths; refusing to overwrite an installed slot.'
    }
    $liveImages = @($liveProcesses | ForEach-Object { ConvertTo-CoreImagePath $_.ExecutablePath })
    $descriptor = $null
    $registered = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    if ($registered) {
        $descriptor = $null
        try { $descriptor = $registered.Description | ConvertFrom-Json -ErrorAction Stop }
        catch {
            # Legacy Bash tasks launch the canonical bin/core-service.sh, outside
            # these slots. They can migrate without deleting their old files.
            if (($registered.Actions.Arguments -join ' ') -match 'service-[ab][\\/]') {
                throw 'Existing ContinuumCore task references a release slot without a readable descriptor; refusing to overwrite its installed files.'
            }
        }
        if ($descriptor.artifact) {
            # Preserve rollback/startup files even while the service is stopped.
            $liveImages += ConvertTo-CoreImagePath $descriptor.artifact
        } elseif (($registered.Actions.Arguments -join ' ') -match 'service-[ab][\\/]') {
            throw 'Existing ContinuumCore task references a release slot without an artifact descriptor.'
        }
    }
    $slot = $null
    foreach ($name in @('service-a', 'service-b')) {
        $candidate = Join-Path $root $name
        $occupied = @($liveImages | Where-Object { $_.StartsWith($candidate + '\', [StringComparison]::OrdinalIgnoreCase) })
        if ($occupied.Count -eq 0) { $slot = $candidate; break }
    }
    if (-not $slot) { throw 'Both installed core service slots are in use; resolve the extra live instance before updating.' }
    # Engines have an independent lifetime: a warm lane can outlive its core.
    # Keep room for that mapped engine, the registered release, and a candidate.
    $engines = @(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.Name -eq 'llama-server.exe' })
    if (@($engines | Where-Object { -not $_.ExecutablePath }).Count) { throw 'Cannot inspect running inference engine paths.' }
    $enginePaths = @($engines | ForEach-Object { ConvertTo-CoreImagePath $_.ExecutablePath })
    if ($descriptor.engine) { $enginePaths += ConvertTo-CoreImagePath $descriptor.engine }
    $engineSlot = $null
    foreach ($name in @('engine-a', 'engine-b', 'engine-c')) {
        $candidate = Join-Path $root $name
        if (-not @($enginePaths | Where-Object { $_.StartsWith($candidate + '\', [StringComparison]::OrdinalIgnoreCase) }).Count) {
            $engineSlot = $candidate; break
        }
    }
    if (-not $engineSlot) { throw 'All installed engine slots are live or registered; refusing to overwrite an inference engine.' }
    New-Item -ItemType Directory -Force -Path $slot | Out-Null
    foreach ($name in @('continuum.exe', 'continuum-core-server.exe')) {
        $source = Join-Path $TargetDirectory ('release\' + $name)
        $destination = Join-Path $slot $name
        Copy-Item -LiteralPath $source -Destination $destination -Force -ErrorAction Stop
        if ((Get-FileHash -LiteralPath $source).Hash -ne (Get-FileHash -LiteralPath $destination).Hash) {
            throw "Installed artifact verification failed: $destination"
        }
    }
    $launcher = Join-Path $slot 'run-service-hidden.ps1'
    Copy-Item -LiteralPath (Join-Path $RepoRoot 'tools\scripts\run-service-hidden.ps1') -Destination $launcher -Force
    $socket = $env:CONTINUUM_CORE_SOCKET
    if (-not $socket) { $socket = Join-Path ([IO.Path]::GetTempPath()) 'continuum-core.sock' }
    return [pscustomobject]@{
        artifact = (Join-Path $slot 'continuum-core-server.exe')
        socket = $socket
        launcher = $launcher
        cli = (Join-Path $slot 'continuum.exe')
        engine = (Join-Path $engineSlot 'llama-server.exe')
        logDirectory = (Join-Path $InstallRoot 'logs')
    }
}

function Register-CoreServiceRelease {
    param([Parameter(Mandatory = $true)]$Release, [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$WorkingDirectory = $RepoRoot, [switch]$PersistPreparedReceipt, [switch]$PrepareOnly)
    foreach ($field in $Release.PSObject.Properties) {
        $value = [string]$field.Value
        if (-not $value -or $value.IndexOfAny([char[]]@('"', "`r", "`n")) -ge 0 -or $value.EndsWith('\')) {
            throw "Release field $($field.Name) cannot be represented as a task argument."
        }
    }
    if (-not (Test-Path -LiteralPath $Release.engine -PathType Leaf)) { throw 'Candidate inference engine is missing; startup registration was preserved.' }
    # Reuse the CLI's artifact/SHA/runtime preflight before changing what the
    # next login will launch. A failed update must leave the old task intact.
    Push-Location $WorkingDirectory
    try {
        & $Release.cli reboot --prebuilt $Release.artifact --validate-only
        if ($LASTEXITCODE -ne 0) { throw 'Candidate validation failed; startup registration and the running core were preserved.' }
    } finally { Pop-Location }
    if ($PersistPreparedReceipt -or $PrepareOnly) { Save-CorePreparedRelease -Release $Release }
    if ($PrepareOnly) { return }
    $shell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File "{0}" -ExecutablePath "{1}" -CorePath "{2}" -SocketPath "{3}" -EnginePath "{4}" -LogDirectory "{5}"' -f $Release.launcher, $Release.cli, $Release.artifact, $Release.socket, $Release.engine, $Release.logDirectory
    $description = $Release | ConvertTo-Json -Compress
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    $canRun = $false
    if ($task) {
        $scheduler = New-Object -ComObject 'Schedule.Service'
        $scheduler.Connect()
        $canRun = Test-CoreServiceCallerAccess -UserSid $userSid -Sddl (
            $scheduler.GetFolder('\').GetTask('ContinuumCore').GetSecurityDescriptor(4))
    }
    if ($task -and $canRun -and $task.Description -eq $description -and $task.Actions.Count -eq 1 -and
        $task.Actions[0].Execute -eq $shell -and $task.Actions[0].Arguments -eq $arguments -and
        (Test-CoreTaskUser -UserId $task.Principal.UserId -ExpectedSid $userSid) -and $task.Principal.LogonType -eq 'S4U' -and
        $task.Principal.RunLevel -eq 'Limited' -and $task.Settings.Enabled -and
        $task.Settings.RestartCount -eq 999 -and $task.Settings.RestartInterval -eq 'PT1M' -and
        $task.Settings.ExecutionTimeLimit -eq 'PT0S' -and $task.Settings.MultipleInstances -eq 'IgnoreNew' -and
        $task.Settings.StartWhenAvailable -and -not $task.Settings.DisallowStartIfOnBatteries -and
        -not $task.Settings.StopIfGoingOnBatteries -and @($task.Triggers).Count -eq 1 -and
        $task.Triggers[0].CimClass.CimClassName -eq 'MSFT_TaskBootTrigger' -and $task.Triggers[0].Enabled) {
        Module-Skip 'service' 'prepared startup task already matches this release'
        return
    }
    New-Item -ItemType Directory -Force -Path $Release.logDirectory | Out-Null
    $planPath = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-' + [guid]::NewGuid().ToString('N') + '.json')
    try {
        @{ shell = $shell; arguments = $arguments; description = $description; userSid = $userSid; cli = $Release.cli } |
            ConvertTo-Json | Set-Content -LiteralPath $planPath -Encoding UTF8
        # Elevate registration only, with the caller's SID explicit. The core and
        # build stay unelevated. Registration deliberately does not start a core.
        Invoke-Elevated -Reason 'registering the ContinuumCore startup task (before core handoff)' -CommandLine @($shell, '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'RemoteSigned', '-File',
            (Join-Path $RepoRoot 'tools\scripts\register-core-service.ps1'), '-PlanPath', $planPath)
        if ($LASTEXITCODE -ne 0) { throw 'Startup registration failed; the running core has not been stopped.' }
    } finally {
        Remove-Item -LiteralPath $planPath -ErrorAction SilentlyContinue
    }
    $verified = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ($verified.Description -ne $description -or @($verified.Actions).Count -ne 1 -or
        $verified.Actions[0].Execute -ne $shell -or $verified.Actions[0].Arguments -ne $arguments -or
        -not (Test-CoreTaskUser -UserId $verified.Principal.UserId -ExpectedSid $userSid) -or -not $verified.Settings.Enabled -or
        $verified.Principal.LogonType -ne 'S4U' -or @($verified.Triggers).Count -ne 1 -or
        $verified.Triggers[0].CimClass.CimClassName -ne 'MSFT_TaskBootTrigger') {
        throw 'Startup registration did not match the prepared release (session-independent S4U at boot is required); refusing handoff.'
    }
    $scheduler = New-Object -ComObject 'Schedule.Service'
    $scheduler.Connect()
    if (-not (Test-CoreServiceCallerAccess -UserSid $userSid -Sddl (
        $scheduler.GetFolder('\').GetTask('ContinuumCore').GetSecurityDescriptor(4)))) {
        throw 'Startup task was registered but caller read/execute was not verified; refusing handoff.'
    }
    Module-Done 'service'
}

function Invoke-CoreServiceRelease {
    param([Parameter(Mandatory = $true)]$Release, [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$WorkingDirectory = $RepoRoot)
    Push-Location $WorkingDirectory
    try {
        & $Release.cli reboot --prebuilt $Release.artifact --service
        if ($LASTEXITCODE -ne 0) { throw 'Guarded service handoff failed; installer did not report success.' }
    } finally { Pop-Location }
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ($task.State -ne 'Running') { throw 'The core answered, but its prepared supervisor is not running.' }
    # Other terminals can briefly have the old CLI image open on Windows.
    # Retry file contention without terminating those user commands.
    $clientDir = Join-Path $env:USERPROFILE '.local\bin'
    New-Item -ItemType Directory -Force -Path $clientDir | Out-Null
    $client = Join-Path $clientDir 'continuum.exe'
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while ($true) {
        try { Copy-Item -LiteralPath $Release.cli -Destination $client -Force -ErrorAction Stop; break }
        catch {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw "The new core is verified and supervised, but refreshing $client failed: $_. Let active CLI commands finish and rerun the same installer."
            }
            Start-Sleep -Milliseconds 250
        }
    }
    $userPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if (@($userPath -split ';' | Where-Object { $_.TrimEnd('\') -eq $clientDir }).Count -eq 0) {
        [Environment]::SetEnvironmentVariable('PATH', ($clientDir + ';' + $userPath).TrimEnd(';'), 'User')
    }
    if (@($env:PATH -split ';' | Where-Object { $_.TrimEnd('\') -eq $clientDir }).Count -eq 0) {
        $env:PATH = $clientDir + ';' + $env:PATH
    }
    Module-Done 'run'
}
