#!/bin/bash
# open-ui.sh — the ONE automatic door into the positronic interface.
#
# Why this exists: the core (`continuum start`) serves the thin-client WS (8974) + call/video
# WS (8790), and `apps/web` is the built UI — but nothing tied them together, so finding "how
# do I open the interface" required archaeology (which is exactly how both a user AND an agent
# get lost). This script is the automatic entry: it resolves the core's WS ports + a stable
# identity, serves the built UI, and opens the browser with everything pre-wired. The web
# client stays fail-loud (no invented config); THIS supplies the config.
#
# Usage:  npm run ui   (or: bash tools/scripts/open-ui.sh)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

CONFIG_ENV="$HOME/.continuum/config.env"
read_cfg() { [ -f "$CONFIG_ENV" ] && grep -E "^$1=" "$CONFIG_ENV" 2>/dev/null | tail -1 | cut -d= -f2- || true; }

# 1) Core WS ports — from config.env (single owner), else the conventional local defaults.
CORE_WS_PORT="$(read_cfg CONTINUUM_CORE_WS)"; CORE_WS_PORT="${CORE_WS_PORT:-8974}"
CALL_WS_PORT="$(read_cfg CONTINUUM_CALL_WS)"; CALL_WS_PORT="${CALL_WS_PORT:-8790}"

# 2) Is the core actually up + serving the WS? Fail loud with the fix, never open a dead UI.
if ! nc -z 127.0.0.1 "$CORE_WS_PORT" 2>/dev/null; then
  echo "✗ core WS not listening on 127.0.0.1:$CORE_WS_PORT — start the core first:  continuum start" >&2
  echo "  (the core must run with CONTINUUM_CORE_WS set; default $CORE_WS_PORT)" >&2
  exit 1
fi

# 3) Stable identity for `me=`. Prefer config.env; else a persisted guest uuid (stable across
#    runs, so the same browser is the same citizen) until identity pairing (#37/#38) lands.
ME="$(read_cfg CONTINUUM_USER_ID)"
if [ -z "$ME" ]; then
  ME_FILE="$HOME/.continuum/ui-guest-id"
  [ -f "$ME_FILE" ] || uuidgen | tr 'A-Z' 'a-z' > "$ME_FILE"
  ME="$(cat "$ME_FILE")"
fi

# 4) ALWAYS rebuild the UI from current source before serving. A vite build is ~100ms, and
#    serving a STALE dist is exactly how the interface looks "lost" — an old build predating
#    a week of shell work renders the wrong thing (glass-boxed 2026-07-28: a Jul-18 dist showed
#    a bare chat view, not the current positron HUD). Fresh-by-construction, never stale.
echo "→ building the positron UI from current source…"
npm run build -w @continuum/web >/dev/null 2>&1 || { echo "✗ UI build failed — run: npm run build -w @continuum/web" >&2; exit 1; }

# 5) Serve the built dist on a fixed port (idempotent — reuse if already up).
UI_PORT="${CONTINUUM_UI_PORT:-5177}"
if ! nc -z localhost "$UI_PORT" 2>/dev/null; then
  ( npm run preview -w @continuum/web -- --port "$UI_PORT" --strictPort >/dev/null 2>&1 & )
  for _ in $(seq 1 20); do nc -z localhost "$UI_PORT" 2>/dev/null && break; sleep 0.3; done
fi

URL="http://localhost:$UI_PORT/?core=ws://127.0.0.1:$CORE_WS_PORT&call=ws://127.0.0.1:$CALL_WS_PORT&me=$ME"
echo "🖥  Positronic interface: $URL"
case "$(uname -s)" in
  Darwin) open "$URL" ;;
  Linux)  command -v xdg-open >/dev/null && xdg-open "$URL" || echo "open this URL in your browser ↑" ;;
  *)      echo "open this URL in your browser ↑" ;;
esac
