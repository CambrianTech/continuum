#!/bin/bash
# wsl-boot.sh — Auto-start services inside WSL2 on Windows boot
# Installed by: install.sh (WSL2 path)
# Called by: Windows Scheduled Task "ContinuumWSL"
#
# This script runs as root inside WSL2 when Windows boots.

LOG="/var/log/continuum-boot.log"
echo "$(date): Continuum WSL boot starting" >> "$LOG"

# 1. Start SSH
if command -v sshd &>/dev/null; then
    service ssh start 2>/dev/null || /usr/sbin/sshd 2>/dev/null
    echo "$(date): SSH started (port 22)" >> "$LOG"
fi

# 2. Start Tailscale
if command -v tailscale &>/dev/null; then
    # Start daemon if not running
    if ! pgrep -x tailscaled &>/dev/null; then
        tailscaled --state=/var/lib/tailscale/tailscaled.state &
        sleep 3
    fi
    tailscale up --ssh --accept-routes 2>>"$LOG"
    TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "unknown")
    echo "$(date): Tailscale up ($TAILSCALE_IP)" >> "$LOG"
fi

# 3. Protect critical services from OOM killer
for service in sshd tailscaled; do
    for pid in $(pgrep -x "$service" 2>/dev/null); do
        echo -1000 > "/proc/$pid/oom_score_adj" 2>/dev/null
    done
done
echo "$(date): OOM protection set for sshd, tailscaled" >> "$LOG"

# 4. Verify GPU access
if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
    GPU=$(/usr/lib/wsl/lib/nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null || nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null || echo "not detected")
    echo "$(date): GPU: $GPU" >> "$LOG"
fi

# 5. Start PostgreSQL if installed
if command -v pg_isready &>/dev/null; then
    service postgresql start 2>/dev/null || true
    echo "$(date): PostgreSQL started" >> "$LOG"
fi

echo "$(date): Continuum WSL boot complete" >> "$LOG"
