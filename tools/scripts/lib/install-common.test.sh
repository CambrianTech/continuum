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

# Regression for card 873931f4: a clean deploy worktree must not inherit another
# checkout's CMake owner, nor erase matching, unreadable or malformed evidence.
# Exercise the actual shared CMake helper; no compiler or engine build is run.
test_llama_cache_tracks_source_ownership() (
  local temp_root scratch source_a source_b owner build helper
  temp_root="$(cd "${TMPDIR:-/tmp}" && pwd -P)" || return 1
  scratch="$(mktemp -d "$temp_root/continuum-llama-cache.XXXXXX")" || return 1
  scratch="$(cd "$scratch" && pwd -P)" || return 1
  case "$scratch" in "$temp_root"/continuum-llama-cache.*) ;; *) return 1 ;; esac
  trap 'rm -rf -- "$scratch"' EXIT
  source_a="$scratch/source one"
  source_b="$scratch/source two"
  build="$scratch/llama-server-build"
  helper="$SCRIPT_DIR/prepare-llama-build.cmake"
  mkdir -p "$source_a" "$source_b" "$build" || return 1
  touch "$source_a/CMakeLists.txt" "$source_b/CMakeLists.txt" || return 1
  owner="$source_a"
  if command -v cygpath >/dev/null 2>&1; then
    # CMakeCache on Windows can use native separators and casing; callers can
    # arrive through Git Bash or native PowerShell. Both name the same owner.
    owner="$(cygpath -w "$source_a")"
  fi

  cmake "-DSOURCE_DIR=$source_a" "-DBUILD_DIR=$build" -P "$helper" || return 1
  [ ! -e "$build/CMakeCache.txt" ] || return 1
  printf 'CMAKE_HOME_DIRECTORY:INTERNAL=%s\n' "$owner" > "$build/CMakeCache.txt"
  mkdir -p "$build/CMakeFiles"
  touch "$build/CMakeFiles/preserve" "$build/preserve-output"
  cmake "-DSOURCE_DIR=$source_a/../source one" "-DBUILD_DIR=$build" -P "$helper" || return 1
  [ -f "$build/CMakeFiles/preserve" ] && [ -f "$build/CMakeCache.txt" ] || return 1

  local out
  out="$(cmake "-DSOURCE_DIR=$source_b" "-DBUILD_DIR=$build" -P "$helper" 2>&1)" || return 1
  assert_contains 'source changed:' "$out" || return 1
  [ ! -e "$build/CMakeCache.txt" ] && [ ! -e "$build/CMakeFiles" ] || return 1
  [ -f "$build/preserve-output" ] || return 1

  # Unknown ownership is a refusal, not an excuse to clear the cache.
  printf 'NOT_AN_OWNER:BOOL=ON\n' > "$build/CMakeCache.txt"
  mkdir -p "$build/CMakeFiles"
  touch "$build/CMakeFiles/preserve"
  if out="$(cmake "-DSOURCE_DIR=$source_b" "-DBUILD_DIR=$build" -P "$helper" 2>&1)"; then return 1; fi
  assert_contains 'cannot identify the source owner' "$out" || return 1
  [ -f "$build/CMakeCache.txt" ] && [ -f "$build/CMakeFiles/preserve" ] || return 1

  # A path that cannot be read as a cache must remain intact too.
  rm -- "$build/CMakeCache.txt" || return 1
  mkdir "$build/CMakeCache.txt" || return 1
  touch "$build/CMakeCache.txt/preserve"
  if out="$(cmake "-DSOURCE_DIR=$source_b" "-DBUILD_DIR=$build" -P "$helper" 2>&1)"; then return 1; fi
  assert_contains 'CMakeCache.txt' "$out" || return 1
  [ -f "$build/CMakeCache.txt/preserve" ] && [ -f "$build/CMakeFiles/preserve" ] || return 1

  # Even an explicitly supplied path cannot redirect removal into a source tree.
  if out="$(cmake "-DSOURCE_DIR=$source_b" "-DBUILD_DIR=$source_a" -P "$helper" 2>&1)"; then return 1; fi
  assert_contains 'unexpected llama-server build directory' "$out" || return 1
  [ -f "$source_a/CMakeLists.txt" ]
)

# what this catches: installing or rerunning cold-storage erased unrelated
# configuration, while xargs changed literal paths before drive selection.
test_cold_storage_preserves_config() (
  source "$LIB"
  local scratch; scratch="$(mktemp -d)" || return 1
  trap 'rm -rf -- "$scratch"' EXIT
  export HOME="$scratch/home"
  local cold="$scratch/cold cache \$literal" config="$HOME/.continuum/config.env"
  mkdir -p "$HOME/.continuum" "$cold" || return 1
  printf "# retained\nLABEL='λ'\nCUSTOM_VALUE='literal \$value \\path = stays'\n" > "$scratch/kept"
  cp "$scratch/kept" "$config" || return 1
  chmod 600 "$config" || return 1
  _cold_export "$cold" || return 1
  head -3 "$config" > "$scratch/actual"
  cmp "$scratch/kept" "$scratch/actual" || return 1
  cp "$config" "$scratch/first" || return 1
  _cold_export "$cold" || return 1
  cmp "$config" "$scratch/first" || return 1
  assert_eq "$cold/tmp" "$TMPDIR" || return 1
  [ -d "$TMPDIR" ] || return 1
  ( unset CONTINUUM_STORAGE_PATH HF_HOME; source "$config"; assert_eq "$cold" "$CONTINUUM_STORAGE_PATH" ) || return 1
  _cold_drive() { echo 'must not rediscover quoted configured path' >&2; return 1; }
  mod_cold_storage || return 1
  cmp "$config" "$scratch/first" || return 1
  if _cold_write_config "$config" "bad'path"; then return 1; fi
  cmp "$config" "$scratch/first" || return 1
  printf '# missing key and final newline' > "$config"
  _cold_export "$cold" || return 1
  assert_eq '# missing key and final newline' "$(head -1 "$config")" || return 1
  ( unset CONTINUUM_STORAGE_PATH; source "$config"; assert_eq "$cold" "$CONTINUUM_STORAGE_PATH" )
)

test_cold_storage_resumes_owned_migration() (
  source "$LIB"
  local scratch; scratch="$(mktemp -d)" || return 1
  scratch="$(cd "$scratch" && pwd -P)" || return 1
  trap 'rm -rf -- "$scratch"' EXIT
  export HOME="$scratch/home"
  local cold="$scratch/cold" src="$HOME/.cache/huggingface" config="$HOME/.continuum/config.env"
  mkdir -p "$src" "$HOME/.continuum" "$cold" || return 1
  printf '%s\n' "$cold" > "$HOME/.continuum/cold-storage.pending"
  printf '# retained until success\n' > "$config"
  printf 'first\n' > "$src/first"; printf 'second\n' > "$src/second"
  local fail_copy=1
  cp() {
    if [ "$fail_copy" = 1 ]; then command cp "$src/first" "$cold/huggingface/first"; return 42; fi
    command cp "$@"
  }
  _cold_drive() { echo 'must not change drives during interrupted migration' >&2; return 1; }
  if mod_cold_storage; then echo 'Partial copy accepted' >&2; return 1; fi
  assert_eq '# retained until success' "$(cat "$config")" || return 1
  [ -f "$cold/huggingface.continuum-migration" ] || return 1
  fail_copy=0
  mod_cold_storage || return 1
  [ ! -e "$src" ] && [ ! -e "$HOME/.continuum/cold-storage.pending" ] || return 1
  assert_eq first "$(cat "$cold/huggingface/first")" || return 1
  assert_eq second "$(cat "$cold/huggingface/second")" || return 1
  mod_cold_storage || return 1
  src="$HOME/.continuum/genome"
  mkdir -p "$src" "$cold/genome" || return 1
  printf kept > "$cold/genome/unrelated"
  if _cold_migrate "$src" "$cold/genome" "$cold"; then return 1; fi
  assert_eq kept "$(cat "$cold/genome/unrelated")" || return 1
  if _cold_migrate "$scratch" "$cold/genome" "$cold"; then return 1; fi
  local overlap_error
  if overlap_error="$(_cold_migrate "$src" "$HOME/.continuum/./genome" "$HOME/.continuum/." 2>&1)"; then return 1; fi
  assert_contains 'overlaps source' "$overlap_error" || return 1
  [ ! -e "$src.continuum-migration" ] || return 1
  mkdir -p "$scratch/foreign/huggingface" || return 1
  printf untouched > "$scratch/foreign/huggingface/keep"
  rmdir "$HOME/.cache" || return 1
  ln -s "$scratch/foreign" "$HOME/.cache" || return 1
  if [ ! -L "$HOME/.cache" ]; then
    case "$(uname -s)" in
      MINGW*|MSYS*) echo 'SKIP: Git Bash copied symlink fixture; native junction coverage runs in windows-service.test.ps1' >&2; return 0 ;;
      *) echo 'Symlink fixture did not create a link' >&2; return 1 ;;
    esac
  fi
  mkdir "$scratch/link-case-cold" || return 1
  if _cold_migrate "$HOME/.cache/huggingface" "$scratch/link-case-cold/huggingface" "$scratch/link-case-cold"; then return 1; fi
  assert_eq untouched "$(cat "$scratch/foreign/huggingface/keep")" || return 1
  rm "$HOME/.cache" || return 1
  ln -s "$cold" "$scratch/linked-cold" || return 1
  if _cold_migrate "$HOME/.cache/huggingface" "$scratch/linked-cold/huggingface" "$scratch/linked-cold"; then return 1; fi
)

# Permit a focused scratch-only test without executing installer tier tests.
if [ "${BASH_SOURCE[0]}" != "$0" ]; then return 0; fi

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
_run_test test_llama_cache_tracks_source_ownership
_run_test test_cold_storage_preserves_config
_run_test test_cold_storage_resumes_owned_migration

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
