#!/usr/bin/env bash
# session-recall.sh — SessionStart hook for the memory-bridge plugin.
#
# Fires on startup / resume / compact. The `compact` case is the one that matters
# most: after a context-overflow compaction (the exact amnesia event), this
# re-injects the agent's relevant lessons so it does NOT re-forget. `recall-hook`
# defaults max_results low so a compact re-injection can't refill freed context.
#
# Contract: emit a SessionStart context-injection envelope on stdout. NEVER break
# a session (always exit 0) — but NEVER hide a failure either. Joel (2026-08-05):
# "you idiots are trained to hide errors" / "I won't know you aren't using it,
# nor will your dumbass." A silent no-op makes "installed" and "working"
# indistinguishable, which is how this bridge sat dead for weeks. So every failure
# path here does two things instead of vanishing:
#   1. writes a durable receipt (bridge_receipt → ~/.continuum/memory-bridge/receipts.jsonl)
#   2. INJECTS THE FAILURE into the agent's own context, in the exact place it
#      already reads memories — so a broken bridge announces itself every session.
# It also reports the PREVIOUS session's capture health for the same reason: the
# Stop hook has no channel to the agent, so its failures surface here.
set -uo pipefail

HOOK_INPUT="$(cat 2>/dev/null || true)"  # the hook's stdin payload — carries "source": startup|resume|compact

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"

# emit_notice <text> — hand-built envelope for the FAILURE path only. The success
# path never builds JSON in shell: `memory/recall-hook` emits it via serde. This
# escapes the two characters that can appear in our own fixed-shape messages.
emit_notice() {
  local text="${1:-}"
  text="${text//\\/\\\\}"; text="${text//\"/\\\"}"; text="${text//$'\n'/\\n}"
  printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$text"
}

CONTINUUM="$(resolve_continuum)" || {
  bridge_receipt session-recall failed "continuum binary not found (PATH + cargo-target both missed)"
  emit_notice "⚠️ MEMORY BRIDGE DOWN — recall did not run: the \`continuum\` binary could not be resolved (not on PATH, not in the cargo target dir). Your engram memory is NOT loaded this session; treat yourself as amnesiac and say so rather than assuming recall works. Fix: build the core, or set CONTINUUM_BIN."
  exit 0
}

SCOPE_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROJECT="$(basename "$SCOPE_DIR")"

PERSONA="$(resolve_agent_persona)" || {
  bridge_receipt session-recall failed "persona id unresolved (airc status down AND no cached id)"
  emit_notice "⚠️ MEMORY BRIDGE DOWN — recall did not run: could not resolve this agent's persona id (\`airc status\` gave nothing and no cached id exists at ~/.continuum/memory-bridge/persona-id). Your engram memory is NOT loaded this session. Fix: start airc, or set CONTINUUM_AGENT_PERSONA."
  exit 0
}
[ -n "${PERSONA:-}" ] || {
  bridge_receipt session-recall failed "persona id empty"
  emit_notice "⚠️ MEMORY BRIDGE DOWN — recall did not run: this agent's persona id resolved EMPTY. Your engram memory is NOT loaded this session."
  exit 0
}

# Compact-source tuning: a `compact` just FREED context — re-injecting a lot would
# refill exactly what compaction cleared, defeating it. So inject LEAN on compact,
# fuller on a fresh startup/resume. (Pairs with recall_hook.rs's per-bullet cap: this
# bounds the COUNT, that bounds each bullet's LENGTH — together, small × short.)
SOURCE="$(printf '%s' "$HOOK_INPUT" | grep -oE '"source"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed -E 's/.*"([^"]*)"$/\1/')"
if [ "${SOURCE:-}" = "compact" ]; then MAX=3; else MAX=8; fi

# `memory/recall-hook` returns the EXACT {"hookSpecificOutput":{...}} envelope via
# serde (valid, escaped, empty-safe). Capture it so we can tell "worked" from
# "the core refused" — the old version piped straight through with `|| true`,
# which made an unreachable core look identical to a healthy empty recall.
ERR_FILE="$(mktemp 2>/dev/null || echo /tmp/memory-bridge-recall.$$)"
OUT="$("$CONTINUUM" memory/recall-hook \
        --persona_id "$PERSONA" --room_id "$PROJECT" \
        --query_text "$PROJECT session context" --max_results "$MAX" 2>"$ERR_FILE")"
RC=$?
ERR="$(head -c 300 "$ERR_FILE" 2>/dev/null | tr '\n' ' ')"
rm -f "$ERR_FILE" 2>/dev/null

if [ $RC -ne 0 ] || [ -z "$OUT" ]; then
  bridge_receipt session-recall failed "recall-hook rc=$RC ${ERR:-no stderr}"
  emit_notice "⚠️ MEMORY BRIDGE DOWN — recall ran but returned nothing (exit $RC). ${ERR:-The core may be unreachable.} Your engram memory is NOT loaded this session; do not assume past lessons are in context. Check: \`continuum memory/recall-hook --persona_id $PERSONA --room_id $PROJECT --query_text test\`."
  exit 0
fi

bridge_receipt session-recall ok "source=${SOURCE:-startup} max=$MAX bytes=${#OUT}"
printf '%s\n' "$OUT"

# The Stop hook (capture) has no channel to the agent — its failures would be
# invisible forever. Surface the last capture receipt here, where the agent reads.
LAST_CAPTURE="$(grep -a '"hook":"session-capture"' "$BRIDGE_STATE_DIR/receipts.jsonl" 2>/dev/null | tail -1)"
case "$LAST_CAPTURE" in
  *'"status":"ok"'*|'') : ;;  # healthy, or no captures yet (fresh install — the next turn writes one)
  *) emit_notice "⚠️ MEMORY CAPTURE FAILING — the last Stop-hook capture did not store: $(printf '%s' "$LAST_CAPTURE" | head -c 240). Turns are NOT being recorded to your corpus right now. Say so rather than assuming this session is being remembered." ;;
esac

exit 0
