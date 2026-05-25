#!/usr/bin/env bash
# canary-smoke-chat-dual-write.sh — Stage-1 Continuum chat -> AIRC proof.
#
# Sends a real Continuum chat message through collaboration/chat/send, then
# asserts the same logical message exists in:
#   1. ORM chat_messages, and
#   2. the repo-scoped AIRC structured event store.
#
# The AIRC side is read with sqlite3 -json by receipt id. This script does not
# parse human stdout from `airc events`.

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STACK_REQUIRED="${STACK_REQUIRED:-0}"
ROOM="${AIRC_CHAT_SMOKE_ROOM:-general}"

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-chat-dual-write\n'
printf '  ROOT_DIR=%s\n' "$ROOT_DIR"
printf '  ROOM=%s\n' "$ROOM"
printf '  STACK_REQUIRED=%s\n' "$STACK_REQUIRED"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if ! command -v airc >/dev/null 2>&1; then
  printf '  ✗ preflight: airc not found on PATH\n' >&2
  exit 2
fi

if ! command -v sqlite3 >/dev/null 2>&1; then
  printf '  ✗ preflight: sqlite3 not found on PATH\n' >&2
  exit 2
fi

STACK_UP=0
CORE_SOCKET="${CONTINUUM_CORE_SOCKET:-$HOME/.continuum/sockets/continuum-core.sock}"
if [ -S "$CORE_SOCKET" ]; then
  STACK_UP=1
elif pgrep -f '[c]ontinuum-core|[w]idget-server|[n]ode.*start-server' >/dev/null 2>&1; then
  STACK_UP=1
fi

if [ "$STACK_UP" -eq 0 ]; then
  if [ "$STACK_REQUIRED" -eq 1 ]; then
    printf '  ✗ stack presence — STACK_REQUIRED=1 but no Continuum stack is running\n' >&2
    exit 2
  fi
  printf '  - skipped — no Continuum stack is running (run npm start, or set STACK_REQUIRED=1 to fail)\n'
  exit 0
fi

cd "$ROOT_DIR/src" || exit 2
npx tsx tests/precommit/chat-airc-dual-write-smoke.test.ts
