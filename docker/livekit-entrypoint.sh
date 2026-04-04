#!/bin/sh
# LiveKit entrypoint for Docker grid mode (network_mode: service:tailscale)
#
# Waits for the tailscale0 interface to come up, discovers its IPv4 address,
# and launches LiveKit with --node-ip set to the Tailscale IP.
# This ensures ICE candidates advertise the correct IP for WebRTC media.

set -e

echo "LiveKit: Waiting for tailscale0 interface..."

ATTEMPTS=0
MAX_ATTEMPTS=60

while ! ip addr show tailscale0 2>/dev/null | grep -q "inet "; do
  ATTEMPTS=$((ATTEMPTS + 1))
  if [ "$ATTEMPTS" -ge "$MAX_ATTEMPTS" ]; then
    echo "LiveKit: ERROR — tailscale0 interface not found after ${MAX_ATTEMPTS}s"
    echo "LiveKit: Is the Tailscale container running? (docker compose --profile grid up)"
    exit 1
  fi
  sleep 1
done

TS_IP=$(ip addr show tailscale0 | grep 'inet ' | awk '{print $2}' | cut -d/ -f1)
echo "LiveKit: Discovered Tailscale IP: ${TS_IP}"
echo "LiveKit: Starting with --node-ip ${TS_IP}"

exec /livekit-server --config /etc/livekit.yaml --node-ip "${TS_IP}"
