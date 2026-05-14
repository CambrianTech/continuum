#!/usr/bin/env bash
# canary-smoke-jtag.sh — JTAG ping + screenshot slice of the canary
# end-to-end smoke matrix (continuum#1132).
#
# WHY THIS GATE EXISTS
#
# The user-facing surface — what Carl actually opens after install — is
# only as good as the JTAG CLI's ability to talk to the running stack
# AND the widget DOM's ability to render. Both have failed silently
# in production: the global `jtag` shim has been observed pointing at
# a deleted temp dir from a prior install (issue #91-#93), and the
# screenshot path can return 200 with a blank page when the widget
# server is up but the bundle is stale.
#
# This slice catches both: (1) jtag CLI invokable; (2) jtag → running
# stack roundtrip works (ping); (3) screenshot writes a non-empty file
# that's a valid PNG.
#
# WHAT IT VALIDATES
#
#   1. jtag binary is on PATH (or ./src/jtag exists in this repo).
#      File-system check only — JTAG CLI requires the running stack
#      even for `--help`, so an invocation-based liveness probe is
#      indistinguishable from a stack-down skip.
#   2. Stack is reachable: `jtag ping` returns success. Catches:
#      stack not running; widget-server crashed; UnixSocket gone;
#      AND the dangling-shim regression class (#91-#93) where the
#      shim resolves but invocation fails with ERR_MODULE_NOT_FOUND.
#   3. Screenshot writes a non-empty PNG: `jtag interface/screenshot
#      --filename TMP.png` produces > 1KB file with PNG magic bytes.
#      Catches: screenshot returns 200 but body is empty/blank.
#
# When the stack is DOWN (no continuum-core process), steps 2-3 SKIP
# with a clear message — operator can run `npm start` to enable.
#
# RUNNING
#
#   bash scripts/ci/canary-smoke-jtag.sh
#
# Optional env:
#   JTAG_BIN=/path/to/jtag         override which jtag binary to test
#   STACK_REQUIRED=1               turn skip-when-down into hard fail
#   SMOKE_VERBOSE=1                show per-step output (default: failures only)
#
# EXIT CODES
#
#   0  every required check passed (skips are OK)
#   2  one or more checks failed (script reports which)

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
JTAG_BIN="${JTAG_BIN:-}"
STACK_REQUIRED="${STACK_REQUIRED:-0}"
SMOKE_VERBOSE="${SMOKE_VERBOSE:-0}"

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
FAILED_STEPS=()

# Resolve jtag CLI: explicit JTAG_BIN > PATH lookup > ./src/jtag fallback.
# Each layer needs to actually invoke; a shim that points at a deleted
# dir resolves via `command -v` but fails on first run (this is exactly
# the production bug pattern this gate is designed to catch).
resolve_jtag() {
  if [ -n "$JTAG_BIN" ] && [ -x "$JTAG_BIN" ]; then
    printf '%s' "$JTAG_BIN"
    return 0
  fi
  if command -v jtag >/dev/null 2>&1; then
    printf '%s' "$(command -v jtag)"
    return 0
  fi
  if [ -x "$ROOT_DIR/src/jtag" ]; then
    printf '%s' "$ROOT_DIR/src/jtag"
    return 0
  fi
  return 1
}

pass() {
  PASS_COUNT=$((PASS_COUNT + 1))
  printf '  ✓ %s\n' "$1"
}

skip() {
  SKIP_COUNT=$((SKIP_COUNT + 1))
  printf '  - %s — %s\n' "$1" "$2"
}

fail() {
  FAIL_COUNT=$((FAIL_COUNT + 1))
  FAILED_STEPS+=("$1: $2")
  printf '  ✗ %s — %s\n' "$1" "$2"
  if [ -n "${3:-}" ]; then
    printf '%s\n' "$3" | tail -20 | sed 's/^/      /'
  fi
}

# ── preflight: locate jtag ──────────────────────────────────────────

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-jtag (continuum#1132)\n'
printf '  ROOT_DIR=%s\n' "$ROOT_DIR"
printf '  STACK_REQUIRED=%s\n' "$STACK_REQUIRED"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

JTAG=""
if ! JTAG=$(resolve_jtag); then
  fail "preflight: jtag CLI" "no jtag binary on PATH and no ./src/jtag"
  printf '\nFailed steps:\n'
  for s in "${FAILED_STEPS[@]}"; do printf '  ✗ %s\n' "$s"; done
  exit 2
fi
printf '  JTAG=%s\n' "$JTAG"

# ── stack-presence detection ────────────────────────────────────────

# JTAG CLI requires the running stack for ANY command, including help —
# the dispatcher initializes by connecting to continuum-core's UnixSocket
# at startup. If no continuum-core process, every JTAG invocation will
# fail with connect ENOENT, which is indistinguishable from a real
# regression. So we gate steps 2-3 behind a process-scan preflight.
STACK_UP=0
if pgrep -f 'continuum-core|widget-server|node.*start-server' >/dev/null 2>&1; then
  STACK_UP=1
fi

if [ "$STACK_UP" -eq 0 ]; then
  if [ "$STACK_REQUIRED" -eq 1 ]; then
    fail "stack presence" "STACK_REQUIRED=1 but no continuum-core process running"
    fail "jtag ping reaches stack" "(stack down)"
    fail "jtag screenshot writes valid PNG" "(stack down)"
  else
    skip "jtag ping reaches stack" "no continuum-core process running (run npm start)"
    skip "jtag screenshot writes valid PNG" "(skipped: stack down)"
  fi
fi

# ── 1. stack reachable: jtag ping ───────────────────────────────────

# `jtag ping` tests the round trip from CLI through the WebSocket bridge
# to continuum-core and back. Catches: dangling-shim regression
# (#91-#93) where shim resolves but invocation fails with
# ERR_MODULE_NOT_FOUND; stack crashed; UnixSocket gone.
if [ "$STACK_UP" -eq 1 ]; then
  ping_out=$("$JTAG" ping 2>&1)
  ping_rc=$?
  if [ "$ping_rc" -eq 0 ] || printf '%s' "$ping_out" | grep -qiE '(pong|"ok"\s*:\s*true|connected)'; then
    pass "jtag ping reaches stack"
  else
    # Specific recovery hint for the dangling-shim pattern.
    hint=""
    if printf '%s' "$ping_out" | grep -qE 'ERR_MODULE_NOT_FOUND.*cli\.ts'; then
      hint=' — dangling shim. Reinstall: bash install.sh (or rebuild bundle: npm run build:cli && cp src/jtag $(readlink "$JTAG"))'
    elif printf '%s' "$ping_out" | grep -qE 'connect ENOENT'; then
      hint=' — UnixSocket missing despite running process. Stack may be mid-startup or in a wedged state.'
    fi
    fail "jtag ping reaches stack" "exit=$ping_rc${hint}" "$ping_out"
  fi
fi

# ── 2. screenshot writes valid PNG ──────────────────────────────────

# Only attempt screenshot if ping passed. The screenshot path goes
# through the widget server; if ping already failed we know screenshot
# would too — the failure detail above is more diagnostic.
if [ "$STACK_UP" -eq 1 ] && [ "$FAIL_COUNT" -eq 0 ]; then
  shot_file=$(mktemp -t jtag-smoke-shot.XXXXXX.png) || {
    fail "jtag screenshot writes valid PNG" "mktemp failed"
    shot_file=""
  }
  if [ -n "$shot_file" ]; then
    shot_out=$("$JTAG" interface/screenshot --filename "$shot_file" 2>&1)
    shot_rc=$?
    shot_size=$(stat -f%z "$shot_file" 2>/dev/null || stat -c%s "$shot_file" 2>/dev/null || echo 0)
    # PNG magic bytes: 89 50 4E 47 (\x89 P N G). Read first 4 bytes as
    # hex to confirm we got a real PNG, not an HTML error page or empty
    # file (the silent-blank-screenshot pattern this gate exists to catch).
    shot_magic=$(head -c 4 "$shot_file" 2>/dev/null | od -An -tx1 | tr -d ' \n' || echo "")
    rm -f "$shot_file"

    if [ "$shot_rc" -ne 0 ]; then
      fail "jtag screenshot writes valid PNG" "exit=$shot_rc" "$shot_out"
    elif [ "$shot_size" -lt 1024 ]; then
      fail "jtag screenshot writes valid PNG" "file size $shot_size bytes < 1KB (silent-blank pattern)" "$shot_out"
    elif [ "$shot_magic" != "89504e47" ]; then
      fail "jtag screenshot writes valid PNG" "magic bytes $shot_magic != 89504e47 (not a PNG; likely HTML error page)" "$shot_out"
    else
      pass "jtag screenshot writes valid PNG (size=${shot_size}B)"
    fi
  fi
fi

# ── summary ─────────────────────────────────────────────────────────

printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'
printf '  canary-smoke-jtag: %d passed, %d skipped, %d failed\n' \
  "$PASS_COUNT" "$SKIP_COUNT" "$FAIL_COUNT"
printf '━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n'

if [ "$FAIL_COUNT" -gt 0 ]; then
  printf 'Failed steps:\n'
  for s in "${FAILED_STEPS[@]}"; do
    printf '  ✗ %s\n' "$s"
  done
  exit 2
fi

exit 0
