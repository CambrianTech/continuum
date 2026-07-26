#!/usr/bin/env bash
# remember.sh — store a durable lesson in the agent's corpus via cu.
#
# Usage: remember.sh "<lesson text>" ["tag1,tag2"]
#
# Fail-LOUD: a /remember that silently no-ops is worse than an error.
#
# ZERO shell JSON — `memory/remember` (M5's #2004, symmetric to recall-hook) takes
# FLAT params and builds+escapes the agent record via serde server-side (uuid,
# timestamp, memory_type=agent, source=agent:<peer>, context{agent_peer_id,session,
# scope}). The `--content` value is passed as one CLI arg, so quotes/newlines in the
# lesson are the shell's problem, not JSON's — no escaping here.
set -uo pipefail

CONTENT="${1:-}"
[ -n "$CONTENT" ] || { echo "remember: nothing to store (usage: remember \"lesson\" [tags])" >&2; exit 1; }
command -v cu >/dev/null 2>&1 || { echo "remember: 'cu' not on PATH (is continuum-core-server installed?)" >&2; exit 1; }

PERSONA="${CONTINUUM_AGENT_PERSONA:-$(airc status 2>/dev/null | awk '/^peer_id:/{print $2; exit}')}"
[ -n "${PERSONA:-}" ] || { echo "remember: could not resolve agent persona (airc peer id)" >&2; exit 1; }
SCOPE="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"

if cu memory/remember --persona_id "$PERSONA" --content "$CONTENT" --scope "$SCOPE" 2>/dev/null | grep -q '"appended"\|"remembered"\|"id"'; then
  echo "remembered (scope: $SCOPE): ${CONTENT:0:80}"
else
  echo "remember: cu memory/remember failed (is the server up? try: cu ping)" >&2
  exit 1
fi
