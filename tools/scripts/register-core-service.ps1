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
$principal = New-ScheduledTaskPrincipal -UserId $plan.userSid -LogonType Interactive -RunLevel Limited
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $plan.userSid
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
