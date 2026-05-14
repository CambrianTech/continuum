#!/usr/bin/env bash
# canary-smoke-matrix.sh — one-command runner for the canary end-to-end
# smoke matrix tracked by continuum#1132.
#
# This script deliberately composes the narrower smoke slices instead of
# duplicating their logic. Each slice stays owned by its subsystem, while
# this entrypoint gives agents and humans one command to paste into issue
# evidence before merging canary-bound work.

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SMOKE_VERBOSE="${SMOKE_VERBOSE:-0}"
RUN_CARGO_CHECK="${RUN_CARGO_CHECK:-0}"
STACK_REQUIRED="${STACK_REQUIRED:-0}"

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0
FAILED_STEPS=()
WARNED_STEPS=()

run_slice() {
  local name="$1"
  local required="$2"
  shift 2

  printf '\n━━━ %s ━━━\n' "$name"

  local out rc
  out=$("$@" 2>&1)
  rc=$?

  if [ "$SMOKE_VERBOSE" = "1" ] || [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | sed 's/^/  /'
  else
    printf '%s\n' "$out" | tail -8 | sed 's/^/  /'
  fi

  if [ "$rc" -eq 0 ]; then
    PASS_COUNT=$((PASS_COUNT + 1))
    printf '  ✓ %s\n' "$name"
    return 0
  fi

  if [ "$required" = "0" ]; then
    WARN_COUNT=$((WARN_COUNT + 1))
    WARNED_STEPS+=("$name exited $rc")
    printf '  - %s — optional slice exited %s\n' "$name" "$rc"
    return 0
  fi

  FAIL_COUNT=$((FAIL_COUNT + 1))
  FAILED_STEPS+=("$name exited $rc")
  printf '  ✗ %s — exit=%s\n' "$name" "$rc"
  return 0
}

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-matrix (continuum#1132)\n'
printf '  ROOT_DIR=%s\n' "$ROOT_DIR"
printf '  RUN_CARGO_CHECK=%s\n' "$RUN_CARGO_CHECK"
printf '  STACK_REQUIRED=%s\n' "$STACK_REQUIRED"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

cd "$ROOT_DIR" || exit 2

run_slice "AIRC queue lifecycle" 1 \
  bash scripts/ci/canary-smoke-airc-queue.sh

run_slice "Rust feature contract" 1 \
  env RUN_CARGO_CHECK="$RUN_CARGO_CHECK" bash scripts/ci/canary-smoke-rust-features.sh

run_slice "JTAG ping + screenshot" "$STACK_REQUIRED" \
  env STACK_REQUIRED="$STACK_REQUIRED" bash scripts/ci/canary-smoke-jtag.sh

printf '\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-matrix: %d passed, %d optional warnings, %d failed\n' \
  "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if [ "$WARN_COUNT" -gt 0 ]; then
  printf 'Optional warnings:\n'
  for step in "${WARNED_STEPS[@]}"; do
    printf '  - %s\n' "$step"
  done
fi

if [ "$FAIL_COUNT" -gt 0 ]; then
  printf 'Failed required slices:\n' >&2
  for step in "${FAILED_STEPS[@]}"; do
    printf '  - %s\n' "$step" >&2
  done
  exit 2
fi

exit 0
