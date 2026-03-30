#!/bin/bash
# install-tailscale.sh — Standalone Tailscale setup
# Run independently: bash scripts/install-tailscale.sh
# Called by install.sh but testable on its own.
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${YELLOW}Setting up Tailscale...${NC}"

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

# 6. Check if already authenticated
TS_IP=$(tailscale ip -4 2>/dev/null || echo "")
if [ -n "$TS_IP" ]; then
  echo -e "  ${GREEN}✅ Tailscale connected: ${TS_IP}${NC}"
  echo -e "  ${GREEN}  Auto-reconnects on reboot. Done.${NC}"
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
