#!/bin/bash
# install-tailscale.sh — Standalone Tailscale setup
# Run independently: bash scripts/install-tailscale.sh
# Called by install.sh but testable on its own.
set -e
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Setting up Tailscale...${NC}"

# WSL2 + Windows-side Tailscale detection (issue #952).
# If this is WSL2 and the Windows host already has Tailscale live, we have
# two potential tailnet identities on one physical machine ("bigmama" on
# Windows + "bigmama-1" on WSL2). For continuum's grid, ONE is canonical
# and it's this one (WSL2): the Docker daemon runs here, and peer agents
# reach this box's SSH endpoint — Windows-side Tailscale can't route
# traffic to WSL2 services without extra port-proxy config. By default we
# proceed with the WSL2 install but WARN loud so Carl understands the
# dual-identity footgun and uninstalls Windows-side or accepts that only
# the WSL2 identity is reachable for grid use. Escape hatch:
# CONTINUUM_GRID_NODE=windows skips the WSL2 install entirely (rare).
if grep -qi microsoft /proc/version 2>/dev/null || [ -n "${WSL_DISTRO_NAME:-}" ]; then
  WIN_TS_EXE="/mnt/c/Program Files/Tailscale/tailscale.exe"
  if [ -x "$WIN_TS_EXE" ] && timeout 3 "$WIN_TS_EXE" status >/dev/null 2>&1; then
    WIN_TS_IP=$(timeout 3 "$WIN_TS_EXE" ip -4 2>/dev/null | head -1 || echo "")
    echo -e "${YELLOW}⚠️  Windows-side Tailscale detected (live${WIN_TS_IP:+, IP: $WIN_TS_IP}).${NC}"
    echo -e "  You're about to install Tailscale on WSL2 too, which creates a SECOND tailnet"
    echo -e "  identity on this one physical machine. For continuum's grid, WSL2 is canonical"
    echo -e "  (Docker daemon + SSH endpoint live here), so the WSL2 identity is what peers"
    echo -e "  will actually reach."
    echo -e ""
    echo -e "  Recommended fixes:"
    echo -e "    • Uninstall Windows-side Tailscale (Settings → Apps) before re-running this install."
    echo -e "    • OR accept dual-identity but understand only the WSL2 one matters for grid."
    echo -e "    • OR set ${GREEN}CONTINUUM_GRID_NODE=windows${NC} and re-run to use Windows-side"
    echo -e "      (skips WSL2 install; you're responsible for port-proxying WSL2 services"
    echo -e "      out through the Windows Tailscale IP yourself)."
    echo -e ""
    if [ "${CONTINUUM_GRID_NODE:-}" = "windows" ]; then
      echo -e "${GREEN}  CONTINUUM_GRID_NODE=windows set — skipping WSL2 install, using Windows-side.${NC}"
      exit 0
    fi
    echo -e "${YELLOW}  Proceeding with WSL2 install (default). Warning surfaced; you decided.${NC}"
    echo -e ""
  fi
fi

# 1. Install if missing
if ! command -v tailscale &>/dev/null; then
  echo -e "  Installing Tailscale..."
  curl -fsSL https://tailscale.com/install.sh | sh
fi

# 2. Kill any stuck daemons AND clean stale socket
if pgrep -x tailscaled &>/dev/null; then
  echo -e "  Killing existing tailscaled processes..."
  sudo kill -9 $(pgrep tailscaled) 2>/dev/null || true
  sleep 2
fi
# Remove stale socket — "address already in use" means old socket wasn't cleaned up
sudo rm -f /var/run/tailscale/tailscaled.sock 2>/dev/null || true

# 3. Set up passwordless sudo (so it auto-starts on boot without prompting)
if [ ! -f /etc/sudoers.d/tailscale ]; then
  echo -e "  Setting up passwordless sudo for tailscale..."
  echo "$USER ALL=(ALL) NOPASSWD: /usr/bin/tailscale, /usr/bin/tailscaled, /usr/sbin/tailscaled" | sudo tee /etc/sudoers.d/tailscale > /dev/null
  sudo chmod 440 /etc/sudoers.d/tailscale
fi

# 4. Start daemon fresh
echo -e "  Starting tailscaled..."
sudo tailscaled --state=/var/lib/tailscale/tailscaled.state &
DAEMON_PID=$!

# 5. Wait for socket (not just process — socket must be ready)
echo -e "  Waiting for daemon socket..."
for i in $(seq 1 30); do
  if timeout 2 tailscale status &>/dev/null; then
    echo -e "  ${GREEN}Daemon ready (${i}s)${NC}"
    break
  fi
  sleep 1
done

# 6. Check if already authenticated. If so, also confirm Tailscale SSH is
# enabled — without --ssh, peer machines can't reach this host without
# per-device OpenSSH keys. The most common breakage is a user running
# plain `tailscale up` later (e.g. after a reboot or a network change),
# which RESETS configured flags including --ssh. Detect that case and
# re-add --ssh idempotently.
TS_IP=$(tailscale ip -4 2>/dev/null || echo "")
if [ -n "$TS_IP" ]; then
  echo -e "  ${GREEN}✅ Tailscale connected: ${TS_IP}${NC}"
  # Probe the running prefs for --ssh. The exact JSON path is
  # .Prefs.RunSSH on recent tailscale versions; older may be .RunSSH.
  TS_SSH_ON=$(tailscale debug prefs 2>/dev/null | python3 -c "
import sys, json
try:
    p = json.load(sys.stdin)
    # newer schemas: top-level RunSSH; older: nested under Prefs
    print('true' if (p.get('RunSSH') or p.get('Prefs', {}).get('RunSSH')) else 'false')
except Exception:
    print('unknown')
" 2>/dev/null)
  if [ "$TS_SSH_ON" = "true" ]; then
    echo -e "  ${GREEN}  Tailscale SSH already enabled. Auto-reconnects on reboot. Done.${NC}"
    exit 0
  fi
  # SSH not enabled (or probe inconclusive). Re-run `up --ssh` to add the
  # flag. This preserves every other flag the user has set (advertise-
  # routes, accept-routes, etc.) and is idempotent — no browser prompt
  # if already authenticated.
  echo -e "  ${YELLOW}⚠️  Tailscale SSH not enabled (status: $TS_SSH_ON).${NC}"
  echo -e "  ${YELLOW}  Enabling now so peers on the Tailnet can SSH in without per-device keys...${NC}"
  if sudo tailscale up --ssh --accept-routes 2>&1; then
    echo -e "  ${GREEN}✅ Tailscale SSH enabled. Done.${NC}"
  else
    echo -e "  ${RED}❌ Failed to enable Tailscale SSH. Run manually:${NC}"
    echo -e "       sudo tailscale up --ssh --accept-routes"
    exit 1
  fi
  exit 0
fi

# 7. Auth required — show URL clearly
echo -e ""
echo -e "  ${YELLOW}══════════════════════════════════════════════════════${NC}"
echo -e "  ${YELLOW}  OPEN THE URL BELOW IN YOUR BROWSER${NC}"
echo -e "  ${YELLOW}  Sign in with Google. One-time only.${NC}"
echo -e "  ${YELLOW}══════════════════════════════════════════════════════${NC}"
echo -e ""

sudo tailscale up --ssh --accept-routes

# 8. Verify
TS_IP=$(tailscale ip -4 2>/dev/null || echo "")
if [ -n "$TS_IP" ]; then
  echo -e ""
  echo -e "  ${GREEN}✅ Tailscale connected: ${TS_IP}${NC}"
  echo -e "  ${GREEN}  This node is on the mesh. Auto-reconnects forever.${NC}"
else
  echo -e "  ${RED}❌ Tailscale auth failed.${NC}"
  exit 1
fi

# 9. Enable systemd service if available
if command -v systemctl &>/dev/null && systemctl is-system-running &>/dev/null 2>&1; then
  sudo systemctl enable tailscaled 2>/dev/null || true
fi
