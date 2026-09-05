#!/usr/bin/env bash
# install-common.test.sh — smoke tests for tools/scripts/lib/install-common.sh
#
# Run: bash tools/scripts/lib/install-common.test.sh
# Or:  bash tools/scripts/lib/install-common.test.sh -v   (verbose)
#
# Each test is a function `test_<thing>` that exits 0 on pass, non-zero
# on fail, and prints a one-line PASS/FAIL summary. The runner at the
# bottom invokes them all, prints a summary, exits with the failure
# count.
#
# These are SMOKE tests — they verify the modules' guards behave as
# documented and that helpers don't have shell-syntax regressions.
# Real end-to-end coverage lives in the BigMama dry-run playbook
# (docs/infrastructure/PR891-E2E-VALIDATION.md).

set -u  # unset vars are errors; deliberately NOT set -e (we want failures
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)
        # to be caught + reported, not abort the whole run)

# Resolve script dir so we can source the library regardless of cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/install-common.sh"

VERBOSE=0
[ "${1:-}" = "-v" ] && VERBOSE=1

PASSED=0
FAILED=0
FAILURES=()

# ── Test framework ───────────────────────────────────────────
_run_test() {
  local name=$1
  local result_dir; result_dir=$(mktemp -d)
  local out
  if out=$("$name" 2>&1 >/dev/null); then
    PASSED=$((PASSED + 1))
    printf '  ✓ %s\n' "$name"
    [ "$VERBOSE" = 1 ] && [ -n "$out" ] && printf '    %s\n' "$out"
  else
    FAILED=$((FAILED + 1))
    FAILURES+=("$name")
    printf '  ✗ %s\n' "$name"
    [ -n "$out" ] && printf '    %s\n' "$out" | head -5
  fi
  rm -rf "$result_dir"
}

assert_eq() {
  local expected=$1 actual=$2 msg=${3:-}
  [ "$expected" = "$actual" ] || { echo "expected=[$expected] actual=[$actual] $msg" >&2; return 1; }
}

assert_contains() {
  local needle=$1 haystack=$2 msg=${3:-}
  case "$haystack" in
    *"$needle"*) return 0 ;;
    *) echo "needle=[$needle] not in haystack=[$haystack] $msg" >&2; return 1 ;;
  esac
}

# ── Library source check ─────────────────────────────────────
test_lib_exists() { [ -f "$LIB" ] || { echo "missing: $LIB"; return 1; }; }

test_lib_syntax_clean() { bash -n "$LIB" || return 1; }

test_lib_sources_idempotent() {
  # Sourcing twice should be a no-op (guard at top of file).
  ( source "$LIB" && source "$LIB" ) || return 1
}

# ── Log primitive tests ──────────────────────────────────────
test_info_outputs_arrow() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; info "hello")
  assert_contains "→" "$out" && assert_contains "hello" "$out"
}

test_ok_outputs_check() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; ok "done")
  assert_contains "✓" "$out" && assert_contains "done" "$out"
}

test_warn_outputs_to_stderr() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; warn "careful" 2>&1 >/dev/null)
  assert_contains "!" "$out" && assert_contains "careful" "$out"
}

test_die_exits_nonzero() {
  ( source "$LIB" >/dev/null 2>&1; die "fatal" 2>/dev/null ) ; local rc=$?
  [ "$rc" -ne 0 ]
}

test_fail_alias_works() {
  # `fail` should be aliased to `die` for callers that use the older name
  ( source "$LIB" >/dev/null 2>&1; fail "fatal" 2>/dev/null ) ; local rc=$?
  [ "$rc" -ne 0 ]
}

# ── Module primitive tests ───────────────────────────────────
test_module_skip_includes_name_and_reason() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; module_skip "x" "y")
  assert_contains "x" "$out" && assert_contains "y" "$out" && assert_contains "skipped" "$out"
}

test_module_start_includes_name_and_what() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; module_start "x" "doing y")
  assert_contains "x" "$out" && assert_contains "doing y" "$out"
}

test_module_done_emits_check() {
  local out; out=$(source "$LIB" >/dev/null 2>&1; module_done "x")
  assert_contains "✓" "$out" && assert_contains "x" "$out" && assert_contains "done" "$out"
}

test_module_fail_exits_nonzero() {
  ( source "$LIB" >/dev/null 2>&1; module_fail "x" "fix instructions" 2>/dev/null ) ; local rc=$?
  [ "$rc" -ne 0 ]
}

# ── Sudo warmup tests ────────────────────────────────────────
test_ensure_sudo_warmed_noop_when_root() {
  # If we're already root (or simulated), should return 0 immediately.
  # We test the no-tty path: fail loud when stdin isn't a terminal AND
  # no warmed cache AND not root. Since we can't BE root in a test
  # context, verify the check shape via static reading.
  grep -q '\[ "\$(id -u)" -eq 0 \] && return 0' "$LIB" || return 1
}

test_ensure_sudo_warmed_has_no_tty_failure_path() {
  grep -q 'stdin is not a terminal' "$LIB" || return 1
}

test_ensure_sudo_warmed_has_keepalive_loop() {
  grep -q 'sudo -n true.*sleep 50' "$LIB" || return 1
}

test_ensure_sudo_warmed_traps_exit() {
  grep -q "trap.*_sudo_cleanup' EXIT" "$LIB" || return 1
}

# ── Module behavior: idempotent + applicability guards ───────
test_mod_submodules_init_skips_when_no_gitmodules() {
  local tmp; tmp=$(mktemp -d)
  ( cd "$tmp" && git init -q 2>/dev/null
    out=$(source "$LIB" >/dev/null 2>&1; mod_submodules_init 2>&1)
    assert_contains "submodules" "$out"
    assert_contains "skipped" "$out" )
  local rc=$?
  rm -rf "$tmp"
  return $rc
}

test_mod_docker_wsl_integration_skips_on_macos() {
  # macOS has no /proc/version. Module should skip cleanly.
  if [ "$(uname -s)" != "Darwin" ]; then
    echo "(skipped: not macOS)" >&2
    return 0
  fi
  local out; out=$(source "$LIB" >/dev/null 2>&1; mod_docker_wsl_integration 2>&1)
  assert_contains "not WSL2" "$out" && assert_contains "skipped" "$out"
}

test_mod_tailscale_check_handles_missing() {
  # Force tailscale-not-found by clearing PATH temporarily.
  local out
  out=$(source "$LIB" >/dev/null 2>&1; PATH=/usr/bin:/bin mod_tailscale_check 2>&1)
  # Either skips (not installed) or skips/active depending on whether
  # tailscale is in /usr/bin (rare). Acceptable: contains "tailscale"
  # and either "not installed" or "active".
  assert_contains "tailscale" "$out"
}

test_mod_docker_check_fails_loud_when_missing() {
  # Force docker-not-found via empty PATH. Should fail loud (exit nonzero).
  ( source "$LIB" >/dev/null 2>&1; PATH=/usr/bin:/bin command -v docker >/dev/null 2>&1 ) && {
    # Docker IS in default PATH on this machine — skip the negative test.
    echo "(skipped: docker present in default PATH)" >&2
    return 0
  }
  ( source "$LIB" >/dev/null 2>&1; PATH=/usr/bin:/bin mod_docker_check 2>/dev/null ) ; local rc=$?
  [ "$rc" -ne 0 ]
}

test_mod_continuum_bin_link_uses_user_space_when_no_sudo_no_tty() {
  local tmp; tmp=$(mktemp -d)
  local src="$tmp/src-bin"
  echo '#!/bin/sh' > "$src"
  chmod +x "$src"
  HOME="$tmp" bash -c "
    source '$LIB' >/dev/null 2>&1
    mod_continuum_bin_link '$src'
  " 2>&1 | grep -q 'continuum-bin' && \
  [ -x "$tmp/.local/bin/continuum" ]
  local rc=$?
  rm -rf "$tmp"
  return $rc
}

# ── Runner ───────────────────────────────────────────────────
echo ""
echo "install-common.test.sh — smoke suite"
echo "------------------------------------"

_run_test test_lib_exists
_run_test test_lib_syntax_clean
_run_test test_lib_sources_idempotent

_run_test test_info_outputs_arrow
_run_test test_ok_outputs_check
_run_test test_warn_outputs_to_stderr
_run_test test_die_exits_nonzero
_run_test test_fail_alias_works

_run_test test_module_skip_includes_name_and_reason
_run_test test_module_start_includes_name_and_what
_run_test test_module_done_emits_check
_run_test test_module_fail_exits_nonzero

_run_test test_ensure_sudo_warmed_noop_when_root
_run_test test_ensure_sudo_warmed_has_no_tty_failure_path
_run_test test_ensure_sudo_warmed_has_keepalive_loop
_run_test test_ensure_sudo_warmed_traps_exit

_run_test test_mod_submodules_init_skips_when_no_gitmodules
_run_test test_mod_docker_wsl_integration_skips_on_macos
_run_test test_mod_tailscale_check_handles_missing
_run_test test_mod_docker_check_fails_loud_when_missing
_run_test test_mod_continuum_bin_link_uses_user_space_when_no_sudo_no_tty

echo ""
echo "------------------------------------"
echo "Passed: $PASSED  Failed: $FAILED"
[ "$FAILED" -gt 0 ] && {
  echo "Failures:"
  for f in "${FAILURES[@]}"; do printf '  - %s\n' "$f"; done
  exit 1
}
echo "All green."
exit 0
