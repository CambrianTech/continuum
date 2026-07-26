#!/usr/bin/env bash
# session-recall.sh — SessionStart hook for the memory-bridge plugin.
#
# Fires on startup / resume / compact. The `compact` case is the one that matters
# most: after a context-overflow compaction (the exact amnesia event), this
# re-injects the agent's relevant lessons so it does NOT re-forget. `recall-hook`
# defaults max_results low so a compact re-injection can't refill freed context.
#
# Contract: emit the SessionStart context-injection envelope on stdout, or NOTHING.
# Fail-safe — ANY error, missing tool, or unreachable substrate → exit 0 silently.
# A memory system must never break a session.
#
# The envelope JSON is built ENTIRELY by the substrate (serde, via
# `memory/recall-hook`) — NO shell JSON, no jq/python. This script only resolves
# the continuum binary + persona + scope and passes the command's stdout through.
set -uo pipefail

HOOK_INPUT="$(cat 2>/dev/null || true)"  # the hook's stdin payload — carries "source": startup|resume|compact

# Resolve the continuum CLI robustly (NOT bare `cu` — that is the Unix UUCP tool,
# which shadowed us on PATH and silently no-op'd the whole bridge).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"
CONTINUUM="$(resolve_continuum)" || exit 0   # no continuum binary → silent no-op

# Scope recall to the current project (git repo root) when in one, else cwd.
SCOPE_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROJECT="$(basename "$SCOPE_DIR")"

PERSONA="$(resolve_agent_persona)"
[ -n "${PERSONA:-}" ] || exit 0

# Compact-source tuning: a `compact` just FREED context — re-injecting a lot would
# refill exactly what compaction cleared, defeating it. So inject LEAN on compact,
# fuller on a fresh startup/resume. (Pairs with recall_hook.rs's per-bullet cap: this
# bounds the COUNT, that bounds each bullet's LENGTH — together, small × short.)
SOURCE="$(printf '%s' "$HOOK_INPUT" | grep -oE '"source"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed -E 's/.*"([^"]*)"$/\1/')"
if [ "${SOURCE:-}" = "compact" ]; then MAX=3; else MAX=8; fi

# `memory/recall-hook` returns the EXACT {"hookSpecificOutput":{...}} envelope via
# serde (valid, escaped, empty-safe). Pass it straight through. Any failure → nothing.
"$CONTINUUM" memory/recall-hook --persona_id "$PERSONA" --room_id "$PROJECT" --query_text "$PROJECT session context" --max_results "$MAX" 2>/dev/null || true
exit 0
