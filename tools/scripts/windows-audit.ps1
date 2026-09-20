#requires -RunAsAdministrator
<#
.SYNOPSIS
    Full autostart + popup audit for Windows. Surfaces EVERY way something
    can launch at boot or login on this machine, so a human can identify
    and kill anything malware-shaped.

.DESCRIPTION
    Companion to the malware-perception fix in PR #1608. Run this on any
    Windows box where Settings UI freezes, terminals pop on login, or
    shutdowns hang — it audits:

      1. Scheduled tasks with logon/startup/boot triggers
      2. HKLM + HKCU Run / RunOnce / WOW6432Node Run keys
      3. User + system Startup folders
      4. Auto-start services (filtered to non-Microsoft paths)
      5. Win32_StartupCommand (Task Manager → Startup view)
      6. Currently running suspicious processes with parent + command line

    Then dumps a one-liner kill list of anything matching known continuum /
    airc / cargo / wsl signatures, ready for the operator to copy-paste.

.NOTES
    Read-only by default. Prints a separate "kill suggestions" block at the
    end — operator runs those manually after reviewing output.

    No telemetry. No network calls. No remediation without consent.
#>

$ErrorActionPreference = 'SilentlyContinue'
$divider = "=" * 78

function Write-Section($title, $color = 'Cyan') {
    Write-Host ""
    Write-Host $divider -ForegroundColor $color
    Write-Host "  $title" -ForegroundColor $color
    Write-Host $divider -ForegroundColor $color
}

# ----------------------------------------------------------------------
Write-Section "1. SCHEDULED TASKS (logon / startup / boot triggers)"
# ----------------------------------------------------------------------
Get-ScheduledTask | Where-Object {
    $_.Triggers | Where-Object {
        $_.CimClass.CimClassName -match 'LogonTrigger|BootTrigger|StartupTrigger'
    }
} | ForEach-Object {
    $exec = ($_.Actions | Select-Object -First 1).Execute
    $args = ($_.Actions | Select-Object -First 1).Arguments
    [PSCustomObject]@{
        Name    = $_.TaskName
        Path    = $_.TaskPath
        Author  = $_.Author
        State   = $_.State
        RunAs   = $_.Principal.UserId
        Command = "$exec $args".Trim()
    }
} | Format-Table -AutoSize -Wrap

# ----------------------------------------------------------------------
Write-Section "2. REGISTRY RUN KEYS (HKLM + HKCU + RunOnce + WOW6432)"
# ----------------------------------------------------------------------
$runKeys = @(
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Run',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\RunOnce',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run',
    'HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce'
)
$runEntries = foreach ($k in $runKeys) {
    $items = Get-ItemProperty $k
    if ($items) {
        $items.PSObject.Properties | Where-Object { $_.Name -notmatch '^PS' } | ForEach-Object {
            [PSCustomObject]@{
                Hive    = ($k -split '\\')[0]
                Key     = ($k -split '\\', 2)[1]
                Name    = $_.Name
                Command = $_.Value
            }
        }
    }
}
$runEntries | Format-Table -AutoSize -Wrap

# ----------------------------------------------------------------------
Write-Section "3. STARTUP FOLDERS (user + all-users)"
# ----------------------------------------------------------------------
@(
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup",
    "$env:ALLUSERSPROFILE\Microsoft\Windows\Start Menu\Programs\Startup"
) | ForEach-Object {
    if (Test-Path $_) {
        Write-Host "  $_" -ForegroundColor Gray
        Get-ChildItem $_ -File | Select-Object Name, LastWriteTime, Length | Format-Table -AutoSize
    }
}

# ----------------------------------------------------------------------
Write-Section "4. AUTO-START SERVICES (filtered to non-Microsoft paths)"
# ----------------------------------------------------------------------
Get-CimInstance Win32_Service | Where-Object {
    $_.StartMode -eq 'Auto' -and
    $_.PathName -notmatch '\\Windows\\system32\\|\\Windows\\Microsoft\.NET\\|^"?C:\\Windows\\' -and
    $_.PathName -notmatch 'svchost'
} | Select-Object Name, DisplayName, State, PathName | Format-Table -AutoSize -Wrap

# ----------------------------------------------------------------------
Write-Section "5. Win32_StartupCommand (Task Manager → Startup view)"
# ----------------------------------------------------------------------
Get-CimInstance Win32_StartupCommand |
    Select-Object Name, Command, Location, User |
    Format-Table -AutoSize -Wrap

# ----------------------------------------------------------------------
Write-Section "6. CURRENTLY RUNNING SUSPICIOUS PROCESSES" 'Yellow'
# ----------------------------------------------------------------------
$suspiciousNames = 'cmd\.exe|powershell\.exe|pwsh\.exe|wsl\.exe|conhost\.exe|airc\.exe|continuum'
Get-CimInstance Win32_Process | Where-Object {
    $_.Name -match $suspiciousNames
} | ForEach-Object {
    $parent = (Get-CimInstance Win32_Process -Filter "ProcessId=$($_.ParentProcessId)").Name
    [PSCustomObject]@{
        PID         = $_.ProcessId
        Name        = $_.Name
        Parent      = "$parent ($($_.ParentProcessId))"
        StartTime   = $_.CreationDate
        CommandLine = $_.CommandLine
    }
} | Sort-Object StartTime | Format-Table -AutoSize -Wrap

# ----------------------------------------------------------------------
Write-Section "7. KILL SUGGESTIONS (copy-paste only after review)" 'Red'
# ----------------------------------------------------------------------
$ourPattern = 'airc|continuum|wsl|cargo|tailscale'

# Tasks
$ourTasks = Get-ScheduledTask | Where-Object {
    ($_.Triggers | Where-Object { $_.CimClass.CimClassName -match 'LogonTrigger|BootTrigger|StartupTrigger' }) -and
    ($_.TaskName -match $ourPattern -or
     ($_.Actions | Select-Object -First 1).Execute -match $ourPattern -or
     $_.Author -match $ourPattern)
}
if ($ourTasks) {
    Write-Host "  # Scheduled tasks suspected to be ours (autostart triggers):" -ForegroundColor Yellow
    foreach ($t in $ourTasks) {
        Write-Host "    Unregister-ScheduledTask -TaskName '$($t.TaskName)' -TaskPath '$($t.TaskPath)' -Confirm:`$false"
    }
}

# Registry Run keys
$ourRun = $runEntries | Where-Object { $_.Name -match $ourPattern -or $_.Command -match $ourPattern }
if ($ourRun) {
    Write-Host "`n  # Registry Run entries suspected to be ours:" -ForegroundColor Yellow
    foreach ($r in $ourRun) {
        Write-Host "    Remove-ItemProperty -Path '$($r.Hive)\$($r.Key)' -Name '$($r.Name)'"
    }
}

# Startup folder entries
$ourStartup = @(
    "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup",
    "$env:ALLUSERSPROFILE\Microsoft\Windows\Start Menu\Programs\Startup"
) | ForEach-Object {
    if (Test-Path $_) { Get-ChildItem $_ -File | Where-Object { $_.Name -match $ourPattern } }
}
if ($ourStartup) {
    Write-Host "`n  # Startup folder entries suspected to be ours:" -ForegroundColor Yellow
    foreach ($f in $ourStartup) {
        Write-Host "    Remove-Item '$($f.FullName)'"
    }
}

if (-not ($ourTasks -or $ourRun -or $ourStartup)) {
    Write-Host "  No autostart entries matching airc/continuum/wsl/cargo/tailscale found." -ForegroundColor Green
    Write-Host "  If terminals are STILL popping at login, look at section 5 + 6 output —" -ForegroundColor Green
    Write-Host "  the culprit is something we don't recognize from our naming patterns." -ForegroundColor Green
}

# ----------------------------------------------------------------------
Write-Section "8. WSL SHUTDOWN (recommended before any reboot)" 'Magenta'
# ----------------------------------------------------------------------
Write-Host "  If WSL is running, shutdowns will hang waiting for in-distro daemons" -ForegroundColor Magenta
Write-Host "  (tailscaled, sshd, postgres) to exit. Drain it first:" -ForegroundColor Magenta
Write-Host ""
Write-Host "    wsl --shutdown" -ForegroundColor Yellow
Write-Host ""

Write-Host $divider -ForegroundColor Cyan
Write-Host "  Audit complete. Review sections 1–6, then run section 7 commands" -ForegroundColor Cyan
Write-Host "  individually after confirming each entry is unwanted." -ForegroundColor Cyan
Write-Host $divider -ForegroundColor Cyan
