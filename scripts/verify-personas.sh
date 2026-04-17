#!/bin/bash
# verify-personas.sh — persona-level acceptance test for a continuum install
#
# Claim tested: Helper AI AND Teacher AI both respond to a chat message
# via the local DMR path (not cloud, not candle CPU) with coherent output
# within a reasonable time window.
#
# This is the merge-gate acceptance artifact. Runs against a live install.
# Writes a JSON transcript (default: ./persona-verify-<timestamp>.json)
# that can be attached to PRs as proof.
#
# Usage:
#   scripts/verify-personas.sh                          # runs with defaults
#   scripts/verify-personas.sh --room=General           # specify room
#   scripts/verify-personas.sh --timeout=60             # total wait budget (seconds)
#   scripts/verify-personas.sh --output=/tmp/pv.json    # transcript path
#   scripts/verify-personas.sh --personas=helper,teacher,codereview,local
#
# Exit codes:
#   0 = all requested personas replied coherently
#   1 = at least one persona failed to reply or replied with an error
#   2 = configuration or infrastructure error (couldn't reach jtag, etc.)

set -euo pipefail

# Shared repo-root finder — exports REPO_ROOT regardless of where we're invoked from.
# shellcheck source=./lib/repo-root.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/repo-root.sh"

# ── Defaults ────────────────────────────────────────────────
ROOM="General"
# 90s is the practical floor — personas take turns via the scheduler;
# Teacher / Helper can be behind others in priority when a room has 4+
# auto-responders. 45s was too tight for the second-in-queue persona.
TIMEOUT_SEC=90
OUTPUT=""
PERSONAS="helper,teacher"
VERBOSE=false

# ── Parse args ──────────────────────────────────────────────
for arg in "$@"; do
  case "$arg" in
    --room=*)     ROOM="${arg#--room=}" ;;
    --timeout=*)  TIMEOUT_SEC="${arg#--timeout=}" ;;
    --output=*)   OUTPUT="${arg#--output=}" ;;
    --personas=*) PERSONAS="${arg#--personas=}" ;;
    --verbose|-v) VERBOSE=true ;;
    --help|-h)
      grep -E "^# " "$0" | sed 's/^# //;s/^#//' | head -30
      exit 0
      ;;
    *) echo "unknown arg: $arg (--help for usage)" >&2; exit 2 ;;
  esac
done

if [ -z "$OUTPUT" ]; then
  OUTPUT="./persona-verify-$(date +%Y%m%d-%H%M%S).json"
fi

# ── Find jtag (REPO_ROOT already set by repo-root.sh) ───────
JTAG=""
if [ -x "$REPO_ROOT/src/jtag" ]; then
  JTAG="$REPO_ROOT/src/jtag"
elif command -v jtag &>/dev/null; then
  JTAG="$(command -v jtag)"
else
  echo "❌ jtag CLI not found. Expected at $REPO_ROOT/src/jtag or on PATH." >&2
  exit 2
fi

$VERBOSE && echo "jtag: $JTAG"
$VERBOSE && echo "room: $ROOM"
$VERBOSE && echo "personas: $PERSONAS"
$VERBOSE && echo "timeout: ${TIMEOUT_SEC}s"
$VERBOSE && echo "output: $OUTPUT"

# ── Gather environment metadata (goes into the transcript) ──
HOST_OS="$(uname -s)"
HOST_ARCH="$(uname -m)"
GIT_SHA="$(cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
GIT_BRANCH="$(cd "$REPO_ROOT" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'unknown')"
DMR_BACKEND="$(docker model status 2>/dev/null | grep -i 'llama.cpp' | head -1 | tr -s ' ' || echo 'unknown')"

# Detect GPU tier for the transcript
GPU_TIER="unknown"
if [[ "$HOST_OS" == "Darwin" ]]; then
  if sysctl -n machdep.cpu.brand_string 2>/dev/null | grep -qi "apple"; then
    GPU_TIER="metal"
  fi
elif command -v nvidia-smi &>/dev/null; then
  GPU_NAME="$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 || echo '')"
  if [ -n "$GPU_NAME" ]; then
    GPU_TIER="cuda ($GPU_NAME)"
  fi
fi

# ── Per-persona probe ───────────────────────────────────────
TRANSCRIPT_TMP="$(mktemp)"
trap "rm -f '$TRANSCRIPT_TMP'" EXIT

OVERALL_PASS=true
RESULTS="["
FIRST_RESULT=true

IFS=',' read -ra PERSONA_LIST <<< "$PERSONAS"
for PERSONA in "${PERSONA_LIST[@]}"; do
  PERSONA="$(echo "$PERSONA" | tr -d '[:space:]' | tr '[:upper:]' '[:lower:]')"
  [ -z "$PERSONA" ] && continue

  echo ""
  echo "━━━ Probing @${PERSONA} in #${ROOM} ━━━"

  # Unique marker phrase so we can identify THIS probe's reply in the export
  MARKER="$(openssl rand -hex 4 2>/dev/null || date +%s%N | tail -c 9)"
  PROMPT="probe-${MARKER}: reply with one concise sentence about why unit tests matter. keep it under 25 words."

  # Send the chat. jtag uses relative paths internally so it must be invoked
  # with CWD=src/ — failing to cd causes ERR_MODULE_NOT_FOUND on cli.ts.
  SEND_START=$(date +%s)
  SEND_RESULT="$(cd "$REPO_ROOT/src" && "$JTAG" collaboration/chat/send --room="$ROOM" --message="@${PERSONA} ${PROMPT}" 2>&1 || echo '{"success":false,"error":"jtag send failed"}')"
  SEND_END=$(date +%s)

  # Extract the message id. jtag prefixes with warnings ('⚠️ Bundle not found',
  # 'npm warn ...') BEFORE the JSON, so slice from the first '{' to EOF.
  MSG_ID="$(printf '%s' "$SEND_RESULT" | python3 -c "import sys,json,re
try:
    raw = sys.stdin.read()
    idx = raw.find('{')
    d = json.loads(raw[idx:]) if idx >= 0 else {}
    print(d.get('shortId', d.get('messageId', '')))
except:
    print('')
" 2>/dev/null)"

  if [ -z "$MSG_ID" ]; then
    echo "  ❌ send failed. raw response:"
    echo "     $SEND_RESULT" | head -3
    OVERALL_PASS=false
    PERSONA_RESULT="{\"persona\":\"$PERSONA\",\"status\":\"send_failed\",\"error\":\"could not post to room\"}"
  else
    echo "  → sent marker=${MARKER} id=${MSG_ID}"

    # Poll for a reply with marker visible in the export. Real latency measurement.
    # Reply window is up to TIMEOUT_SEC per persona.
    REPLY=""
    REPLY_FROM=""
    REPLY_SECONDS=0
    START_POLL=$(date +%s)
    while true; do
      NOW=$(date +%s)
      REPLY_SECONDS=$((NOW - START_POLL))
      if [ "$REPLY_SECONDS" -ge "$TIMEOUT_SEC" ]; then break; fi

      EXPORT="$(cd "$REPO_ROOT/src" && "$JTAG" collaboration/chat/export --room="$ROOM" --limit=20 2>&1 || echo '')"

      # Look for a message whose replyTo matches our marker OR whose content
      # references our marker (persona replies typically quote-back or
      # respond directly to our message).
      FOUND="$(printf '%s' "$EXPORT" | python3 -c "
import sys,json,re
try:
    raw = sys.stdin.read()
    idx = raw.find('{')
    d = json.loads(raw[idx:]) if idx >= 0 else {}
    md = d.get('markdown','')
    marker = '${MARKER}'
    persona = '${PERSONA}'.lower()
    # Parse messages out of the markdown. Each block is of shape:
    #   (possible leading empty line)
    #   ## #<id> - <display name> (reply to #<id>)
    #   *<timestamp>*
    #   (empty line)
    #   <body line 1>
    #   <body line 2>
    #   ...
    # Blocks separated by '---' at start-of-line.
    blocks = re.split(r'\n---\n', md)
    for b in reversed(blocks):  # newest first
        lines = b.strip().split('\n')
        # First non-empty line is the header (## #<id> - <name>)
        header = ''
        body_start = 0
        for i, line in enumerate(lines):
            if line.startswith('## '):
                header = line.lower()
                # Body starts after the header and the timestamp '*...*' line + blank
                body_start = i + 1
                # Skip timestamp line(s) and empty lines until we hit content
                while body_start < len(lines) and (lines[body_start].startswith('*') or lines[body_start].strip() == ''):
                    body_start += 1
                break
        body = '\n'.join(lines[body_start:]).strip()
        # Match on persona display-name hints in the header (helper/teacher/codereview/local).
        # Exclude messages whose BODY contains our probe marker (those are OUR sends, not replies).
        # Body length > 30 filters out ultra-short / failed messages.
        if persona in header and marker not in body and len(body) > 30:
            print('FOUND::' + body[:500].replace('\n',' '))
            break
except Exception:
    pass
" 2>/dev/null)"

      if [[ "$FOUND" == FOUND::* ]]; then
        REPLY="${FOUND#FOUND::}"
        break
      fi

      sleep 2
    done

    if [ -n "$REPLY" ]; then
      REPLY_TOKENS=$(echo "$REPLY" | wc -w | tr -d ' ')
      echo "  ✅ reply in ${REPLY_SECONDS}s, ~${REPLY_TOKENS} words"
      echo "     \"${REPLY:0:120}...\""
      PERSONA_RESULT="{\"persona\":\"$PERSONA\",\"status\":\"replied\",\"reply_seconds\":$REPLY_SECONDS,\"reply_word_count\":$REPLY_TOKENS,\"reply_excerpt\":$(printf '%s' "${REPLY:0:500}" | python3 -c 'import sys,json; print(json.dumps(sys.stdin.read()))')}"
    else
      echo "  ❌ no coherent reply within ${TIMEOUT_SEC}s"
      OVERALL_PASS=false
      PERSONA_RESULT="{\"persona\":\"$PERSONA\",\"status\":\"timeout\",\"reply_seconds\":$TIMEOUT_SEC}"
    fi
  fi

  if $FIRST_RESULT; then
    RESULTS="$RESULTS$PERSONA_RESULT"
    FIRST_RESULT=false
  else
    RESULTS="$RESULTS,$PERSONA_RESULT"
  fi
done
RESULTS="$RESULTS]"

# ── Write transcript ────────────────────────────────────────
VERDICT="pass"
EXIT_CODE=0
if ! $OVERALL_PASS; then
  VERDICT="fail"
  EXIT_CODE=1
fi

cat > "$OUTPUT" <<EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "verdict": "$VERDICT",
  "environment": {
    "host_os": "$HOST_OS",
    "host_arch": "$HOST_ARCH",
    "git_sha": "$GIT_SHA",
    "git_branch": "$GIT_BRANCH",
    "dmr_backend": "$DMR_BACKEND",
    "gpu_tier": "$GPU_TIER"
  },
  "room": "$ROOM",
  "timeout_seconds": $TIMEOUT_SEC,
  "results": $RESULTS
}
EOF

echo ""
echo "━━━ Verdict: $VERDICT ━━━"
echo "transcript: $OUTPUT"

if [ "$EXIT_CODE" -ne 0 ]; then
  echo ""
  echo "At least one persona did not reply. Inspect the transcript for details."
  echo "Common causes:"
  echo "  - syncPersonaProviders() hasn't run (restart node-server after first seed)"
  echo "  - DMR backend stuck on latest-cpu (Docker Desktop Settings → AI toggle)"
  echo "  - personas still have provider='candle' in DB (pre-GPU-always image)"
  echo "  - continuum-core not running (docker compose ps continuum-core)"
fi

exit $EXIT_CODE
