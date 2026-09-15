param([Parameter(Mandatory = $true)][string]$PlanPath)
$ErrorActionPreference = 'Stop'
$plan = Get-Content -LiteralPath $PlanPath -Raw | ConvertFrom-Json
if (-not $plan.userSid -or -not $plan.shell -or -not $plan.arguments -or -not $plan.description) {
    throw 'Incomplete service registration plan.'
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
$scheduler = New-Object -ComObject 'Schedule.Service'
$scheduler.Connect()
$registered = $scheduler.GetFolder('\').GetTask('ContinuumCore')
$security = [Security.AccessControl.CommonSecurityDescriptor]::new(
    $false, $false, $registered.GetSecurityDescriptor(4)) # DACL_SECURITY_INFORMATION
$caller = [Security.Principal.SecurityIdentifier]::new([string]$plan.userSid)
$readExecute = 0x001200a9 # FILE_GENERIC_READ | FILE_GENERIC_EXECUTE (SDDL FRFX)
$security.DiscretionaryAcl.AddAccess(
    [Security.AccessControl.AccessControlType]::Allow, $caller, $readExecute,
    [Security.AccessControl.InheritanceFlags]::None,
    [Security.AccessControl.PropagationFlags]::None)
$registered.SetSecurityDescriptor(
    $security.GetSddlForm([Security.AccessControl.AccessControlSections]::Access), 0)
