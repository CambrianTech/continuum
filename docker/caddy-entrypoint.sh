#!/bin/sh
# Auto-discover TLS certs from /certs/ and start Caddy.
# Certs are Tailscale-provisioned: <hostname>.crt and <hostname>.key

CRT=$(ls /certs/*.crt 2>/dev/null | head -1)
KEY=$(ls /certs/*.key 2>/dev/null | head -1)

if [ -z "$CRT" ] || [ -z "$KEY" ]; then
  echo "ERROR: No TLS certs found in /certs/. LiveKit TLS proxy cannot start."
  echo "Run: tailscale cert --cert-file ~/.continuum/<hostname>.crt --key-file ~/.continuum/<hostname>.key <hostname>"
  exit 1
fi

echo "LiveKit TLS proxy: $CRT -> livekit:7880"
export TLS_CERT="$CRT"
export TLS_KEY="$KEY"

exec caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
