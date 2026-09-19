param([Parameter(Mandatory = $true)][string]$PlanPath)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib\windows-service.ps1')
$plan = Get-Content -LiteralPath $PlanPath -Raw | ConvertFrom-Json
if (-not $plan.userSid -or -not $plan.shell -or -not $plan.arguments -or -not $plan.description) {
    throw 'Incomplete service registration plan.'
}
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
if (Get-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -ErrorAction SilentlyContinue) {
    # Refuse unsupported existing policy before updating the task action.
    Get-CoreServiceSecurityDescriptor -Sddl (
        $scheduler.GetFolder('\').GetTask('ContinuumCore').GetSecurityDescriptor(4)) | Out-Null
}
$action = New-ScheduledTaskAction -Execute $plan.shell -Argument $plan.arguments
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
Register-ScheduledTask -TaskName ContinuumCore -TaskPath '\' -Action $action -Principal $principal `
    -Trigger $trigger -Settings $settings -Description $plan.description -Force | Out-Null

# Registration may run under an elevated token, but routine `continuum start`
# runs as the caller. Task Scheduler's automatic principal ACE guarantees read,
# not execute. Add only read/execute for that SID while preserving existing
# SYSTEM/administrator permissions and any other explicit task ACL entries.
try {
    $registered = $scheduler.GetFolder('\').GetTask('ContinuumCore')
    $updated = Grant-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registered.GetSecurityDescriptor(4)
    $registered.SetSecurityDescriptor($updated, 0)
    if (-not (Test-CoreServiceCallerAccess -UserSid $plan.userSid -Sddl $registered.GetSecurityDescriptor(4))) {
        throw 'Caller read/execute was not saved.'
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
$release = $plan.description | ConvertFrom-Json
if (-not $release.cli) { throw 'The prepared release names no CLI; cannot register the deploy consumer.' }
$deployAction = New-ScheduledTaskAction -Execute $release.cli -Argument 'deploy-consume'
$deployPrincipal = New-ScheduledTaskPrincipal -UserId $plan.userSid -LogonType S4U -RunLevel Limited
$deployTrigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(1) -RepetitionInterval (New-TimeSpan -Minutes 10)
$deploySettings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Hours 4) -MultipleInstances IgnoreNew
Register-ScheduledTask -TaskName ContinuumDeploy -TaskPath '\' -Action $deployAction -Principal $deployPrincipal `
    -Trigger $deployTrigger -Settings $deploySettings `
    -Description 'Continuum deploy consumer: turns a DeployRequest from the Rust tracker into reboot --service. Registered by the installer (register-core-service.ps1); session-independent.' -Force | Out-Null
