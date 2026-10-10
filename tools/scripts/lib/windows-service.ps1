. (Join-Path $PSScriptRoot 'payload-paths.ps1')
. (Join-Path $PSScriptRoot 'windows-prepared.ps1')
. (Join-Path $PSScriptRoot 'windows-media-reconciliation.ps1')
# The CLI also loads this file in a fresh PowerShell for slot preparation.
# Reuse the shared native launcher there without resetting an outer installer's
# already-loaded elevation ownership state.
if (-not (Get-Command Invoke-InstallerProcess -CommandType Function -ErrorAction SilentlyContinue)) {
    . (Join-Path $PSScriptRoot 'windows-elevation.ps1')
}
# Native installer lifecycle. Two installed slots bound disk usage and keep
# running images out of Cargo's output directory. Never overwrite an active slot.
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
            (([long]$ace.AccessMask -band (4026531840L -bor 0x1201bf)) -ne 0)) {
            throw 'Unsupported startup task ACL: deny ACE overlaps caller read/execute; no ACL changes made.'
        }
    }
    return $security
}

function Test-CoreServiceCallerAccess {
    param([Parameter(Mandatory = $true)][string]$Sddl,
        [Parameter(Mandatory = $true)][string]$UserSid, [switch]$Update)
    $rights = if ($Update) { 0x1201bf } else { 0x1200a9 }
    $security = Get-CoreServiceSecurityDescriptor -Sddl $Sddl
    return @($security.DiscretionaryAcl | Where-Object {
        $_.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and
        ([int]$_.AceFlags -band [int][Security.AccessControl.AceFlags]::InheritOnly) -eq 0 -and
        $_.SecurityIdentifier.Value -eq $UserSid -and ($_.AccessMask -band $rights) -eq $rights
    }).Count -gt 0
}

function Grant-CoreServiceCallerAccess {
    param([Parameter(Mandatory = $true)][string]$Sddl,
        [Parameter(Mandatory = $true)][string]$UserSid, [switch]$Update)
    $rights = if ($Update) { 0x1201bf } else { 0x1200a9 }
    $security = Get-CoreServiceSecurityDescriptor -Sddl $Sddl
    $caller = [Security.Principal.SecurityIdentifier]::new($UserSid)
    if (-not (Test-CoreServiceCallerAccess -Sddl $Sddl -UserSid $UserSid -Update:$Update)) {
        $found = $false
        for ($i = 0; $i -lt $security.DiscretionaryAcl.Count; $i++) {
            $ace = $security.DiscretionaryAcl[$i]
            if ($ace.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and
                $ace.AceFlags -eq [Security.AccessControl.AceFlags]::None -and
                $ace.SecurityIdentifier -eq $caller) {
                $ace.AccessMask = $ace.AccessMask -bor $rights
                $security.DiscretionaryAcl[$i] = $ace
                $found = $true
                break
            }
        }
        if (-not $found) {
            $security.DiscretionaryAcl.InsertAce($security.DiscretionaryAcl.Count,
                [Security.AccessControl.CommonAce]::new([Security.AccessControl.AceFlags]::None,
                    [Security.AccessControl.AceQualifier]::AccessAllowed, $rights, $caller, $false, $null))
        }
    }
    return $security.GetSddlForm([Security.AccessControl.AccessControlSections]::Access)
}

function Protect-CoreBuildOutput {
    param([Parameter(Mandatory = $true)][string]$TargetDirectory)
    $TargetDirectory = ConvertTo-CoreImagePath $TargetDirectory
    $artifact = Join-Path $TargetDirectory 'release\continuum-core-server.exe'
    if (-not (Test-Path -LiteralPath $artifact)) { return }
    # Probe the actual output first. A service-session process may hide its
    # image path, but an exclusive writable handle proves this file is not
    # mapped. No process inspection or privilege is needed for that case.
    try {
        $probe = [IO.File]::Open($artifact, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $probe.Dispose()
        return
    } catch [IO.IOException] {
        # The mapped output still needs preservation below.
    }
    $processes = @(Get-CimInstance Win32_Process -ErrorAction Stop |
        Where-Object { $_.Name -eq 'continuum-core-server.exe' })
    if (@($processes | Where-Object { -not $_.ExecutablePath }).Count) {
        throw 'Cannot inspect running core image paths before building.'
    }
    if (-not @($processes | Where-Object { (ConvertTo-CoreImagePath $_.ExecutablePath) -eq $artifact }).Count) { return }
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

function Get-CoreEngineIdleSlot {
    # The core's own answer (card 2c5d0ec0, #4491): the engine slot that no live lane RECORD
    # names and `current` does not name. The lane records name the exe each lane launched, so
    # this needs no process-table read, which a service-session lane defeats (its path is
    # unreadable from the operator's session). One implementation for bash and PowerShell.
    # $null when this CLI predates the verb: the one deploy after the verbs land is driven by
    # the OLD CLI, and the caller keeps the process-table selection for that deploy, saying so.
    param([string]$Cli, [Parameter(Mandatory = $true)][string]$InstallRoot, [switch]$SkipIfBusy)
    # Native stderr under 'Stop' is a terminating error in Windows PowerShell 5.1; the exit code
    # is the contract here, so read it rather than the error stream.
    $ErrorActionPreference = 'Continue'
    if (-not $Cli -or -not (Test-Path -LiteralPath $Cli)) { return $null }
    try { $help = (Invoke-InstallerProcess $Cli @('--help') 2>&1 | Out-String) } catch { return $null }
    if ($help -notmatch 'continuum engine idle-slot') { return $null }
    $saved = $env:CONTINUUM_HOME
    try {
        $env:CONTINUUM_HOME = $InstallRoot
        $answer = @(Invoke-InstallerProcess $Cli @('engine', 'idle-slot') 2>$null)
        $code = $LASTEXITCODE
    } finally { $env:CONTINUUM_HOME = $saved }
    if ($code -eq 3) {
        # Every slot is current or run by a live lane (a relaunch onto the last engine has not
        # finished). A deploy skips the engine and still lands the core (card 3f8f5754, the bash
        # installer's exit 3); a first install, with nothing to keep, refuses.
        if ($SkipIfBusy) { return 'BUSY' }
        throw 'All installed engine slots are live or registered; refusing to overwrite an inference engine.'
    }
    if ($code -ne 0 -or -not $answer.Count) { throw "continuum engine idle-slot failed (exit $code); no slot can be proven idle." }
    $root = ConvertTo-CoreImagePath (Join-Path (Get-ManagedPayloadRoot -HomeRoot $InstallRoot) 'bin')
    $slot = ConvertTo-CoreImagePath ([string]$answer[-1]).Trim()
    if (-not @('engine-a', 'engine-b', 'engine-c' | Where-Object { [string]::Equals($slot, (Join-Path $root $_), [StringComparison]::OrdinalIgnoreCase) }).Count) {
        throw "continuum engine idle-slot answered $slot, which is not an engine slot under $root."
    }
    return $slot
}

function Get-CoreReceiptedEngineSlots {
    # The engine slots the installer's own records still name: the registered descriptor and the
    # active and previous release receipts. The core's idle-slot verb reads only its current and
    # previous POINTERS. When an activation fails after writing its receipt but before promoting the
    # pointer, the two disagree, and the verb calls a receipted slot idle. On the 5090
    # (2026-10-10 02:05Z) the install then copied the new engine over the slot install-active.json
    # named, destroying that receipt's payload, and refused on the next step. Only the engine PATH is
    # read here, never the hashes: this asks which slot a record owns, not whether it is intact.
    param([Parameter(Mandatory = $true)][string]$InstallRoot, $Descriptor)
    $engines = @()
    if ($Descriptor -and $Descriptor.engine) { $engines += [string]$Descriptor.engine }
    foreach ($name in @('install-active.json', 'install-previous.json')) {
        $path = Join-Path $InstallRoot $name
        if (-not (Test-Path -LiteralPath $path)) { continue }
        if ((Get-Item -LiteralPath $path).Length -gt 65536) { throw "Release receipt $name is oversized; no slot can be proven unreceipted." }
        $receipt = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -ErrorAction Stop
        if ($receipt.release -and $receipt.release.engine) { $engines += [string]$receipt.release.engine }
    }
    @($engines | ForEach-Object { ConvertTo-CoreImagePath (Split-Path $_ -Parent) })
}

function Select-CoreEngineSlot {
    param([string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'), $Descriptor, [string]$Cli, [switch]$SkipIfBusy)
    $root = ConvertTo-CoreImagePath (Join-Path (Get-ManagedPayloadRoot -HomeRoot $InstallRoot) 'bin')
    $fromCore = Get-CoreEngineIdleSlot -Cli $Cli -InstallRoot $InstallRoot -SkipIfBusy:$SkipIfBusy
    if ($fromCore -eq 'BUSY') { return $null }
    $receipted = @(Get-CoreReceiptedEngineSlots -InstallRoot $InstallRoot -Descriptor $Descriptor)
    if ($fromCore) {
        if (@($receipted | Where-Object { [string]::Equals($_, $fromCore, [StringComparison]::OrdinalIgnoreCase) }).Count) {
            # The verb prefers a slot that is neither current nor previous, so on a node whose records
            # agree with its pointers this never fires. When it does, the records disagree, and
            # overwriting would destroy a release a receipt still owns: refuse, and let the supported
            # receipt reconciliation retire the record first.
            if ($SkipIfBusy) { return $null }
            throw "The core named $fromCore idle, but an installer release receipt still names it; refusing to overwrite a receipted engine."
        }
        # Belt and braces: a live engine whose path IS readable must not sit in the answer.
        $readable = @(Get-CimInstance Win32_Process -ErrorAction Stop |
            Where-Object { $_.Name -eq 'llama-server.exe' -and $_.ExecutablePath } |
            ForEach-Object { ConvertTo-CoreImagePath $_.ExecutablePath })
        if (@($readable | Where-Object { $_.StartsWith($fromCore + '\', [StringComparison]::OrdinalIgnoreCase) }).Count) {
            throw "The core named $fromCore idle, but a running engine executes from it; refusing to overwrite it."
        }
        return $fromCore
    }
    Write-Warning 'This CLI predates engine slots: selecting the engine slot from the process table for this deploy; the next deploy asks the core.'
    # Engines have an independent lifetime: a warm lane can outlive its core.
    # Keep room for that mapped engine, the registered release, and a candidate.
    $engines = @(Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.Name -eq 'llama-server.exe' })
    # A lane the supervisor started runs in the service session, and its path is unreadable from
    # the operator's session: WMI's ExecutablePath is empty and OpenProcess(QUERY_LIMITED) is
    # denied (5090, 2026-09-27). Refusing on that made the engine arm unrunnable whenever a lane
    # was up. An unreadable engine counts as the REGISTERED engine (the only one the supervisor
    # launches), which the descriptor adds below; should one ever live in another slot, Windows
    # locks a running image, so installing over it fails loudly rather than replacing it.
    $enginePaths = @($engines | Where-Object { $_.ExecutablePath } | ForEach-Object { ConvertTo-CoreImagePath $_.ExecutablePath })
    if ($Descriptor.engine) { $enginePaths += ConvertTo-CoreImagePath $Descriptor.engine }
    elseif (@($engines | Where-Object { -not $_.ExecutablePath }).Count) { throw 'A running inference engine is unreadable and no engine is registered; refusing to pick a slot.' }
    $engineSlot = $null
    foreach ($name in @('engine-a', 'engine-b', 'engine-c')) {
        $candidate = Join-Path $root $name
        if (@($receipted | Where-Object { [string]::Equals($_, $candidate, [StringComparison]::OrdinalIgnoreCase) }).Count) { continue }
        if (-not @($enginePaths | Where-Object { $_.StartsWith($candidate + '\', [StringComparison]::OrdinalIgnoreCase) }).Count) {
            $engineSlot = $candidate; break
        }
    }
    if (-not $engineSlot) {
        if ($SkipIfBusy) { return $null }
        throw 'All installed engine slots are live or registered; refusing to overwrite an inference engine.'
    }
    return $engineSlot
}

function Invoke-CoreEnginePromote {
    # `current` is the one truth on every OS (card d5584dfc, option (b)): a drift-verified slot is
    # promoted by the core's own verb, an unprivileged file write, so neither an unattended deploy
    # nor install needs the scheduled task re-registered for the engine to change. Returns $false
    # (and says so) when this CLI predates the verb; then the release registration bootstraps
    # `current` at the next service start, as before. A refused promote throws.
    param([string]$Cli, [Parameter(Mandatory = $true)][string]$InstallRoot, [Parameter(Mandatory = $true)][string]$Slot)
    # Native stderr under 'Stop' is terminating in Windows PowerShell 5.1: read exit codes.
    $ErrorActionPreference = 'Continue'
    if (-not $Cli -or -not (Test-Path -LiteralPath $Cli)) {
        Write-Warning 'No registered CLI to promote the engine with; the release registration bootstraps it.'
        return $false
    }
    try { $help = (Invoke-InstallerProcess $Cli @('--help') 2>&1 | Out-String) } catch { $help = '' }
    if ($help -notmatch 'continuum engine promote') {
        Write-Warning 'This CLI predates engine slots: the release registration bootstraps the engine this deploy.'
        return $false
    }
    $name = Split-Path -Leaf $Slot
    $stamp = (Get-Content -LiteralPath (Join-Path $Slot '.llama-server.stamp') -Raw -ErrorAction Stop).Trim()
    $saved = $env:CONTINUUM_HOME
    try {
        $env:CONTINUUM_HOME = $InstallRoot
        $said = (Invoke-InstallerProcess $Cli @('engine', 'promote', $name, $stamp) 2>&1 | Out-String).Trim()
        $code = $LASTEXITCODE
    } finally { $env:CONTINUUM_HOME = $saved }
    if ($code -ne 0) { throw "continuum engine promote refused $name (exit $code): $said" }
    return $true
}

function Prepare-CoreServiceEngine {
    param([Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$Description,
        [Parameter(Mandatory = $true)][string]$ReceiptPath, [switch]$PrebuiltOnly)
    # The calling reboot holds install.lock across preparation and handoff.
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ((Get-CoreRegisteredRelease -Task $task | ConvertTo-Json -Compress) -cne $Description) { throw 'Installed release changed before engine preparation.' }
    $release = $Description | ConvertFrom-Json -ErrorAction Stop
    $requirement = Get-CoreEngineRequirement -RepoRoot $RepoRoot
    if ($PrebuiltOnly) {
        # An explicit published handoff can only promote its registered verified
        # payload. Drift is a preparation failure, never permission to compile.
        $built = Split-Path $release.engine
        $drift = Get-CoreEngineDrift -Directory $built -Requirement $requirement
        if ($drift) { throw $drift }
        $engineReceipt = Get-CoreEngineReceipt -Directory $built
        Get-CorePublishedEngineStamp -Directory $built -Receipt $engineReceipt | Out-Null
        if (-not (Invoke-CoreEnginePromote -Cli $release.cli -InstallRoot (Join-Path $env:USERPROFILE '.continuum') -Slot $built)) { throw 'Prepared CLI cannot promote the published engine.' }
        $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
        if ((Get-CoreRegisteredRelease -Task $task | ConvertTo-Json -Compress) -cne $Description) { throw 'Installed release changed during engine preparation.' }
        [IO.File]::WriteAllText($ReceiptPath, $release.engine, (New-Object Text.UTF8Encoding $false))
        return
    }
    # A slot that ALREADY holds the pinned engine is promoted as is (card 6d5bacab): a build whose
    # promotion never happened (the 5090's cancelled install left engine-c built and verified while
    # current stayed engine-b). Promotion overwrites nothing, so it needs no proof that the slot is
    # idle, and with every slot populated and a lane that predates engine records, idle-slot would
    # SKIP every deploy and that engine would never be used. This runs only when the current
    # engine drifts from the pin, so a matching slot is never the current one.
    $installRoot = Join-Path $env:USERPROFILE '.continuum'
    $slotRoot = ConvertTo-CoreImagePath (Join-Path (Get-ManagedPayloadRoot -HomeRoot $installRoot) 'bin')
    foreach ($name in @('engine-a', 'engine-b', 'engine-c')) {
        $built = Join-Path $slotRoot $name
        if (-not (Test-Path -LiteralPath (Join-Path $built 'llama-server.exe'))) { continue }
        if (Get-CoreEngineDrift -Directory $built -Requirement $requirement) { continue }
        if (Invoke-CoreEnginePromote -Cli $release.cli -InstallRoot $installRoot -Slot $built) {
            $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
            if ((Get-CoreRegisteredRelease -Task $task | ConvertTo-Json -Compress) -cne $Description) { throw 'Installed release changed during engine preparation.' }
            [IO.File]::WriteAllText($ReceiptPath, (Join-Path $built 'llama-server.exe'), (New-Object Text.UTF8Encoding $false))
            return
        }
    }
    $slot = Select-CoreEngineSlot -Descriptor $release -Cli $release.cli -SkipIfBusy
    if (-not $slot) {
        # The core still deploys on the engine it has; the next deploy builds this one. An
        # explicit receipt line, so the caller never reads an empty receipt as a skip.
        $why = 'every engine slot is current or run by a live lane'
        [IO.File]::WriteAllText($ReceiptPath, "SKIP: $why", (New-Object Text.UTF8Encoding $false))
        Write-Warning "Engine not prepared this deploy: $why."
        return
    }
    Mod-LlamaServer -RepoRoot $RepoRoot -InstallDirectory $slot -RequireReceipt
    $after = Get-CoreEngineRequirement -RepoRoot $RepoRoot
    if ($after.source_revision -cne $requirement.source_revision -or $after.backend -cne $requirement.backend) {
        throw 'Tracked engine requirement changed during preparation.'
    }
    $drift = Get-CoreEngineDrift -Directory $slot -Requirement $requirement
    if ($drift) { throw $drift }
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ((Get-CoreRegisteredRelease -Task $task | ConvertTo-Json -Compress) -cne $Description) { throw 'Installed release changed during engine preparation.' }
    # The verified slot becomes the engine by the core's own verb, for install and unattended
    # deploy alike (card d5584dfc); install still registers it as the release's bootstrap engine.
    $null = Invoke-CoreEnginePromote -Cli $release.cli -InstallRoot (Join-Path $env:USERPROFILE '.continuum') -Slot $slot
    [IO.File]::WriteAllText($ReceiptPath, (Join-Path $slot 'llama-server.exe'), (New-Object Text.UTF8Encoding $false))
}

function Get-CoreBrowserReleaseDrift {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, $Release)
    $task = $null
    if (-not $Release) {
        $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
        $Release = Get-CoreRegisteredRelease -Task $task
    }
    $root = [IO.Path]::GetFullPath($RepoRoot)
    if (-not [string]::Equals($Release.eyeRoot, $root, [StringComparison]::OrdinalIgnoreCase)) {
        return 'browser asset root is not registered'
    }
    if ($task -and -not (Test-CoreProvisionedTask -Task $task) -and (@($task.Actions).Count -ne 1 -or
        $task.Actions[0].Arguments.IndexOf((' -EyeRoot "{0}"' -f $root), [StringComparison]::OrdinalIgnoreCase) -lt 0)) {
        return 'startup action does not pass the browser asset root'
    }
    $source = Join-Path $root 'tools\scripts\run-service-hidden.ps1'
    if (-not (Test-Path -LiteralPath $Release.launcher -PathType Leaf) -or
        (Get-FileHash -LiteralPath $source).Hash -ne (Get-FileHash -LiteralPath $Release.launcher).Hash) {
        return 'installed launcher differs from the release launcher'
    }
    return ''
}

function Update-CoreBrowserRelease {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    $release = Get-CoreRegisteredRelease -Task $task
    $selected = $release | ConvertTo-Json -Compress
    if (-not (Get-CoreBrowserReleaseDrift -RepoRoot $RepoRoot)) { return }
    # Never rewrite a live, integrity-bound launcher. The same staging owner
    # prepares an inactive generation even for browser metadata migration.
    $root = [IO.Path]::GetFullPath($RepoRoot)
    $release = New-CoreServiceRelease -RepoRoot $root -ArtifactDirectory (Split-Path $release.artifact -Parent) -EnginePath $release.engine
    if ((Get-CoreBrowserReleaseDrift -RepoRoot $root -Release $release)) {
        throw 'Browser launcher copy did not verify; core handoff refused.'
    }
    if ((Get-CoreRegisteredRelease -Task (Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop) | ConvertTo-Json -Compress) -cne $selected) {
        throw 'Installed release changed during browser migration; core handoff refused.'
    }
    # This operation preserves the installed binaries and only migrates browser
    # metadata. Validate that installed release from its slot, as prepared resume
    # does; comparing it with the newer checkout prevents every version upgrade.
    $workingDirectory = Split-Path -Parent $release.artifact
    try { Register-CoreServiceRelease -Release $release -RepoRoot $root -WorkingDirectory $workingDirectory }
    finally { Clear-Elevation }
    if (Get-CoreBrowserReleaseDrift -RepoRoot $root) {
        throw 'Registered browser release did not converge; core handoff refused.'
    }
}

function New-CoreServiceRelease {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$InstallRoot = (Join-Path $env:USERPROFILE '.continuum'),
        [string]$TargetDirectory = $env:CARGO_TARGET_DIR,
        [string]$ArtifactDirectory,
        [string]$EnginePath,
        [switch]$ReconcileLegacyMedia
    )
    if (-not $ArtifactDirectory) { $ArtifactDirectory = Join-Path $TargetDirectory 'release' }
    $root = ConvertTo-CoreImagePath (Join-Path (Get-ManagedPayloadRoot -HomeRoot $InstallRoot) 'bin')
    $liveProcesses = @(Get-CimInstance Win32_Process -ErrorAction Stop |
        Where-Object { $_.Name -in @('continuum.exe', 'continuum-core-server.exe') })
    $unknownImages = @($liveProcesses | Where-Object { -not $_.ExecutablePath }).Count -gt 0
    $liveImages = @($liveProcesses | Where-Object { $_.ExecutablePath } | ForEach-Object { ConvertTo-CoreImagePath $_.ExecutablePath })
    $descriptor = $null
    $registered = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    if ($registered) {
        $descriptor = $null
        try { $descriptor = Get-CoreRegisteredRelease -Task $registered -InstallRoot $InstallRoot }
        catch {
            if (Test-CoreProvisionedTask -Task $registered) { throw }
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
    if ($unknownImages -and -not $descriptor.artifact) {
        throw 'Cannot inspect all live Continuum image paths and no registered release protects startup files.'
    }
    $registeredReleaseSnapshot = $descriptor | ConvertTo-Json -Compress
    $slot = $null
    $mediaBlocked = @()
    $serviceFiles = @('continuum.exe', 'continuum-core-server.exe', 'livekit-bridge.exe', 'run-service-hidden.ps1', 'start-livekit-windows.ps1')
    foreach ($name in @('service-a', 'service-b')) {
        $candidate = Join-Path $root $name
        $occupied = @($liveImages | Where-Object { $_.StartsWith($candidate + '\', [StringComparison]::OrdinalIgnoreCase) })
        if ($occupied.Count -gt 0) { continue }
        # Service-session images can be unreadable. Preserve the registered slot
        # above, then verify every destination before touching any candidate file.
        # Windows denies write access to mapped executables. Copy-Item retains
        # that protection if a process starts after this non-mutating probe.
        $blocked = @()
        foreach ($file in $serviceFiles) {
            $destination = Join-Path $candidate $file
            if (-not (Test-Path -LiteralPath $destination)) { continue }
            try {
                $probe = [IO.File]::Open($destination, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
                $probe.Dispose()
            } catch [IO.IOException] { $blocked += $file }
            catch [UnauthorizedAccessException] { $blocked += $file }
        }
        if ($blocked.Count -eq 1 -and $blocked[0] -eq 'livekit-bridge.exe') {
            $mediaBlocked += $candidate
        }
        if ($blocked.Count -eq 0) { $slot = $candidate; break }
    }
    if (-not $slot -and $mediaBlocked.Count -eq 1) {
        # Try all free slots first. Only an otherwise idle, unregistered slot
        # qualifies. PrepareOnly never borrows elevation or changes the bridge.
        $candidate = $mediaBlocked[0]
        Invoke-CoreLegacyMediaReconciliation -Image (Join-Path $candidate 'livekit-bridge.exe') -InstallRoot $InstallRoot -AllowElevation:$ReconcileLegacyMedia
        $currentTask = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
        if ($currentTask.Description -cne $registered.Description) {
            throw 'Registered release changed during legacy media reconciliation; candidate files were preserved.'
        }
        $currentRelease = if ($currentTask) { Get-CoreRegisteredRelease -Task $currentTask -InstallRoot $InstallRoot } else { $null }
        if (($currentRelease | ConvertTo-Json -Compress) -cne $registeredReleaseSnapshot) {
            throw 'Registered release changed during legacy media reconciliation; candidate files were preserved.'
        }
        # Recheck every image lock after graceful exit; Copy-Item retains OS
        # protection if another owner maps an image after this observation.
        foreach ($file in $serviceFiles) {
            $destination = Join-Path $candidate $file
            if (-not (Test-Path -LiteralPath $destination)) { continue }
            $probe = [IO.File]::Open($destination, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            $probe.Dispose()
        }
        $slot = $candidate
    }
    if (-not $slot) { throw 'Both installed core service slots are in use; resolve the extra live instance before updating.' }
    # The CLI this release installs is the one that knows the lane records' contract.
    $engineSlot = if ($EnginePath) { Split-Path $EnginePath -Parent } else { Select-CoreEngineSlot -InstallRoot $InstallRoot -Descriptor $descriptor -Cli (Join-Path $ArtifactDirectory 'continuum.exe') }
    Clear-CorePreparedSelectionForSlot -InstallRoot $InstallRoot -Slot $slot
    New-Item -ItemType Directory -Force -Path $slot | Out-Null
    # CI cores carry declared runtime DLLs beside the binaries. The native deploy
    # stage honors this same manifest; installer/prepared rollback must do so too.
    $runtimeNames = @()
    $runtimeList = Join-Path $ArtifactDirectory 'runtime-libs.txt'
    if (Test-Path -LiteralPath $runtimeList -PathType Leaf) {
        $runtimeNames = @(Get-Content -LiteralPath $runtimeList -ErrorAction Stop | ForEach-Object { $_.Trim() } | Where-Object { $_ })
        foreach ($name in $runtimeNames) {
            if ($name -notmatch '^[A-Za-z0-9_.-]+\.dll$' -or -not (Test-Path -LiteralPath (Join-Path $ArtifactDirectory $name) -PathType Leaf)) {
                throw "Invalid or missing declared core runtime library: $name"
            }
        }
        $runtimeNames += 'runtime-libs.txt'
    }
    foreach ($name in (@('continuum.exe', 'continuum-core-server.exe', 'livekit-bridge.exe') + $runtimeNames)) {
        $source = Join-Path $ArtifactDirectory $name
        $destination = Join-Path $slot $name
        Copy-Item -LiteralPath $source -Destination $destination -Force -ErrorAction Stop
        if ((Get-FileHash -LiteralPath $source).Hash -ne (Get-FileHash -LiteralPath $destination).Hash) {
            throw "Installed artifact verification failed: $destination"
        }
    }
    $launcher = Join-Path $slot 'run-service-hidden.ps1'
    Copy-Item -LiteralPath (Join-Path $RepoRoot 'tools\scripts\run-service-hidden.ps1') -Destination $launcher -Force
    Copy-Item -LiteralPath (Join-Path $RepoRoot 'tools\scripts\start-livekit-windows.ps1') -Destination (Join-Path $slot 'start-livekit-windows.ps1') -Force -ErrorAction Stop
    $socket = $env:CONTINUUM_CORE_SOCKET
    if (-not $socket) { $socket = Join-Path ([IO.Path]::GetTempPath()) 'continuum-core.sock' }
    return [pscustomobject]@{
        artifact = (Join-Path $slot 'continuum-core-server.exe')
        socket = $socket
        launcher = $launcher
        cli = (Join-Path $slot 'continuum.exe')
        engine = (Join-Path $engineSlot 'llama-server.exe')
        logDirectory = (Join-Path $InstallRoot 'logs')
        eyeRoot = [IO.Path]::GetFullPath($RepoRoot)
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
        Invoke-InstallerProcess $Release.cli @('reboot', '--prebuilt', $Release.artifact, '--validate-only')
        if ($LASTEXITCODE -ne 0) { throw 'Candidate validation failed; startup registration and the running core were preserved.' }
    } finally { Pop-Location }
    $selectionRecovery = Get-CoreDamagedSelectionRecovery
    if ($selectionRecovery) {
        if (($Release | ConvertTo-Json -Compress) -cne ($selectionRecovery.Prepared.Receipt.release | ConvertTo-Json -Compress)) {
            throw 'Damaged selection must resume the existing verified preparation before restaging.'
        }
        Assert-CoreRecoveryStopped -InstallRoot (Join-Path $env:USERPROFILE '.continuum')
    }
    if (($PersistPreparedReceipt -or $PrepareOnly) -and -not $selectionRecovery) { Save-CorePreparedRelease -Release $Release }
    if ($PrepareOnly) { return }
    $protocol = (Invoke-InstallerProcess $Release.cli @('installed-service', '--protocol') | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $protocol -cne '3') { throw 'Candidate CLI lacks supervisor protocol 3; registration and active release were preserved.' }
    $shell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue
    $bootstrap = Get-CoreSupervisorBootstrap -Generation (Get-FileHash -LiteralPath $Release.cli -Algorithm SHA256).Hash.ToLowerInvariant()
    # Reuse a verified compatible authority; only an incompatible generation
    # requires a new protected sibling and normal elevated registration.
    if ($task -and (Test-CoreProvisionedTask -Task $task)) {
        $priorBootstrap = ($task.Description | ConvertFrom-Json).bootstrap
        Assert-CoreSupervisorBootstrap -Path $priorBootstrap
        $priorProtocol = (Invoke-InstallerProcess $priorBootstrap @('installed-service', '--protocol') | Out-String).Trim()
        if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect installed bootstrap capability.' }
        if ($priorProtocol -ceq '3') { $bootstrap = $priorBootstrap }
    }
    $activePath = Join-Path $env:USERPROFILE '.continuum\install-active.json'
    $arguments = 'installed-service core "{0}"' -f $activePath
    $deployArguments = 'installed-service deploy "{0}"' -f $activePath
    $description = [ordered]@{ schema = 2; activeRelease = $activePath; bootstrap = $bootstrap } | ConvertTo-Json -Compress
    $userSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $deploy = Get-ScheduledTask -TaskName ContinuumDeploy -TaskPath '\' -ErrorAction SilentlyContinue
    $canRun = $false
    if ($task) {
        $scheduler = New-Object -ComObject 'Schedule.Service'
        $scheduler.Connect()
        $canRun = Test-CoreServiceCallerAccess -UserSid $userSid -Sddl (
            $scheduler.GetFolder('\').GetTask('ContinuumCore').GetSecurityDescriptor(4))
    }
    $compatibleTask = $task -and $canRun -and $task.Actions.Count -eq 1 -and
        $task.Actions[0].Execute -eq $bootstrap -and
        $task.Actions[0].WorkingDirectory -eq (Split-Path $bootstrap -Parent) -and
        (Test-CoreTaskUser -UserId $task.Principal.UserId -ExpectedSid $userSid) -and $task.Principal.LogonType -eq 'S4U' -and
        $task.Principal.RunLevel -eq 'Limited' -and
        $task.Settings.RestartCount -eq 999 -and $task.Settings.RestartInterval -eq 'PT1M' -and
        $task.Settings.ExecutionTimeLimit -eq 'PT0S' -and $task.Settings.MultipleInstances -eq 'IgnoreNew' -and
        $task.Settings.StartWhenAvailable -and -not $task.Settings.DisallowStartIfOnBatteries -and
        -not $task.Settings.StopIfGoingOnBatteries -and @($task.Triggers).Count -eq 1 -and
        $task.Triggers[0].CimClass.CimClassName -eq 'MSFT_TaskBootTrigger' -and $task.Triggers[0].Enabled
    $provisioned = $false
    if ($compatibleTask -and $task.Description -eq $description -and $task.Actions[0].Arguments -eq $arguments) {
        $folder = $scheduler.GetFolder('\')
        $provisioned = $deploy -and @($deploy.Actions).Count -eq 1 -and
            $deploy.Actions[0].Execute -eq $bootstrap -and $deploy.Actions[0].Arguments -eq $deployArguments -and
            $deploy.Actions[0].WorkingDirectory -eq (Split-Path $bootstrap -Parent) -and
            (Test-CoreTaskUser -UserId $deploy.Principal.UserId -ExpectedSid $userSid) -and
            $deploy.Principal.LogonType -eq 'S4U' -and $deploy.Principal.RunLevel -eq 'Limited' -and
            (Test-CoreServiceCallerAccess -Sddl ($folder.GetTask('ContinuumCore').GetSecurityDescriptor(4)) -UserSid $userSid) -and
            (Test-CoreServiceCallerAccess -Sddl ($folder.GetTask('ContinuumDeploy').GetSecurityDescriptor(4)) -UserSid $userSid)
    }
    if ($provisioned) {
        Assert-CoreSupervisorBootstrap -Path $bootstrap -UserSid $userSid
        if ($selectionRecovery) { Complete-CoreDamagedSelectionRecovery -Plan $selectionRecovery -Release $Release }
        else { Save-CorePreparedRelease -Release $Release -Selection Active }
        Module-Skip 'service' 'fixed supervisor retained; verified active release committed without elevation'
        return
    }
    # First migration must never install a fixed action pointing at nothing.
    # Publish the SAME legacy selection first; only after both task contracts
    # verify may the candidate replace it. A mid-migration reboot still selects
    # the prior installed release, and the first update has a real rollback.
    # A recognized damaged selection remains launch-invalid throughout bootstrap
    # migration. Do not reseal it or promise that it is a usable rollback.
    $seededFirstInstall = $false
    if (-not $selectionRecovery) { $seededFirstInstall = Initialize-CoreActiveSelection -Task $task -Release $Release }
    New-Item -ItemType Directory -Force -Path $Release.logDirectory | Out-Null
    $planPath = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-' + [guid]::NewGuid().ToString('N') + '.json')
    try {
        @{ shell = $bootstrap; arguments = $arguments; description = $description; userSid = $userSid;
            cli = $bootstrap; deployArguments = $deployArguments; bootstrapSource = $Release.cli;
            bootstrapHashes = (Get-CoreReleaseHashes -Release $Release);
            coreEnabled = $(if ($task) { [bool]$task.Settings.Enabled } else { $true });
            deployEnabled = $(if ($deploy) { [bool]$deploy.Settings.Enabled } else { $true }) } |
            ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $planPath -Encoding UTF8
        # Elevate registration only, with the caller's SID explicit. The core and
        # build stay unelevated. Registration deliberately does not start a core.
        Invoke-Elevated -Reason 'provisioning fixed Continuum supervision once (routine release updates remain unelevated)' -CommandLine @($shell, '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'RemoteSigned', '-File',
                (Join-Path $RepoRoot 'tools\scripts\register-core-service.ps1'), '-PlanPath', $planPath,
                '-PlanSha', (Get-FileHash -LiteralPath $planPath -Algorithm SHA256).Hash.ToLowerInvariant())
        if ($LASTEXITCODE -ne 0) {
            if ($seededFirstInstall) { Remove-Item -LiteralPath $activePath -ErrorAction Stop }
            throw 'Startup registration failed; the running core has not been stopped.'
        }
    } finally {
        Remove-Item -LiteralPath $planPath -ErrorAction SilentlyContinue
    }
    $verified = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ($verified.Description -ne $description -or @($verified.Actions).Count -ne 1 -or
        $verified.Actions[0].Execute -ne $bootstrap -or $verified.Actions[0].Arguments -ne $arguments -or
        $verified.Actions[0].WorkingDirectory -ne (Split-Path $bootstrap -Parent) -or
        -not (Test-CoreTaskUser -UserId $verified.Principal.UserId -ExpectedSid $userSid) -or
        ($task -and $verified.Settings.Enabled -ne $task.Settings.Enabled) -or
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
    $verifiedDeploy = Get-ScheduledTask -TaskName ContinuumDeploy -TaskPath '\' -ErrorAction Stop
    if (@($verifiedDeploy.Actions).Count -ne 1 -or $verifiedDeploy.Actions[0].Execute -cne $bootstrap -or
        $verifiedDeploy.Actions[0].Arguments -cne $deployArguments -or
        $verifiedDeploy.Actions[0].WorkingDirectory -cne (Split-Path $bootstrap -Parent) -or
        -not (Test-CoreTaskUser -UserId $verifiedDeploy.Principal.UserId -ExpectedSid $userSid) -or
        $verifiedDeploy.Principal.LogonType -ne 'S4U' -or $verifiedDeploy.Principal.RunLevel -ne 'Limited' -or
        ($deploy -and $verifiedDeploy.Settings.Enabled -ne $deploy.Settings.Enabled) -or
        -not (Test-CoreServiceCallerAccess -UserSid $userSid -Sddl ($scheduler.GetFolder('\').GetTask('ContinuumDeploy').GetSecurityDescriptor(4)))) {
        throw 'Deploy supervisor contract did not verify; active release was preserved.'
    }
    Assert-CoreSupervisorBootstrap -Path $bootstrap -UserSid $userSid
    if ($selectionRecovery) { Complete-CoreDamagedSelectionRecovery -Plan $selectionRecovery -Release $Release }
    else { Save-CorePreparedRelease -Release $Release -Selection Active }
    Module-Done 'service'
}

function Invoke-CoreServiceRelease {
    param([Parameter(Mandatory = $true)]$Release, [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$WorkingDirectory = $RepoRoot, [Parameter(Mandatory = $true)][IO.FileStream]$InstallLease)
    $leasePath = $InstallLease.Name
    # Keep the validated CLI's lease capability and its paired Core bytes fixed
    # across the brief lock transfer; a concurrent reboot cannot swap the files
    # between capability validation and execution.
    $cliPin = [IO.File]::Open($Release.cli, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $corePin = $null
    try {
    $corePin = [IO.File]::Open($Release.artifact, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    Push-Location $WorkingDirectory
    try {
        # Saved releases can contain older CLIs. Prove this candidate implements
        # the same lease protocol while the installer still owns exclusion.
        $validation = @(Invoke-InstallerProcess $Release.cli @('reboot', '--prebuilt', $Release.artifact, '--validate-only'))
        if ($LASTEXITCODE -ne 0 -or 'continuum-install-lease-protocol:1' -cnotin $validation) {
            throw 'Prepared CLI lacks the verified installation lease protocol; prepare a current release before handoff.'
        }
        $hash = [Security.Cryptography.SHA256]::Create()
        try { $descriptorSha = [BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes(($Release | ConvertTo-Json -Compress)))).Replace('-', '').ToLowerInvariant() }
        finally { $hash.Dispose() }
        # Registration reserves the candidate slot across this transfer. Reboot
        # reacquires the same lease and validates its descriptor before stopping.
        $InstallLease.Dispose()
        Invoke-InstallerProcess $Release.cli @('reboot', '--prebuilt', $Release.artifact, '--service', '--service-descriptor-sha', $descriptorSha)
        if ($LASTEXITCODE -ne 0) { throw 'Guarded service handoff failed; installer did not report success.' }
    } finally { Pop-Location }
    } finally {
        if ($corePin) { $corePin.Dispose() }
        $cliPin.Dispose()
    }
    # A newer installer may win the transfer back. Never overwrite its public
    # CLI or PATH with this invocation's older selected release.
    $tailLease = [IO.File]::Open($leasePath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
    $task = Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction Stop
    if ((Get-CoreRegisteredRelease -Task $task | ConvertTo-Json -Compress) -cne ($Release | ConvertTo-Json -Compress)) { throw 'Installed release changed after handoff; public CLI was preserved.' }
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
    } finally { $tailLease.Dispose() }
}
