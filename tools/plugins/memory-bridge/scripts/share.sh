#!/usr/bin/env bash
# share.sh — hand a durable lesson to ANOTHER agent's corpus (engram handoff / telepathy).
#
# Usage: share.sh "<recipient peer id or airc name>" "<lesson text>"
#
# The cross-agent twin of remember.sh: where /remember writes a lesson into YOUR own
# corpus (self-learned), /share writes it into the RECIPIENT's corpus with shared-by
# provenance (memory_type=shared, source=shared:<you>), so a lesson you learned once
# lands in another agent's memory without them re-deriving it. Their recall surfaces it,
# tagged as received-from-you.
#
# Fail-LOUD: a /share that silently no-ops is worse than an error.
#
# ZERO shell JSON — `memory/share` takes FLAT params and builds+escapes the record via
# serde server-side. Lesson quotes/newlines are the shell's problem (one CLI arg), not JSON's.
set -uo pipefail

RECIPIENT="${1:-}"
CONTENT="${2:-}"
if [ -z "$RECIPIENT" ] || [ -z "$CONTENT" ]; then
  echo "share: usage: share \"<recipient peer id or airc name>\" \"<lesson>\"" >&2
  echo "  find recipients with: airc peers" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"
CONTINUUM="$(resolve_continuum)" || { echo "share: 'continuum' CLI not found (build the core, or set CONTINUUM_BIN)" >&2; exit 1; }

FROM="$(resolve_agent_persona)"
[ -n "${FROM:-}" ] || { echo "share: could not resolve THIS agent's persona (airc peer id)" >&2; exit 1; }

# Resolve the recipient. A full UUID is used verbatim; otherwise treat it as an airc
# name and look up its peer id via `airc whois` (falling back to a `airc peers` scan).
# Fail loud if it can't be resolved — sharing to a phantom peer is a silent black hole.
resolve_recipient() {
  local raw="$1"
  if printf '%s' "$raw" | grep -qiE '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'; then
    printf '%s' "$raw"; return 0
  fi
  # Name → peer id via airc, best-effort (whois first, then a peers-table scan).
  local id
  id="$(airc whois "$raw" 2>/dev/null | grep -oiE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)"
  [ -z "$id" ] && id="$(airc peers 2>/dev/null | grep -iF "$raw" | grep -oiE '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}' | head -1)"
  printf '%s' "$id"
}

TO="$(resolve_recipient "$RECIPIENT")"
[ -n "${TO:-}" ] || { echo "share: could not resolve recipient '$RECIPIENT' to a peer id (try: airc peers)" >&2; exit 1; }

SCOPE="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"

if "$CONTINUUM" memory/share --from_persona_id "$FROM" --to_persona_id "$TO" --content "$CONTENT" --scope "$SCOPE" 2>/dev/null | grep -q '"appended"\|"id"'; then
  echo "shared to $TO (scope: $SCOPE): ${CONTENT:0:80}"
else
  echo "share: continuum memory/share failed (is the server up? try: continuum ping)" >&2
  exit 1
fi
