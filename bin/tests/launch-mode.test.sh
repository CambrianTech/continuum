#!/usr/bin/env bash
# launch-mode.test.sh — unit tests for the config.env settings helpers and the
# launch-mode resolver in bin/continuum.
#
# What this catches: the ONE launch-mode setting + its precedence + the
# auto-detect-then-persist contract. The resolver is the single source of truth
# every surface (npm start, the tray, install, the UI shutting itself off) will
# edit/read — a regression here silently boots the wrong face on servers/laptops.
#
# Runs WITHOUT a live system: sources bin/continuum (the main-guard keeps it from
# dispatching), points CONTINUUM_HOME at a temp dir, and stubs has_display() so
# both branches are deterministic regardless of the host (Mac always has display).
#
#   bash bin/tests/launch-mode.test.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRAIN="$(dirname "$SCRIPT_DIR")/continuum"

PASS=0
FAIL=0
fail() { echo "  ✗ $1"; FAIL=$((FAIL + 1)); }
pass() { echo "  ✓ $1"; PASS=$((PASS + 1)); }
assert_eq() { # expected actual label
  if [ "$1" = "$2" ]; then pass "$3"; else fail "$3 — expected '$1', got '$2'"; fi
}

# Fresh temp home per run; sourcing picks it up via the ${CONTINUUM_HOME:-...} default.
TMP_HOME="$(mktemp -d "${TMPDIR:-/tmp}/continuum-lm-test.XXXXXX")"
trap 'rm -rf "$TMP_HOME"' EXIT
export CONTINUUM_HOME="$TMP_HOME"

# shellcheck disable=SC1090
source "$BRAIN"

CONFIG="$TMP_HOME/config.env"

echo "config_set / config_get:"
config_set HTTP_PORT 9000
config_set ANOTHER_KEY hello
assert_eq "9000" "$(config_get HTTP_PORT)" "config_get reads a written key"
assert_eq "hello" "$(config_get ANOTHER_KEY)" "config_get reads a second key"
# Upsert must replace, not duplicate, and must preserve siblings.
config_set HTTP_PORT 9100
assert_eq "9100" "$(config_get HTTP_PORT)" "config_set upserts (new value wins)"
assert_eq "1" "$(grep -c '^HTTP_PORT=' "$CONFIG")" "config_set leaves exactly one line for the key"
assert_eq "hello" "$(config_get ANOTHER_KEY)" "config_set preserves sibling keys"
if config_get NOPE >/dev/null 2>&1; then fail "config_get returns non-zero for missing key"; else pass "config_get returns non-zero for missing key"; fi

echo "launch_mode — explicit override (no write, no detect):"
rm -f "$CONFIG"
assert_eq "ui" "$(launch_mode ui 2>/dev/null)" "explicit 'ui' arg short-circuits"
assert_eq "headless" "$(launch_mode headless 2>/dev/null)" "explicit 'headless' arg short-circuits"
if [ -f "$CONFIG" ] && grep -q CONTINUUM_LAUNCH_MODE "$CONFIG" 2>/dev/null; then
  fail "explicit override must NOT write a setting"
else
  pass "explicit override must NOT write a setting"
fi

echo "launch_mode — env var beats config.env:"
rm -f "$CONFIG"
config_set CONTINUUM_LAUNCH_MODE headless
assert_eq "ui" "$(CONTINUUM_LAUNCH_MODE=ui launch_mode 2>/dev/null)" "CONTINUUM_LAUNCH_MODE env overrides config.env value"
unset CONTINUUM_LAUNCH_MODE

echo "launch_mode — config.env value is honored:"
rm -f "$CONFIG"
config_set CONTINUUM_LAUNCH_MODE headless
assert_eq "headless" "$(launch_mode 2>/dev/null)" "config.env CONTINUUM_LAUNCH_MODE=headless is read"

echo "launch_mode — auto detects + persists (write-back):"
# Force the headless branch deterministically.
has_display() { return 1; }
rm -f "$CONFIG"
assert_eq "headless" "$(launch_mode auto 2>/dev/null)" "auto + no display → headless"
assert_eq "headless" "$(config_get CONTINUUM_LAUNCH_MODE)" "auto persisted 'headless' back to config.env"
# Force the ui branch.
has_display() { return 0; }
rm -f "$CONFIG"
assert_eq "ui" "$(launch_mode auto 2>/dev/null)" "auto + display → ui"
assert_eq "ui" "$(config_get CONTINUUM_LAUNCH_MODE)" "auto persisted 'ui' back to config.env"

echo "launch_mode — unset setting falls through to auto-detect + persist:"
has_display() { return 1; }
rm -f "$CONFIG"
assert_eq "headless" "$(launch_mode 2>/dev/null)" "no setting at all → auto-detect (headless)"
assert_eq "headless" "$(config_get CONTINUUM_LAUNCH_MODE)" "no-setting path also persists the resolved value"

echo "launch_mode — garbage value warns + re-detects (no silent accept):"
has_display() { return 0; }
config_set CONTINUUM_LAUNCH_MODE banana
assert_eq "ui" "$(launch_mode 2>/dev/null)" "garbage setting → auto-detect, not the garbage"

echo ""
echo "── $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
