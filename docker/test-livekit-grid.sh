#!/bin/sh
# Diagnostic script: test LiveKit grid mode assumptions
#
# Run on BigMama (or any Docker host with Tailscale sidecar):
#   cd /path/to/continuum
#   bash docker/test-livekit-grid.sh
#
# Tests each assumption independently so we know exactly what failed.

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo "${GREEN}PASS${NC}: $1"; }
fail() { echo "${RED}FAIL${NC}: $1"; }
info() { echo "${YELLOW}INFO${NC}: $1"; }

echo "═══════════════════════════════════════════════"
echo "  LiveKit Grid Mode — Assumption Diagnostics"
echo "═══════════════════════════════════════════════"
echo ""

# ── Step 0: Ensure tailscale container is running ────────────
info "Step 0: Starting tailscale container..."
docker compose --profile grid up -d tailscale 2>&1

# Wait for tailscale to be ready (has a tailnet IP)
info "Waiting for tailscale to join tailnet (up to 30s)..."
TAILSCALE_CONTAINER=$(docker compose ps -q tailscale 2>/dev/null)
if [ -z "$TAILSCALE_CONTAINER" ]; then
  fail "Tailscale container not found. Is the grid profile configured?"
  exit 1
fi

ATTEMPTS=0
while [ $ATTEMPTS -lt 30 ]; do
  TS_STATUS=$(docker exec "$TAILSCALE_CONTAINER" tailscale status --json 2>/dev/null || true)
  if echo "$TS_STATUS" | grep -q '"Online":true' 2>/dev/null; then
    break
  fi
  ATTEMPTS=$((ATTEMPTS + 1))
  sleep 1
done

if [ $ATTEMPTS -ge 30 ]; then
  fail "Tailscale didn't come online in 30s"
  info "Container logs:"
  docker compose logs tailscale --tail=20
  exit 1
fi

TS_IP=$(docker exec "$TAILSCALE_CONTAINER" tailscale ip -4 2>/dev/null || true)
TS_HOSTNAME=$(docker exec "$TAILSCALE_CONTAINER" tailscale status --json 2>/dev/null | grep -o '"DNSName":"[^"]*"' | head -1 || true)
pass "Tailscale online — IP: ${TS_IP}, ${TS_HOSTNAME}"
echo ""

# ── Test 1: Can a shared-namespace container see tailscale0? ──
echo "── Test 1: Network namespace sharing (tailscale0 visible?) ──"
INTERFACES=$(docker run --rm --network "container:${TAILSCALE_CONTAINER}" alpine ip addr 2>&1)
echo "$INTERFACES" | head -30

if echo "$INTERFACES" | grep -q "tailscale0"; then
  TS0_LINE=$(echo "$INTERFACES" | grep -A 2 "tailscale0" | grep "inet ")
  pass "tailscale0 interface exists: ${TS0_LINE}"
else
  fail "tailscale0 interface NOT found in shared namespace"
  info "Available interfaces:"
  echo "$INTERFACES" | grep -E "^[0-9]+:" | sed 's/^/  /'
  info "This might mean TS_USERSPACE=true (no kernel TUN device)"
  info "Or the interface has a different name. Check 'ip addr' output above."
  echo ""
  echo "── Possible fix: Check if userspace mode creates a different interface ──"
  docker exec "$TAILSCALE_CONTAINER" ls -la /dev/net/tun 2>&1 || info "/dev/net/tun not found"
  exit 1
fi
echo ""

# ── Test 2: Can we extract the Tailscale IP from tailscale0? ──
echo "── Test 2: IP discovery from tailscale0 interface ──"
DISCOVERED_IP=$(docker run --rm --network "container:${TAILSCALE_CONTAINER}" alpine sh -c \
  "ip addr show tailscale0 | grep 'inet ' | awk '{print \$2}' | cut -d/ -f1" 2>&1)

if [ -n "$DISCOVERED_IP" ] && echo "$DISCOVERED_IP" | grep -qE '^[0-9]+\.[0-9]+'; then
  pass "Discovered IP from tailscale0: ${DISCOVERED_IP}"
  if [ "$DISCOVERED_IP" = "$TS_IP" ]; then
    pass "Matches tailscale ip -4 output"
  else
    fail "MISMATCH: tailscale0 says ${DISCOVERED_IP}, tailscale ip -4 says ${TS_IP}"
  fi
else
  fail "Could not extract IP from tailscale0"
  info "Raw output: ${DISCOVERED_IP}"
fi
echo ""

# ── Test 3: Can we bind a port on the shared namespace? ──
echo "── Test 3: Port binding in shared namespace ──"
# Start a simple TCP listener on port 7890 (livekit's signaling port)
docker run -d --rm --name test-listener --network "container:${TAILSCALE_CONTAINER}" \
  alpine sh -c "nc -l -p 7890 -e echo hello" 2>/dev/null || true

# Try to connect from the tailscale container itself (localhost)
CONNECT_RESULT=$(docker exec "$TAILSCALE_CONTAINER" sh -c \
  "echo test | nc -w 2 127.0.0.1 7890 2>&1" 2>&1 || true)

docker stop test-listener 2>/dev/null || true

if echo "$CONNECT_RESULT" | grep -q "hello"; then
  pass "localhost:7890 reachable within shared namespace"
else
  info "nc connect result: ${CONNECT_RESULT}"
  info "This tests if Tailscale serve can proxy to localhost:7890"
  info "(May fail if nc isn't available — not critical)"
fi
echo ""

# ── Test 4: Is port reachable from Tailscale IP? ──
echo "── Test 4: Tailscale IP direct port access (7881 simulation) ──"
# Start a listener on 7881 in the shared namespace
docker run -d --rm --name test-rtc --network "container:${TAILSCALE_CONTAINER}" \
  alpine sh -c "nc -l -p 7881 -k" 2>/dev/null || true
sleep 1

# Try connecting from the HOST to the Tailscale IP on 7881
# This simulates what a remote browser would do for WebRTC
if [ -n "$TS_IP" ]; then
  RTC_RESULT=$(nc -z -w 2 "$TS_IP" 7881 2>&1 && echo "open" || echo "closed")
  if [ "$RTC_RESULT" = "open" ]; then
    pass "Tailscale IP ${TS_IP}:7881 reachable from host (WebRTC path works)"
  else
    fail "Tailscale IP ${TS_IP}:7881 NOT reachable from host"
    info "This means WebRTC media won't reach LiveKit"
    info "Check: is Tailscale ACL blocking? Is the host on the same tailnet?"
  fi
else
  info "Skipped (no Tailscale IP)"
fi

docker stop test-rtc 2>/dev/null || true
echo ""

# ── Test 5: Tailscale serve config validation ──
echo "── Test 5: Tailscale serve status ──"
SERVE_STATUS=$(docker exec "$TAILSCALE_CONTAINER" tailscale serve status 2>&1 || true)
echo "$SERVE_STATUS" | head -20
if echo "$SERVE_STATUS" | grep -q "7880"; then
  pass "Port 7880 is in serve config"
else
  info "Port 7880 not in serve output (may need reload)"
fi
echo ""

# ── Test 6: Full livekit-grid startup ──
echo "── Test 6: LiveKit grid container startup ──"
info "Starting livekit-grid..."
docker compose --profile grid up -d livekit-grid 2>&1

info "Waiting for livekit-grid to start (up to 90s)..."
ATTEMPTS=0
while [ $ATTEMPTS -lt 90 ]; do
  LK_LOGS=$(docker compose logs livekit-grid --tail=5 2>/dev/null)
  if echo "$LK_LOGS" | grep -q "LiveKit: Starting with --node-ip"; then
    pass "livekit-grid started"
    echo "$LK_LOGS" | grep "LiveKit:" | sed 's/^/  /'
    break
  fi
  if echo "$LK_LOGS" | grep -q "ERROR"; then
    fail "livekit-grid hit an error"
    docker compose logs livekit-grid --tail=20
    break
  fi
  ATTEMPTS=$((ATTEMPTS + 1))
  sleep 1
done

if [ $ATTEMPTS -ge 90 ]; then
  fail "livekit-grid didn't start in 90s"
  docker compose logs livekit-grid --tail=30
fi
echo ""

# ── Test 7: LiveKit signaling via Tailscale serve ──
echo "── Test 7: WSS signaling through Tailscale serve ──"
if [ -n "$TS_IP" ]; then
  # Try HTTP (not HTTPS) to the serve port — should get redirected or respond
  SIGNAL_RESULT=$(curl -sk --max-time 5 "https://${TS_IP}:7880/" 2>&1 || true)
  if echo "$SIGNAL_RESULT" | grep -qi "livekit\|upgrade\|websocket\|404\|not found"; then
    pass "LiveKit signaling reachable via Tailscale serve (https://${TS_IP}:7880)"
  else
    info "Response from https://${TS_IP}:7880: $(echo "$SIGNAL_RESULT" | head -3)"
    info "Expected some HTTP response (even an error). Empty = TLS/proxy issue."
  fi
else
  info "Skipped (no Tailscale IP)"
fi
echo ""

# ── Summary ──
echo "═══════════════════════════════════════════════"
echo "  Diagnostic complete. Review PASS/FAIL above."
echo ""
echo "  To clean up:  docker compose --profile grid down"
echo "  To see logs:  docker compose --profile grid logs -f"
echo "═══════════════════════════════════════════════"
