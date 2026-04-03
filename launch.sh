#!/usr/bin/env bash
# launch.sh — Start Continuum and open the browser.
#
# Usage:
#   ./launch.sh              # local mode (http://localhost:9003)
#   ./launch.sh --grid       # grid mode (https://nodename.tailnet)
#
# This runs on the HOST machine, not inside Docker.
# It starts containers, waits for healthy, detects the URL, and opens the browser.

set -euo pipefail

COMPOSE_CMD="docker compose"

# Detect grid mode from args or COMPOSE_PROFILES
GRID_MODE=false
if [[ "${1:-}" == "--grid" ]] || [[ "${COMPOSE_PROFILES:-}" == *grid* ]]; then
  GRID_MODE=true
fi

echo "🚀 Starting Continuum..."
$COMPOSE_CMD up -d

# Wait for node-server to be healthy
echo "⏳ Waiting for server..."
RETRIES=60
while [ $RETRIES -gt 0 ]; do
  STATUS=$($COMPOSE_CMD ps node-server --format '{{.Health}}' 2>/dev/null || echo "starting")
  if [ "$STATUS" = "healthy" ]; then
    break
  fi
  RETRIES=$((RETRIES - 1))
  sleep 2
done

if [ $RETRIES -eq 0 ]; then
  echo "❌ Server did not become healthy within 120s"
  echo "   Run 'docker compose logs node-server' to debug"
  exit 1
fi

# Detect the access URL
if [ "$GRID_MODE" = true ]; then
  # Grid mode: get Tailscale FQDN from the tailscale container
  TS_FQDN=$($COMPOSE_CMD exec -T tailscale tailscale status --json 2>/dev/null \
    | grep -o '"DNSName":"[^"]*"' | head -1 | cut -d'"' -f4 | sed 's/\.$//')

  if [ -n "$TS_FQDN" ]; then
    URL="https://${TS_FQDN}"
  else
    # Fallback: try to read from .env
    TS_HOST=$(grep TS_HOSTNAME .env 2>/dev/null | cut -d= -f2)
    if [ -n "$TS_HOST" ]; then
      URL="https://${TS_HOST}.ts.net"
      echo "⚠️  Could not detect full Tailscale domain. Guessing: $URL"
    else
      URL="http://localhost:9003"
      echo "⚠️  Grid mode but no Tailscale detected. Using localhost."
    fi
  fi
else
  # Local mode: plain HTTP on localhost
  URL="http://localhost:9003"
fi

echo ""
echo "✅ Continuum ready at: $URL"
echo ""

# Open the browser (platform-aware)
open_browser() {
  local url="$1"
  case "$(uname -s)" in
    Darwin)
      open "$url"
      ;;
    Linux)
      # Check for WSL
      if grep -qi microsoft /proc/version 2>/dev/null; then
        /mnt/c/Windows/explorer.exe "$url"
      elif command -v xdg-open &>/dev/null; then
        xdg-open "$url"
      else
        echo "   Open in your browser: $url"
        return
      fi
      ;;
    *)
      echo "   Open in your browser: $url"
      return
      ;;
  esac
  echo "🌐 Browser opened"
}

# Only auto-open if running locally (not over SSH)
if [ -z "${SSH_CONNECTION:-}" ] && [ -z "${SSH_TTY:-}" ]; then
  open_browser "$URL"
else
  echo "   (Remote session detected — open this URL on your local machine)"
fi
