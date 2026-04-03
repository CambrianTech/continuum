#!/usr/bin/env bash
# open.sh — Find and open Continuum in your browser.
#
# Usage:
#   ./open.sh              # open local (localhost:9003)
#   ./open.sh bigmama      # open a grid node by name
#   ./open.sh grid         # list all grid nodes, pick one

set -euo pipefail

open_url() {
  local url="$1"
  echo "✅ $url"
  case "$(uname -s)" in
    Darwin)  open "$url" ;;
    Linux)
      if grep -qi microsoft /proc/version 2>/dev/null; then
        /mnt/c/Windows/explorer.exe "$url"
      elif command -v xdg-open &>/dev/null; then
        xdg-open "$url"
      fi ;;
  esac
}

# No args = local
if [ $# -eq 0 ]; then
  open_url "http://localhost:9003"
  exit 0
fi

# "grid" = list all nodes
if [ "$1" = "grid" ]; then
  SUFFIX=$(tailscale status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('MagicDNSSuffix',''))" 2>/dev/null)
  echo "🔍 Grid nodes on your tailnet:"
  echo ""
  tailscale status 2>/dev/null | while read -r ip name rest; do
    echo "  $name  →  https://${name}.${SUFFIX}"
  done
  echo ""
  echo "Usage: ./open.sh <nodename>"
  exit 0
fi

# Named node = find on tailnet and open
NODE="$1"
TAILNET_SUFFIX=$(tailscale status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('MagicDNSSuffix',''))" 2>/dev/null)

if [ -z "$TAILNET_SUFFIX" ]; then
  echo "❌ Tailscale not running. For local: ./open.sh"
  exit 1
fi

open_url "https://${NODE}.${TAILNET_SUFFIX}"
