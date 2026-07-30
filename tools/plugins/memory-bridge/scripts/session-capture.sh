#!/usr/bin/env bash
# session-capture.sh — Stop hook: AUTOMATIC per-turn episodic capture.
#
# Joel (2026-07-30): "That you manually call it and it doesn't just operate
# means it sucks and is useless." Volitional memory isn't memory. This hook
# fires after EVERY assistant turn and appends the turn's final message to the
# agent's corpus with zero volition — the agent never decides to remember.
# Mirrors the persona architecture: turns record automatically; consolidation
# (`memory/consolidate`) and decay (#221) are substrate jobs that curate later.
# Low importance (0.3) so semantic recall ranks deliberate lessons above
# routine chatter; `/remember` (importance 0.8) stays the emphasis channel.
#
# Contract: silent no-op on ANY failure (missing tool, dead core, unreadable
# transcript). A memory system must never break a turn. No hook output —
# Stop hooks that emit nothing let the turn end normally.
set -uo pipefail

HOOK_INPUT="$(cat 2>/dev/null || true)"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"
CONTINUUM="$(resolve_continuum)" || exit 0
PERSONA="$(resolve_agent_persona)" || exit 0
[ -n "${PERSONA:-}" ] || exit 0

SCOPE_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROJECT="$(basename "$SCOPE_DIR")"

# Pull the transcript path from the hook payload, then the last assistant
# message text from the JSONL transcript. python3 parses; it never BUILDS
# JSON for the wire (the CLI's flat --key value coercion does the escaping).
CONTENT="$(printf '%s' "$HOOK_INPUT" | python3 -c '
import json, sys
try:
    payload = json.load(sys.stdin)
    path = payload.get("transcript_path", "")
    last = ""
    with open(path) as f:
        for line in f:
            try:
                entry = json.loads(line)
            except Exception:
                continue
            msg = entry.get("message") or {}
            if msg.get("role") != "assistant":
                continue
            parts = msg.get("content") or []
            texts = [p.get("text", "") for p in parts if isinstance(p, dict) and p.get("type") == "text"]
            if texts:
                last = "\n".join(t for t in texts if t)
    out = last.strip()
    if len(out) > 900:
        out = out[:900] + " …[truncated]"
    print(out)
except Exception:
    pass
' 2>/dev/null || true)"

# Nothing said (pure tool turn) → nothing to capture.
[ -n "$CONTENT" ] || exit 0

"$CONTINUUM" memory/remember \
  --persona_id "$PERSONA" \
  --scope "$PROJECT" \
  --importance 0.3 \
  --content "[session turn] $CONTENT" >/dev/null 2>&1 || true

exit 0
