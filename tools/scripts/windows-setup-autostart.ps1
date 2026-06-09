# windows-setup-autostart.ps1 — Run ONCE as admin to make WSL2 auto-start on boot
#
# Usage: Right-click PowerShell → "Run as administrator" → paste:
#   Set-ExecutionPolicy Bypass -Scope Process; .\windows-setup-autostart.ps1
#
# What it does:
#   1. Copies wsl-boot.sh into WSL2
#   2. Creates a Windows Scheduled Task that starts WSL2 on boot
#   3. The boot script starts SSH, Tailscale, and protects them from OOM
#   4. After this, the machine survives reboots without human intervention

Write-Host "Setting up Continuum auto-start..." -ForegroundColor Cyan

# 1. Check we're running as admin
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")
if (-not $isAdmin) {
    Write-Host "ERROR: This script must be run as Administrator." -ForegroundColor Red
    Write-Host "Right-click PowerShell → 'Run as administrator'" -ForegroundColor Yellow
    exit 1
}

# 2. Check WSL2 is installed
$wslCheck = wsl --list --quiet 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: WSL2 not installed. Run: wsl --install" -ForegroundColor Red
    exit 1
}
Write-Host "  WSL2: OK" -ForegroundColor Green

# 3. Copy boot script into WSL2
$bootScript = @"
#!/bin/bash
LOG="/var/log/continuum-boot.log"
echo "`$(date): Continuum WSL boot starting" >> "`$LOG"

# Start SSH
service ssh start 2>/dev/null || /usr/sbin/sshd 2>/dev/null
echo "`$(date): SSH started" >> "`$LOG"

# Start Tailscale
if command -v tailscale &>/dev/null; then
    if ! pgrep -x tailscaled &>/dev/null; then
        tailscaled --state=/var/lib/tailscale/tailscaled.state &
        sleep 3
    fi
    tailscale up --ssh --accept-routes 2>>"`$LOG"
    echo "`$(date): Tailscale up (`$(tailscale ip -4 2>/dev/null))" >> "`$LOG"
fi

# OOM protection for SSH and Tailscale
for service in sshd tailscaled; do
    for pid in `$(pgrep -x "`$service" 2>/dev/null); do
        echo -1000 > "/proc/`$pid/oom_score_adj" 2>/dev/null
    done
done

# Start PostgreSQL
service postgresql start 2>/dev/null || true

# GPU check
GPU=`$(/usr/lib/wsl/lib/nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null || echo "none")
echo "`$(date): Boot complete. GPU: `$GPU" >> "`$LOG"
"@

# Write to WSL filesystem
$bootScript | wsl bash -c "cat > /etc/continuum-boot.sh && chmod +x /etc/continuum-boot.sh"
Write-Host "  Boot script: /etc/continuum-boot.sh" -ForegroundColor Green

# 4. Configure wsl.conf to run boot script
wsl bash -c "grep -q 'continuum-boot' /etc/wsl.conf 2>/dev/null || echo '[boot]
command=/etc/continuum-boot.sh' | sudo tee -a /etc/wsl.conf > /dev/null"
Write-Host "  wsl.conf: configured" -ForegroundColor Green

# 5. Create Windows Scheduled Task
$taskName = "ContinuumWSL"
$existingTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue

if ($existingTask) {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    Write-Host "  Removed existing task" -ForegroundColor Yellow
}

$action = New-ScheduledTaskAction -Execute "wsl.exe" -Argument "-u root -- /etc/continuum-boot.sh"
$trigger = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId "SYSTEM" -RunLevel Highest -LogonType ServiceAccount
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable

Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Description "Start WSL2 with SSH, Tailscale, and GPU access on boot"
Write-Host "  Scheduled Task: $taskName (runs at boot as SYSTEM)" -ForegroundColor Green

Write-Host ""
Write-Host "Done! This machine will now auto-start WSL2 on every boot." -ForegroundColor Cyan
Write-Host "  SSH, Tailscale, and PostgreSQL start automatically."
Write-Host "  SSH and Tailscale are protected from OOM killer."
Write-Host "  Log: /var/log/continuum-boot.log (inside WSL)"
Write-Host ""
Write-Host "Test it: restart Windows and SSH in after 30 seconds." -ForegroundColor Yellow
