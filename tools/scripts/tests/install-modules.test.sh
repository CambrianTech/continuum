#!/usr/bin/env bash
# Install-module tests against a SCRATCH HOME.
#
# Why this exists (2026-08-06): every install defect in this project has been
# found by a person hitting it on their own machine, because there is no way to
# exercise the install path without actually installing. The Windows CLI has
# been unreachable from PowerShell for its entire existence — `continuum` is
# linked into `~/.local/bin`, which MSYS adds to ITS shell PATH and Windows does
# not — and nobody saw it because everyone working on it lives in Git Bash.
# A test that stands somewhere none of us stands is worth more than any single
# fix it validates.
#
# Contract: this NEVER touches the operator's real HOME. Every module under test
# runs with HOME + XDG dirs redirected into a temp tree that is removed on exit.
# Modules that mutate system state (winget, sudo, registry) are deliberately NOT
# exercised here — see the "out of scope" note at the bottom.
#
# Run:  bash tools/scripts/tests/install-modules.test.sh
# Exit: 0 all passed, 1 any failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LIB="$REPO_ROOT/tools/scripts/lib/install-common.sh"

PASS=0
FAIL=0

ok()   { printf '  \033[32m✓\033[0m %s\n' "$1"; PASS=$((PASS+1)); }
bad()  { printf '  \033[31m✗\033[0m %s\n     %s\n' "$1" "${2:-}"; FAIL=$((FAIL+1)); }
note() { printf '  \033[2m·\033[0m %s\n' "$1"; }

# A scratch HOME per case. `module_fail` calls `exit 1`, so every module
# invocation runs in a SUBSHELL — otherwise one failing module kills the
# whole harness and every later test silently never runs (which would be
# this project's favourite bug shape, in the test suite itself).
with_scratch_home() {
  local body="$1"
  local tmp; tmp="$(mktemp -d "${TMPDIR:-/tmp}/continuum-install-test.XXXXXX")"
  (
    export HOME="$tmp"
    export XDG_DATA_HOME="$tmp/.local/share"
    export XDG_CONFIG_HOME="$tmp/.config"
    export CONTINUUM_HOME="$tmp/.continuum"
    mkdir -p "$HOME"
    # shellcheck source=/dev/null
    source "$LIB" >/dev/null 2>&1
    eval "$body"
  )
  local rc=$?
  rm -rf "$tmp"
  return $rc
}

printf '\n\033[1minstall modules — scratch HOME\033[0m\n'

# what this catches: the link module silently doing nothing, or landing
# somewhere the caller did not ask for. Reports WHICH tier fired, because the
# tier ladder (/usr/local/bin → sudo → ~/.local/bin) resolves differently on
# MSYS than on a real Unix box and that difference is load-bearing for #53.
if with_scratch_home '
  src="$HOME/src-continuum"; printf "#!/bin/sh\necho hi\n" > "$src"; chmod +x "$src"
  mod_continuum_bin_link "$src" >/dev/null 2>&1
  for cand in /usr/local/bin/continuum "$HOME/.local/bin/continuum"; do
    [ -x "$cand" ] && { echo "$cand" > "$HOME/.landed"; break; }
  done
  [ -s "$HOME/.landed" ]
'; then
  ok "mod_continuum_bin_link places the CLI somewhere on the tier ladder"
else
  bad "mod_continuum_bin_link placed nothing" "no continuum on any tier after a clean run"
fi

# what this catches: a non-idempotent installer. Re-running install is the
# documented UPDATE path ("re-run = update"), so a second run must skip, not
# redo or fail.
if with_scratch_home '
  src="$HOME/src-continuum"; printf "#!/bin/sh\necho hi\n" > "$src"; chmod +x "$src"
  mod_continuum_bin_link "$src" >/dev/null 2>&1
  out="$(mod_continuum_bin_link "$src" 2>&1)"
  printf "%s" "$out" | grep -qi "skip"
'; then
  ok "second run is idempotent (reports skipped)"
else
  bad "second run did not report a skip" "re-run is the documented update path; it must no-op when current"
fi

# what this catches: accepting a source that does not exist and "succeeding".
# A silent success here installs nothing and reports nothing — the exact
# class this project keeps finding in production.
#
# NOTE on the nested subshell: `module_fail` ends in `exit 1`, so you CANNOT
# write `! mod_continuum_bin_link ...` — the exit tears down the shell running
# the negation, which then reports failure and reads as "the module accepted
# it". That false negative was this harness's first result, and it was the
# harness being wrong, not the code. Run the module in its OWN subshell and
# inspect `$?`.
if with_scratch_home '
  ( mod_continuum_bin_link "$HOME/definitely-not-here" >/dev/null 2>&1 )
  [ $? -ne 0 ]
'; then
  ok "refuses a missing source binary (fails loud, non-zero)"
else
  bad "accepted a missing source binary" "must fail loudly, never install nothing quietly"
fi

# Informational, not an assertion: name the platform gap so the next reader
# does not have to rediscover it. On Windows this is the whole of defect #53.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    note "WINDOWS: the tier ladder targets Unix paths only. Neither /usr/local/bin"
    note "nor ~/.local/bin is on the WINDOWS user PATH — verified with"
    note "\`Get-Command continuum\` → not found, while Git Bash resolves it fine."
    note "There is no Windows tier in mod_continuum_bin_link. See task #53."
    ;;
esac

# Out of scope here, on purpose: anything that mutates machine state —
# winget/apt/brew installs, sudo, and the per-user PATH registration that #53
# needs. A scratch HOME cannot honestly exercise per-user PATH, so that one
# wants a second Windows user account rather than a fake HOME. Keeping those
# out is what makes this harness safe to run on the operator's own box.

printf '\n  %d passed, %d failed\n\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
