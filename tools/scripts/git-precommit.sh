#!/bin/bash
# Git pre-commit hook — fast Rust compile check before commit
# Lightweight gate: catches compilation errors early without the full test suite.
set -e

START_TIME=$(date +%s)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RUST_DIR="$REPO_ROOT/core/continuum-core"

# Determine if any Rust-relevant files are staged for commit.
staged_rust_files() {
    git diff --cached --name-only 2>/dev/null | grep -E \
        '^(core/|Cargo\.(toml|lock)$|core/.*/Cargo\.(toml|lock)$)' || true
}

echo "��� PRE-COMMIT: Rust compile check"
echo "==================================="

STAGED="$(staged_rust_files)"
if [ -z "$STAGED" ]; then
    echo "���️  No Rust-relevant files staged — skipping cargo check."
    exit 0
fi

# Source GPU feature flags for the platform (same as pre-push).
echo ""
echo "��� Phase: Rust compilation (staged)"
echo "------------------------------------"
RUST_START=$(date +%s)
if [ -d "$RUST_DIR" ]; then
    source "$(dirname "$0")/shared/cargo-features.sh"
    # --message-format=short: avoids annotate-snippets ICE in large crates.
    if (cd "$RUST_DIR" && cargo check --message-format=short $CARGO_GPU_FEATURES 2>/dev/null); then
        echo "✅ Rust staged: clean ($(( $(date +%s) - RUST_START ))s) ${CARGO_GPU_FEATURES:-[cpu-only]}"
    else
        echo "�� Rust compilation FAILED on staged changes"
        echo "   Run: cd core/continuum-core && cargo check $CARGO_GPU_FEATURES"
        exit 1
    fi
else
    echo "���️  Rust directory not found (skipping)"
fi

echo ""
echo "✅ Pre-commit checks complete ($(( $(date +%s) - START_TIME ))s)"
exit 0
