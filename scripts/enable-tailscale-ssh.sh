#!/usr/bin/env bash
# enable-tailscale-ssh.sh — one-time-setup, idempotent.
#
# Run this on a host (BigMama, dev box, anything you want others to reach)
# and from then on, any device on your Tailnet can SSH in WITHOUT a
# per-device key. Tailscale handles auth via your Tailnet identity + ACLs
# instead of OpenSSH's per-device authorized_keys.
#
# Why this exists: managing OpenSSH authorized_keys across devices is a
# perpetual paper cut (new Mac → new key → manual paste, every time). On
# Windows it's worse — admin users need C:\ProgramData\ssh\
# administrators_authorized_keys with the right ACL. Tailscale SSH skips
# the whole mess.
#
# Usage:
#   bash scripts/enable-tailscale-ssh.sh
#
# Windows host: run from WSL2 OR from Git Bash. For the PowerShell-only
# path see scripts/enable-tailscale-ssh.ps1.
#
# What it does:
#   1. Confirms `tailscale` CLI is installed and the daemon is up
#   2. Runs `tailscale up --ssh` (the magic flag — preserves all existing
#      flags, just adds --ssh; safe to re-run)
#   3. Reports the host's Tailscale IP so you can hand it to a teammate

set -euo pipefail

# Find the tailscale CLI. On Linux/WSL2 it's on PATH. On macOS it's bundled
# in the .app. On Windows-from-WSL2 it's typically reachable via the host's
# C:\Program Files\Tailscale\tailscale.exe through interop, but we prefer
# the WSL2-native one if the user installed it there.
if command -v tailscale &>/dev/null; then
  TS=tailscale
elif [[ -x "/Applications/Tailscale.app/Contents/MacOS/Tailscale" ]]; then
  TS="/Applications/Tailscale.app/Contents/MacOS/Tailscale"
elif [[ -x "/mnt/c/Program Files/Tailscale/tailscale.exe" ]]; then
  TS="/mnt/c/Program Files/Tailscale/tailscale.exe"
else
  cat >&2 <<EOF
ERROR: tailscale CLI not found.
  Install: https://tailscale.com/download
  Then re-run this script.
EOF
  exit 1
fi

echo "→ tailscale CLI: $TS"

# Confirm the daemon is reachable. If `tailscale status` errors, the
# daemon isn't running OR you're not logged in yet — surface the actual
# error rather than swallow it.
if ! "$TS" status >/dev/null 2>&1; then
  echo "→ tailscale daemon not responding. Running 'tailscale status' for diagnosis:"
  "$TS" status >&2 || true
  echo ""
  echo "Most likely fix: open the Tailscale app (or run 'tailscale up' once" >&2
  echo "to authenticate this machine). Then re-run this script." >&2
  exit 1
fi

# The actual fix. `tailscale up --ssh` is idempotent and preserves all
# previously-set flags (advertise-routes, accept-routes, etc.). The
# --reset flag is intentionally NOT used here — we only want to ADD --ssh.
echo "→ Enabling Tailscale SSH (idempotent, preserves other flags)..."
"$TS" up --ssh

# Confirm the change took
HOSTNAME_RAW="$(hostname 2>/dev/null || echo unknown)"
TS_IP="$("$TS" ip -4 2>/dev/null | head -1)"

cat <<EOF

✓ Tailscale SSH enabled on this host.
  hostname:     $HOSTNAME_RAW
  tailscale ip: $TS_IP

Teammates on your Tailnet can now reach this host with:

  tailscale ssh <user>@$HOSTNAME_RAW
  # or by IP:
  tailscale ssh <user>@$TS_IP

No per-device SSH keys needed — Tailnet identity + ACL is the auth.

If a teammate still gets "No ED25519 host key is known", give it ~10
seconds for the host key to propagate via Tailscale's coordination
server, then retry.
EOF
