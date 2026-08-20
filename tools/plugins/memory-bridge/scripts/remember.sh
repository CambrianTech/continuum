#!/usr/bin/env bash
# remember.sh — store a durable lesson in the agent's corpus via the continuum CLI.
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

# Resolve the continuum CLI robustly (NOT bare `cu` — collides with Unix UUCP).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"
CONTINUUM="$(resolve_continuum)" || { echo "remember: 'continuum' CLI not found (build the core, or set CONTINUUM_BIN)" >&2; exit 1; }

PERSONA="$(resolve_agent_persona)"
[ -n "${PERSONA:-}" ] || { echo "remember: could not resolve agent persona (airc peer id)" >&2; exit 1; }
SCOPE="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"

# BOUND THE CALL AND KEEP THE REAL ERROR.
#
# This was `... 2>/dev/null | grep -q ...` with no timeout, and it had two
# defects that compounded into "the memory system silently does nothing":
#
#   1. NO TIMEOUT. `memory/remember` dispatches to a running core. With no core
#      up, the CLI blocks indefinitely, so this script HUNG instead of failing.
#      Measured 2026-08-13: an agent's `remember` sat 300s and was killed by the
#      caller's timeout (exit 143). Nothing stored, nothing reported, and the
#      lesson it was preserving was the one about not hand-rolling around
#      governed paths.
#   2. `2>/dev/null` DISCARDED THE CAUSE. The fallback line could only GUESS
#      ("is the server up?"), so the one diagnostic that would have named it —
#      the CLI's own stderr — was thrown away on every failure.
#
# A memory tool that fails silently is worse than none: the agent believes the
# correction is durable, stops carrying it in context, and re-learns it next
# session. That is the whole failure mode this plugin exists to prevent.
#
# 20s is generous for a local IPC round-trip and short enough that a missing
# core is reported while the operator is still watching.
REMEMBER_TIMEOUT="${REMEMBER_TIMEOUT:-20}"
err_file="$(mktemp)"
trap 'rm -f "$err_file"' EXIT

if out="$(timeout "$REMEMBER_TIMEOUT" "$CONTINUUM" memory/remember \
      --persona_id "$PERSONA" --content "$CONTENT" --scope "$SCOPE" 2>"$err_file")" \
   && printf '%s' "$out" | grep -q '"appended"\|"remembered"\|"id"'; then
  echo "remembered (scope: $SCOPE): ${CONTENT:0:80}"
else
  status=$?
  err="$(tr -d '\r' < "$err_file" | tail -3)"
  if [ "$status" -eq 124 ]; then
    echo "remember: TIMED OUT after ${REMEMBER_TIMEOUT}s — the continuum core did not answer." >&2
    echo "  NOTHING WAS STORED. Start the core (npm start), then re-run." >&2
  else
    echo "remember: continuum memory/remember FAILED (exit $status) — NOTHING WAS STORED." >&2
    [ -n "$err" ] && echo "  cause: $err" >&2
    echo "  check the core is up: continuum ping" >&2
  fi
  exit 1
fi
