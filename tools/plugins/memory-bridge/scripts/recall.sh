#!/usr/bin/env bash
# recall.sh — explicit memory search via cu (backs the /recall skill).
# Prints the recalled lessons' content, most-relevant first.
set -uo pipefail

QUERY="${*:-}"
[ -n "$QUERY" ] || { echo "recall: provide a query" >&2; exit 1; }
command -v cu >/dev/null 2>&1 || { echo "recall: 'cu' not on PATH (is continuum-core-server installed?)" >&2; exit 1; }

PERSONA="${CONTINUUM_AGENT_PERSONA:-$(airc status 2>/dev/null | awk '/^peer_id:/{print $2; exit}')}"
[ -n "${PERSONA:-}" ] || { echo "recall: could not resolve agent persona (airc peer id)" >&2; exit 1; }
SCOPE="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"

OUT="$(cu memory/multi-layer-recall --persona_id "$PERSONA" --query_text "$QUERY" --room_id "$SCOPE" --max_results 8 2>/dev/null || true)"
LINES="$(printf '%s' "$OUT" | grep -aE '"content"' | sed -E 's/.*"content": *"(.*)"[,]?[[:space:]]*$/- \1/')"
if [ -n "$LINES" ]; then
  printf '%s\n' "$LINES"
else
  echo "recall: no memories matched \"$QUERY\" (scope: $SCOPE)"
fi
