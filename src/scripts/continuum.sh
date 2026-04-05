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
#   continuum <anything> Forward to jtag inside Docker

set -eo pipefail

# Find docker-compose.yml — check current dir, then known locations
find_compose_dir() {
  if [ -f docker-compose.yml ]; then echo "."; return; fi
  for d in "$HOME/continuum" "$HOME/Development/cambrian/continuum" "/Volumes/FlashGordon/cambrian/continuum"; do
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

case "${1:-}" in
  up|start)
    cd "$DIR" && docker compose up -d
    echo "⏳ Waiting for services..."
    for i in $(seq 1 30); do
      docker compose ps widget-server 2>/dev/null | grep -q "healthy" && break
      sleep 2
    done
    echo "✅ Continuum is running"
    open_browser
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
  help|-h|--help)
    echo "Usage: continuum [command]"
    echo ""
    echo "  (no args)     Open browser (start if needed)"
    echo "  up            Start containers + open browser"
    echo "  down          Stop containers"
    echo "  logs [svc]    Tail logs"
    echo "  status        Show container health"
    echo "  <anything>    Forward to jtag inside Docker"
    ;;
  "")
    # No args: open browser if not already open, start if not running
    cd "$DIR"
    if ! docker compose ps widget-server 2>/dev/null | grep -q "healthy"; then
      docker compose up -d
      echo "⏳ Starting..."
      for i in $(seq 1 30); do
        docker compose ps widget-server 2>/dev/null | grep -q "healthy" && break
        sleep 2
      done
    fi
    # Check if browser already connected (ping returns browser info)
    if docker exec continuum-node-server-1 ./jtag ping 2>/dev/null | grep -q '"url"'; then
      URL=$(docker exec continuum-node-server-1 ./jtag ping 2>/dev/null | grep '"url"' | sed 's/.*"url": "//;s/".*//')
      echo "✅ Continuum is running (browser: $URL)"
    else
      echo "✅ Continuum is running"
      open_browser
    fi
    ;;
  *)
    # Everything else → jtag inside Docker
    cd "$DIR" && docker exec continuum-node-server-1 ./jtag "$@"
    ;;
esac
