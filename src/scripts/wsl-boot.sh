#!/bin/bash
# wsl-boot.sh — Auto-start services inside WSL2 on Windows boot
# Installed by: install.sh (WSL2 path)
# Called by: Windows Scheduled Task "ContinuumWSL"
#
# This script runs as root inside WSL2 when Windows boots.

LOG="/var/log/continuum-boot.log"
echo "$(date): Continuum WSL boot starting" >> "$LOG"

# 0. Fix DNS
# WSL2 auto-generates resolv.conf pointing at Tailscale DNS (10.255.255.254) or
# a broken nameserver. External DNS (8.8.8.8) doesn't work because WSL2's NAT
# blocks outbound UDP port 53. The fix: use the LAN gateway (router) which is
# always reachable from WSL2.
GATEWAY=$(ip route show default 2>/dev/null | awk '{print $3}')
if [ -n "$GATEWAY" ]; then
    # Disable auto-generation permanently
    if ! grep -q "generateResolvConf" /etc/wsl.conf 2>/dev/null; then
        echo -e "\n[network]\ngenerateResolvConf = false" >> /etc/wsl.conf
    fi
    echo "nameserver $GATEWAY" > /etc/resolv.conf
    echo "$(date): DNS fixed (gateway $GATEWAY, auto-generation disabled)" >> "$LOG"
else
    # Fallback: try common router IPs
    for ns in 192.168.1.1 192.168.0.1 10.0.0.1; do
        if ping -c 1 -W 1 "$ns" &>/dev/null; then
            echo "nameserver $ns" > /etc/resolv.conf
            echo "$(date): DNS fixed (fallback $ns)" >> "$LOG"
            break
        fi
    done
fi

# 1. Start SSH
if command -v sshd &>/dev/null; then
    service ssh start 2>/dev/null || /usr/sbin/sshd 2>/dev/null
    echo "$(date): SSH started (port 22)" >> "$LOG"
fi

# 2. Start Tailscale
# PREREQUISITE (one-time, run manually at the tower):
#   echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/tailscale, /usr/bin/tailscaled" | sudo tee /etc/sudoers.d/tailscale
#   sudo systemctl enable tailscaled
if command -v tailscale &>/dev/null; then
    # Start daemon if not running (needs sudo on WSL2)
    if ! pgrep -x tailscaled &>/dev/null; then
        if sudo -n true 2>/dev/null; then
            sudo tailscaled --state=/var/lib/tailscale/tailscaled.state &
            sleep 5
        else
            echo "$(date): WARNING: tailscaled needs sudo but no passwordless sudo configured" >> "$LOG"
            echo "$(date): Run: echo '\$USER ALL=(ALL) NOPASSWD: /usr/bin/tailscale, /usr/bin/tailscaled' | sudo tee /etc/sudoers.d/tailscale" >> "$LOG"
        fi
    fi
    if pgrep -x tailscaled &>/dev/null; then
        sudo tailscale up --ssh --accept-routes 2>>"$LOG" || tailscale up --ssh --accept-routes 2>>"$LOG"
        TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "unknown")
        echo "$(date): Tailscale up ($TAILSCALE_IP)" >> "$LOG"
    fi
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
