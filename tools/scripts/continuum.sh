#!/usr/bin/env bash
# Fix stale cwd (deleted directory, Docker volume removed, etc)
cd "$HOME" 2>/dev/null || true
# continuum — thin wrapper for Docker-based Continuum
#
# Usage:
#   continuum            Open browser (start if not running)
#   continuum up         Start containers + open browser
#   continuum down       Stop containers
#   continuum logs       Tail all logs
#   continuum status     Show container health
#   continuum grid       Grid diagnostics and setup
#   continuum <anything> Forward to jtag inside Docker

set -eo pipefail

# Find docker-compose.yml — check current dir, then known locations
find_compose_dir() {
  if [ -f docker-compose.yml ]; then echo "."; return; fi
  for d in "$HOME/continuum" "$HOME/Development/cambrian/continuum"; do
    [ -f "$d/docker-compose.yml" ] && echo "$d" && return
  done
  echo "❌ Cannot find continuum docker-compose.yml" >&2
  exit 1
}

DIR=$(find_compose_dir)

open_browser() {
  local url="${1:-http://localhost:9003}"
  open "$url" 2>/dev/null || xdg-open "$url" 2>/dev/null || cmd.exe /c start "$url" 2>/dev/null || echo "  Open: $url"
}

# Write Tailscale status to shared file so Docker containers can discover peers.
# Containers can't run `tailscale status` (no CLI) but CAN reach Tailscale IPs.
refresh_tailscale_status() {
  if command -v tailscale &>/dev/null && tailscale status &>/dev/null 2>&1; then
    mkdir -p "$HOME/.continuum/grid"
    tailscale status --json > "$HOME/.continuum/grid/tailscale-status.json" 2>/dev/null || true
  fi
}

# Continuum's grid IS Docker (sandboxed single-machine clusters), so a dead
# or wedged engine is the grid being down — and a normal `docker compose`
# call against a hung daemon just hangs. Run the dependable preflight first:
# it probes the engine and, if it's down, recovers it (force-kills a hung
# Docker Desktop + clean WSL cycle + relaunch on Windows; relaunch/start on
# mac/linux), then verifies — failing LOUD if it truly can't come up.
ensure_docker() {
  local preflight="$DIR/scripts/ensure-docker.sh"
  [ -f "$preflight" ] || return 0   # tolerate older checkouts without the preflight
  bash "$preflight" || {
    echo "❌ Docker engine is not available and could not be recovered — see ensure-docker messages above." >&2
    exit 1
  }
}

case "${1:-}" in
  up|start)
    refresh_tailscale_status
    ensure_docker
    cd "$DIR" && docker compose up -d
    echo "⏳ Waiting for services..."
    for i in $(seq 1 30); do
      docker compose ps widget-server 2>/dev/null | grep -q "healthy" && break
      sleep 2
    done
    echo "✅ Continuum is running"
    # Grid sidecar with HTTPS → use Tailscale domain
    if [ -f "$DIR/.env" ] && grep -q "COMPOSE_PROFILES=grid" "$DIR/.env"; then
      # Wait a moment for Tailscale sidecar to connect
      sleep 3
      TS_DOMAIN=$(docker compose exec -T tailscale tailscale status --json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('Self',{}).get('DNSName','').rstrip('.'))" 2>/dev/null || echo "")
      if [ -n "$TS_DOMAIN" ]; then
        echo "  https://$TS_DOMAIN"
        open_browser "https://$TS_DOMAIN"
      else
        open_browser
      fi
    else
      open_browser
    fi
    ;;
  down|stop)
    cd "$DIR" && docker compose down
    ;;
  logs)
    cd "$DIR" && docker compose logs -f ${2:+$2}
    ;;
  status)
    cd "$DIR" && docker compose ps
    ;;
  grid)
    # Grid management: setup check, enable/disable sidecar, forward grid/* commands
    shift
    GRID_CMD="${1:-}"
    case "$GRID_CMD" in
      check|setup-check|setup|"")
        cd "$DIR"
        if ! command -v tailscale &>/dev/null; then
          echo "Grid requires Tailscale (free, 2 min install)."
          echo ""
          open "https://tailscale.com/download" 2>/dev/null || \
            xdg-open "https://tailscale.com/download" 2>/dev/null || \
            cmd.exe /c start "https://tailscale.com/download" 2>/dev/null || \
            echo "  https://tailscale.com/download"
          echo ""
          echo "After installing, run: continuum grid"
        elif ! tailscale status &>/dev/null 2>&1; then
          echo "Tailscale installed but not connected. Opening..."
          tailscale up 2>/dev/null &
          echo ""
          echo "Log in, then run: continuum grid"
        else
          TS_IP=$(tailscale ip -4 2>/dev/null)
          PEERS=$(tailscale status 2>/dev/null | grep -c " active" 2>/dev/null || true)
          echo "Grid: $TS_IP (${PEERS:-0} peers)"
          echo "  Remote: http://$TS_IP:9003"
          # Show Rust grid status if running
          if docker compose ps node-server 2>/dev/null | grep -q "healthy"; then
            docker exec continuum-node-server-1 ./jtag grid/status 2>/dev/null || true
          fi
        fi
        ;;
      enable)
        cd "$DIR"
        if ! command -v tailscale &>/dev/null; then
          echo "❌ Tailscale not installed. Install first: https://tailscale.com/download"
          exit 1
        fi
        if [ -f "$DIR/.env" ] && grep -q "TS_AUTHKEY" "$DIR/.env"; then
          echo "Grid sidecar already configured in .env"
          echo "Run: continuum grid check"
          exit 0
        fi
        echo "The Docker grid sidecar runs Tailscale inside Docker for HTTPS on port 443."
        echo "You need a Tailscale auth key for the sidecar to join your tailnet."
        echo ""
        echo "  1. https://login.tailscale.com/admin/settings/keys → Generate auth key"
        echo "  2. https://login.tailscale.com/admin/dns → Enable HTTPS Certificates"
        echo ""
        read -p "Paste your auth key (tskey-auth-...): " TS_KEY
        if [[ "$TS_KEY" == tskey-auth-* ]]; then
          TS_HOST="continuum-$(hostname -s 2>/dev/null || hostname | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9-')"
          if [ -f "$DIR/.env" ]; then
            grep -v "^TS_AUTHKEY=\|^TS_HOSTNAME=\|^COMPOSE_PROFILES=" "$DIR/.env" > "$DIR/.env.tmp" || true
            mv "$DIR/.env.tmp" "$DIR/.env"
          fi
          echo "TS_AUTHKEY=$TS_KEY" >> "$DIR/.env"
          echo "TS_HOSTNAME=$TS_HOST" >> "$DIR/.env"
          echo "COMPOSE_PROFILES=grid" >> "$DIR/.env"
          echo ""
          echo "✅ Grid sidecar configured (hostname: $TS_HOST)"
          echo "   Restart to activate: continuum down && continuum up"
        else
          echo "❌ Invalid auth key (must start with tskey-auth-)"
        fi
        ;;
      disable)
        cd "$DIR"
        if [ -f "$DIR/.env" ]; then
          grep -v "^TS_AUTHKEY=\|^TS_HOSTNAME=\|^COMPOSE_PROFILES=" "$DIR/.env" > "$DIR/.env.tmp" || true
          mv "$DIR/.env.tmp" "$DIR/.env"
          echo "✅ Grid sidecar disabled. Restart: continuum down && continuum up"
        else
          echo "Grid sidecar not configured."
        fi
        ;;
      *)
        # Forward grid/* commands to jtag
        cd "$DIR" && docker exec continuum-node-server-1 ./jtag "grid/$GRID_CMD" "${@:2}"
        ;;
    esac
    ;;
  help|-h|--help)
    echo "Usage: continuum [command]"
    echo ""
    echo "  (no args)     Open browser (start if needed)"
    echo "  up            Start containers + open browser"
    echo "  down          Stop containers"
    echo "  logs [svc]    Tail logs"
    echo "  status        Show container health"
    echo "  grid [cmd]    Grid management:"
    echo "    check         Diagnose grid readiness (default)"
    echo "    enable        Configure Docker grid sidecar"
    echo "    disable       Remove grid sidecar config"
    echo "    status        Transport & node status"
    echo "    nodes         List known nodes"
    echo "    discover      Trigger peer discovery"
    echo "  <anything>    Forward to jtag inside Docker"
    ;;
  "")
    # No args: open browser if not already open, start if not running
    refresh_tailscale_status
    ensure_docker
    cd "$DIR"
    if ! docker compose ps widget-server 2>/dev/null | grep -q "healthy"; then
      docker compose up -d
      echo "⏳ Starting..."
      for i in $(seq 1 30); do
        docker compose ps widget-server 2>/dev/null | grep -q "healthy" && break
        sleep 2
      done
      # If grid sidecar is configured, wait for Tailscale to connect
      if [ -f "$DIR/.env" ] && grep -q "COMPOSE_PROFILES=grid" "$DIR/.env"; then
        echo "⏳ Waiting for Tailscale..."
        for i in $(seq 1 15); do
          docker compose exec -T tailscale tailscale status &>/dev/null && break
          sleep 2
        done
      fi
    fi
    # Check if browser already connected (ping returns browser info)
    if docker exec continuum-node-server-1 ./jtag ping 2>/dev/null | grep -q '"url"'; then
      URL=$(docker exec continuum-node-server-1 ./jtag ping 2>/dev/null | grep '"url"' | sed 's/.*"url": "//;s/".*//')
      echo "✅ Continuum is running (browser: $URL)"
    else
      echo "✅ Continuum is running"
      # If grid sidecar is running with Tailscale HTTPS, use that URL
      if [ -f "$DIR/.env" ] && grep -q "COMPOSE_PROFILES=grid" "$DIR/.env" && docker compose ps tailscale 2>/dev/null | grep -q "Up"; then
        TS_HOSTNAME=$(grep TS_HOSTNAME "$DIR/.env" 2>/dev/null | cut -d= -f2)
        TS_DOMAIN=$(docker compose exec -T tailscale tailscale status --json 2>/dev/null | python3 -c "import json,sys; print(json.load(sys.stdin).get('Self',{}).get('DNSName','').rstrip('.'))" 2>/dev/null || echo "")
        if [ -n "$TS_DOMAIN" ]; then
          open_browser "https://$TS_DOMAIN"
        elif [ -n "$TS_HOSTNAME" ]; then
          open_browser "https://$TS_HOSTNAME"
        else
          open_browser
        fi
      else
        open_browser
      fi
    fi
    ;;
  *)
    # Everything else → jtag inside Docker
    cd "$DIR" && docker exec continuum-node-server-1 ./jtag "$@"
    ;;
esac
