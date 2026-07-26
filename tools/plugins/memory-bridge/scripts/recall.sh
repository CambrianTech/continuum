#!/usr/bin/env bash
# recall.sh — explicit memory search via the continuum CLI (backs the /recall skill).
# Prints the recalled lessons' content, most-relevant first.
set -uo pipefail

QUERY="${*:-}"
[ -n "$QUERY" ] || { echo "recall: provide a query" >&2; exit 1; }

# Resolve the continuum CLI robustly (NOT bare `cu` — collides with Unix UUCP).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"
CONTINUUM="$(resolve_continuum)" || { echo "recall: 'continuum' CLI not found (build the core, or set CONTINUUM_BIN)" >&2; exit 1; }

PERSONA="$(resolve_agent_persona)"
[ -n "${PERSONA:-}" ] || { echo "recall: could not resolve agent persona (airc peer id)" >&2; exit 1; }
SCOPE="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"

OUT="$("$CONTINUUM" memory/multi-layer-recall --persona_id "$PERSONA" --query_text "$QUERY" --room_id "$SCOPE" --max_results 8 2>/dev/null || true)"
LINES="$(printf '%s' "$OUT" | grep -aE '"content"' | sed -E 's/.*"content": *"(.*)"[,]?[[:space:]]*$/- \1/')"
if [ -n "$LINES" ]; then
  printf '%s\n' "$LINES"
else
  echo "recall: no memories matched \"$QUERY\" (scope: $SCOPE)"
fi
