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
# Contract: a Stop hook must not break a turn, so it always exits 0 and emits no
# output. That is NOT a licence to swallow errors — the previous version ended
# every command with `|| true`, so an unreachable core produced a hook that
# "ran fine" and stored nothing, indefinitely. Joel (2026-08-05): "you idiots are
# trained to hide errors." Now every path — success and failure — writes a durable
# receipt, and session-recall.sh reads the latest one back INTO THE AGENT'S CONTEXT
# at the next session start. Silent to the turn, loud to the record.
set -uo pipefail

HOOK_INPUT="$(cat 2>/dev/null || true)"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=/dev/null
. "$SCRIPT_DIR/lib.sh"

CONTINUUM="$(resolve_continuum)" || {
  bridge_receipt session-capture failed "continuum binary not found"
  exit 0
}
PERSONA="$(resolve_agent_persona)" || {
  bridge_receipt session-capture failed "persona id unresolved (airc down AND no cached id)"
  exit 0
}
[ -n "${PERSONA:-}" ] || { bridge_receipt session-capture failed "persona id empty"; exit 0; }

SCOPE_DIR="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
PROJECT="$(basename "$SCOPE_DIR")"

# Pull the transcript path from the hook payload, then the last assistant
# message text from the JSONL transcript. Python parses; it never BUILDS
# JSON for the wire (the CLI's flat --key value coercion does the escaping).
# Every I/O boundary below pins UTF-8 EXPLICITLY. Python's `open()`, `sys.stdin`
# and `sys.stdout` all default to the platform's preferred encoding — UTF-8 on
# Linux/macOS but cp1252 on Windows — so a transcript containing an em-dash or a
# star (i.e. every real transcript) raised UnicodeDecodeError on Windows ONLY.
# This is the same defect class as the `python3` alias resolved just below:
# the hook worked on the Macs and was silently dead on the Windows node.
# Resolve a Python that RUNS, not one that merely EXISTS. On Windows `python3`
# is a Microsoft Store "App Execution Alias": a real file on PATH that ignores
# its arguments, prints an install ad, and exits 49. So `command -v python3`
# SUCCEEDS on a box with no python3, and this hook failed every turn on such a
# box with "Python was not found" landing in the receipt — capture silently off
# on the machine that most needed it. Presence is not liveness; probe by
# EXECUTING a sentinel.
# Order: python3 first (right on Linux/macOS, where bare `python` may be absent
# or Python 2), then python, then the Windows `py` launcher.
_probe_py() { [ "$("$@" -c 'print("ok")' 2>/dev/null)" = "ok" ]; }
if   _probe_py python3; then BRIDGE_PY=(python3)
elif _probe_py python;  then BRIDGE_PY=(python)
elif _probe_py py -3;   then BRIDGE_PY=(py -3)
else
    bridge_receipt session-capture failed "no working python (python3, python, py -3 all failed to execute)"
    exit 0
fi

CONTENT="$(printf '%s' "$HOOK_INPUT" | "${BRIDGE_PY[@]}" -c '
import json, sys
try:
    payload = json.loads(sys.stdin.buffer.read().decode("utf-8", "replace"))
    path = payload.get("transcript_path", "")
    last = ""
    with open(path, encoding="utf-8", errors="replace") as f:
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
    sys.stdout.buffer.write(out.encode("utf-8"))
except Exception as e:
    print("__CAPTURE_ERROR__" + type(e).__name__ + ": " + str(e)[:120], file=sys.stderr)
' 2>"${TMPDIR:-/tmp}/memory-bridge-capture.$$.err")"
PY_ERR="$(head -c 200 "${TMPDIR:-/tmp}/memory-bridge-capture.$$.err" 2>/dev/null | tr '\n' ' ')"
rm -f "${TMPDIR:-/tmp}/memory-bridge-capture.$$.err" 2>/dev/null

# A transcript we cannot read is a REAL failure (bad path, unreadable file) and
# must be recorded — distinct from a pure tool turn, which legitimately has no
# speech to capture and is not an error.
if [ -n "$PY_ERR" ]; then
  bridge_receipt session-capture failed "transcript parse: $PY_ERR"
  exit 0
fi
if [ -z "$CONTENT" ]; then
  bridge_receipt session-capture skipped "no assistant text in turn (tool-only turn)"
  exit 0
fi

ERR_FILE="${TMPDIR:-/tmp}/memory-bridge-remember.$$.err"
"$CONTINUUM" memory/remember \
  --persona_id "$PERSONA" \
  --scope "$PROJECT" \
  --importance 0.3 \
  --content "[session turn] $CONTENT" >/dev/null 2>"$ERR_FILE"
RC=$?
ERR="$(head -c 250 "$ERR_FILE" 2>/dev/null | tr '\n' ' ')"
rm -f "$ERR_FILE" 2>/dev/null

if [ $RC -eq 0 ]; then
  bridge_receipt session-capture ok "scope=$PROJECT chars=${#CONTENT}"
else
  bridge_receipt session-capture failed "memory/remember rc=$RC ${ERR:-no stderr}"
fi

exit 0
