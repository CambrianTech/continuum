#!/bin/bash
# cargo-test.sh — `cargo test` wrapper that auto-applies platform GPU features.
#
# Why this exists:
#   continuum-core's vendored `llama` crate intentionally requires `--features
#   metal` (macOS) or `--features cuda` (Linux+Nvidia) so the build refuses to
#   produce a CPU-only inference binary (per the no-CPU-fallback alpha
#   contract — see #1262 + tests/no_cpu_fallback_contract.rs). The guard is
#   correct, but it makes the obvious developer command fail:
#
#     cd core/continuum-core && cargo test tick_db_handle --lib
#       → fails in the llama crate before the test runs
#
#   Fresh installs and agents repeatedly hit this. The fix is a wrapper that
#   reuses the same `scripts/shared/cargo-features.sh` detector that build
#   scripts and the precommit hook already source, so `cargo test` Just
#   Works on every platform.
#
# Usage (runs from anywhere — paths resolve off this script's location):
#
#   ./tools/scripts/cargo-test.sh tick_db_handle --lib
#   ./tools/scripts/cargo-test.sh --test no_cpu_fallback_contract
#   ./tools/scripts/cargo-test.sh --lib -- --test-threads=1
#
# Integration tests that link a `test-fixtures`-gated symbol are declared with
# `required-features = ["test-fixtures"]` in Cargo.toml, so a bare run SKIPS
# them cleanly; add `--features test-fixtures` to include them.
#
# All arguments after the script name pass through to `cargo test`. The
# wrapper appends the platform feature flags via $CARGO_GPU_FEATURES.
#
# Environment overrides (advanced):
#   CARGO_TEST_RUST_PACKAGE  — workspace package to test (default: continuum-core)
#   CARGO_TEST_NO_FEATURES=1 — skip the auto-feature append (CI-only debug;
#                              the macOS llama guard will fail without it)
#
# Related (#1257): same pattern as `scripts/git-prepush.sh` Phase 3 cargo
# test, hoisted from precommit-internal to a developer-facing entry point.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Repo root: tools/scripts → tools → repo root. (Post-reorg the Rust crates
# live under <repo>/core/, not the old <repo>/src/workers/ layout.)
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Source the platform GPU feature detector. This is the single source of
# truth for "what features does this platform need?" — same file that
# build-with-loud-failure.sh and git-prepush.sh source. Keeps this wrapper
# from drifting from the rest of the build matrix.
# shellcheck disable=SC1091
source "$SCRIPT_DIR/shared/cargo-features.sh"

PACKAGE="${CARGO_TEST_RUST_PACKAGE:-continuum-core}"
RUST_DIR="$REPO_ROOT/core/$PACKAGE"

if [ ! -d "$RUST_DIR" ]; then
  echo "ERROR: package directory not found: $RUST_DIR" >&2
  echo "  Set CARGO_TEST_RUST_PACKAGE=<name> to target a different workspace package." >&2
  exit 1
fi

if [ "${CARGO_TEST_NO_FEATURES:-0}" = "1" ]; then
  echo "⚠️  CARGO_TEST_NO_FEATURES=1 — running without platform GPU features."
  echo "    This will fail on macOS due to the no-CPU-fallback llama guard."
  FEATURES_ARG=""
else
  FEATURES_ARG="$CARGO_GPU_FEATURES"
fi

echo "🧪 cargo test for $PACKAGE"
echo "   features:    ${FEATURES_ARG:-<none — Linux CPU mode>}"
echo "   args:        $*"
echo "   cwd:         $RUST_DIR"
echo

cd "$RUST_DIR"
# shellcheck disable=SC2086
exec cargo test "$@" $FEATURES_ARG
