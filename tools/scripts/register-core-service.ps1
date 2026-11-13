param([Parameter(Mandatory = $true)][string]$PlanPath, [string]$PlanSha)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\windows-service.ps1')
$planBytes = [IO.File]::ReadAllBytes($PlanPath)
if ($planBytes.Length -gt 65536) { throw 'Supervisor provision plan is oversized.' }
if ($PlanSha) {
    $hasher = [Security.Cryptography.SHA256]::Create()
    try { $actualSha = [BitConverter]::ToString($hasher.ComputeHash($planBytes)).Replace('-', '').ToLowerInvariant() } finally { $hasher.Dispose() }
    if ($PlanSha -cne $actualSha) { throw 'Supervisor provision plan changed across elevation.' }
}
$plan = [Text.Encoding]::UTF8.GetString($planBytes).TrimStart([char]0xfeff) | ConvertFrom-Json
if ($plan.bootstrapSource -and -not $PlanSha) { throw 'Bootstrap provisioning requires a digest-bound plan.' }
if (-not $plan.userSid -or -not $plan.shell -or -not $plan.arguments -or -not $plan.description -or -not $plan.cli) {
    throw 'Incomplete service registration plan.'
}
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$previous = @{}
foreach ($taskName in @('ContinuumCore', 'ContinuumDeploy')) {
    if (Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue) {
        # Refuse unsupported policy on either task before modifying either one.
        Get-CoreServiceSecurityDescriptor -Sddl (
            $scheduler.GetFolder('\').GetTask($taskName).GetSecurityDescriptor(4)) | Out-Null
        $old = $scheduler.GetFolder('\').GetTask($taskName)
        $previous[$taskName] = @{ Xml = $old.Xml; Sddl = $old.GetSecurityDescriptor(7) }
    }
}
if ($plan.bootstrapSource) { Install-CoreSupervisorBootstrap -Plan $plan }
try {
$action = New-ScheduledTaskAction -Execute $plan.shell -Argument $plan.arguments
if ($plan.bootstrapSource) { $action.WorkingDirectory = Split-Path $plan.cli -Parent }
# THE SUPERVISOR OUTLIVES THE LOGON SESSION. S4U = run as the user whether or not
# they are logged on, no stored password, no network credentials (LAN TCP is
# unaffected), least privilege. Until 2026-09-19 this was `Interactive` + AtLogOn:
# the core was supervised against a crash (RestartCount 999) but lived INSIDE the
# interactive session, and the 5090 went dark for 2 h 42 m when that session tore
# down (System log 09:50:04Z: five per-session user services 'terminated
# unexpectedly'; core log stops mid-line; nothing relaunched it — RestartOnFailure
# never fires for a session ending). install-service.sh's own header had named
# this failure ("the persona died because someone logged off"); its --system
# answer ran the core as HighestAvailable, which is the wrong trade. S4U at boot
# is the grid-node shape: session-independent AND unprivileged. Card 7b56a84b.
$principal = New-ScheduledTaskPrincipal -UserId $plan.userSid -LogonType S4U -RunLevel Limited
$trigger = New-ScheduledTaskTrigger -AtStartup
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -StartWhenAvailable -RestartInterval (New-TimeSpan -Minutes 1) -RestartCount 999 `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
if ($plan.PSObject.Properties.Name -contains 'coreEnabled') { $settings.Enabled = [bool]$plan.coreEnabled }
Register-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -Action $action -Principal $principal `
    -Trigger $trigger -Settings $settings -Description $plan.description -Force | Out-Null

# Registration may run under an elevated token, but routine `continuum start`
# runs as the caller. Task Scheduler's automatic principal ACE guarantees read,
# not execute. Add read/write/execute for that SID while preserving existing
# SYSTEM/administrator permissions and any other explicit task ACL entries.
try {
    $registered = $scheduler.GetFolder('\').GetTask('ContinuumCore')
    $updated = Grant-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registered.GetSecurityDescriptor(4)
    $registered.SetSecurityDescriptor($updated, 0)
    if (-not (Test-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registered.GetSecurityDescriptor(4))) {
        throw 'Caller read/write/execute was not saved.'
    }
} catch {
    throw "Startup task was registered, but caller access repair failed. Its startup action may already select the prepared release; the running core has not been stopped. Refusing handoff: $_"
}

# THE DEPLOY CONSUMER RIDES THE SAME SUPERVISOR CONTRACT (card 82af11f5). Every
# 10 minutes the installed CLI asks whether the Rust tracker has requested a
# deploy (state/deploy-request.json) and, if so, builds the tip and hands it to
# ContinuumCore through `reboot --service`. It is registered HERE, under the same
# elevated token, with the same session-independent S4U principal — never by the
# CLI unelevated (an Interactive task dies with the session, exactly as the core
# did on 2026-09-19). `deploy-consume --install` remains only as the dev-box
# convenience it says it is; this registration supersedes it (same task name).
# The plan names the installed CLI as its own field (`cli`); the description is the
# task's human label, not a channel to smuggle the release through.
$deployArguments = if ($plan.deployArguments) { $plan.deployArguments } else { 'deploy-consume' }
$deployAction = New-ScheduledTaskAction -Execute $plan.cli -Argument $deployArguments
if ($plan.bootstrapSource) { $deployAction.WorkingDirectory = Split-Path $plan.cli -Parent }
$deployPrincipal = New-ScheduledTaskPrincipal -UserId $plan.userSid -LogonType S4U -RunLevel Limited
$deployTrigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes 10)
$deploySettings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Hours 4) -MultipleInstances IgnoreNew
if ($plan.PSObject.Properties.Name -contains 'deployEnabled') { $deploySettings.Enabled = [bool]$plan.deployEnabled }
Register-ScheduledTask -TaskName ContinuumDeploy -TaskPath '\' -Action $deployAction -Principal $deployPrincipal `
    -Trigger $deployTrigger -Settings $deploySettings `
    -Description 'Continuum deploy consumer: turns a DeployRequest from the Rust tracker into reboot --service. Registered by the installer (register-core-service.ps1); session-independent.' -Force | Out-Null

$registeredDeploy = $scheduler.GetFolder('\').GetTask('ContinuumDeploy')
$deployAcl = Grant-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registeredDeploy.GetSecurityDescriptor(4)
$registeredDeploy.SetSecurityDescriptor($deployAcl, 0)
if (-not (Test-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registeredDeploy.GetSecurityDescriptor(4))) {
    throw 'Deploy consumer caller read/write/execute was not saved.'
}
} catch {
    $failure = $_
    $restoreFailures = @()
    foreach ($taskName in @('ContinuumCore', 'ContinuumDeploy')) {
        try {
            if ($previous.ContainsKey($taskName)) {
                $old = $previous[$taskName]
                $null = $scheduler.GetFolder('\').RegisterTask($taskName, $old.Xml, 6, $plan.userSid, $null, 2, $old.Sddl)
            } elseif (Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue) {
                $scheduler.GetFolder('\').DeleteTask($taskName, 0)
            }
        } catch { $restoreFailures += "$taskName`: $_" }
    }
    if ($restoreFailures.Count) { throw "Provisioning failed ($failure); previous task restoration failed: $($restoreFailures -join '; ')" }
    throw $failure
}
