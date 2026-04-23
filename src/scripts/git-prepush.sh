#!/bin/bash
# Git pre-push hook — compilation + test gate
# Runs before code reaches the remote. Fast enough to not block workflow,
# thorough enough to catch real problems.
#
# Skip with: git push --no-verify (when you know what you're doing)
set -e

START_TIME=$(date +%s)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RUST_DIR="$SRC_DIR/workers/continuum-core"

echo "🚀 PRE-PUSH: Compilation + test gate"
echo "====================================="

FAILED=0

# Phase 1: TypeScript compilation (<15s)
echo ""
echo "📋 Phase 1: TypeScript compilation"
echo "-----------------------------------"
TS_START=$(date +%s)
if cd "$SRC_DIR" && npm run build:ts > /dev/null 2>&1; then
    echo "✅ TypeScript: clean ($(( $(date +%s) - TS_START ))s)"
else
    echo "❌ TypeScript compilation FAILED"
    echo "   Run: cd src && npm run build:ts"
    FAILED=1
fi

# Phase 1b: ESLint — baseline-tolerant.
#
# Rationale: the repo has thousands of pre-existing ESLint violations
# accumulated over time (see eslint-baseline.txt for the count). Strict
# `--max-warnings 0` would block every push regardless of whether the
# pusher introduced anything new. We still want the gate — just one
# that catches REGRESSIONS, not historical state.
#
# How this works:
#   1. Run ESLint, count errors against the explicit glob (`.` is
#      "all ignored" in ESLint 9 with the current eslint.config.js).
#   2. Read eslint-baseline.txt — the recorded "acceptable" count.
#   3. Pass if current <= baseline. Fail if current > baseline (means
#      this push added new violations).
#   4. Suggest updating the baseline if current dropped substantially
#      (cleanup is welcome, but the baseline should track real state).
#
# Update baseline after a real cleanup pass:
#   cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 \
#     | grep -cE "error\s+" > eslint-baseline.txt
echo ""
echo "📋 Phase 1b: ESLint (baseline-tolerant)"
echo "----------------------------------------"
LINT_START=$(date +%s)
BASELINE_FILE="$SRC_DIR/eslint-baseline.txt"
if [ ! -f "$BASELINE_FILE" ]; then
    echo "⚠️  eslint-baseline.txt not present at $BASELINE_FILE — skipping ESLint gate."
    echo "   Generate it once with: cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 | grep -cE \"error\\s+\" > eslint-baseline.txt"
else
    BASELINE=$(cat "$BASELINE_FILE" | tr -d '[:space:]')
    CURRENT=$(cd "$SRC_DIR" && npx eslint './**/*.ts' --max-warnings 0 --quiet 2>&1 | grep -cE "error\s+" || true)
    LINT_DUR=$(( $(date +%s) - LINT_START ))
    if [ "$CURRENT" -le "$BASELINE" ]; then
        if [ "$CURRENT" -lt "$BASELINE" ]; then
            DROPPED=$(( BASELINE - CURRENT ))
            echo "✅ ESLint: $CURRENT errors (baseline $BASELINE, dropped $DROPPED — update eslint-baseline.txt to lock the win) (${LINT_DUR}s)"
        else
            echo "✅ ESLint: $CURRENT errors at baseline ($BASELINE) (${LINT_DUR}s)"
        fi
    else
        DELTA=$(( CURRENT - BASELINE ))
        echo "❌ ESLint: $CURRENT errors — baseline is $BASELINE, this push added $DELTA new violation(s)."
        echo "   Run to see what's new:"
        echo "   cd src && npx eslint './**/*.ts' --max-warnings 0 --quiet"
        FAILED=1
    fi
fi

# Phase 2: Rust compilation check (<20s cached)
echo ""
echo "📋 Phase 2: Rust compilation"
echo "----------------------------"
RUST_START=$(date +%s)
if [ -d "$RUST_DIR" ]; then
    if cd "$RUST_DIR" && cargo check 2>/dev/null; then
        echo "✅ Rust: clean ($(( $(date +%s) - RUST_START ))s)"
    else
        echo "❌ Rust compilation FAILED"
        echo "   Run: cd src/workers/continuum-core && cargo check"
        FAILED=1
    fi
else
    echo "⚠️  Rust directory not found (skipping)"
fi

# Phase 3: Rust tests (<30s cached)
# Use cargo's exit code as the canonical pass/fail signal — the
# previous `tail -1 | grep "test result: ok"` failed because cargo
# emits a trailing newline, so tail -1 saw an empty line and grep
# always returned no match. Exit code is the reliable test gate.
echo ""
echo "📋 Phase 3: Rust tests"
echo "----------------------"
TEST_START=$(date +%s)
if [ -d "$RUST_DIR" ]; then
    if (cd "$RUST_DIR" && cargo test --lib > /tmp/git-prepush-cargo.log 2>&1); then
        echo "✅ Rust tests: passed ($(( $(date +%s) - TEST_START ))s)"
    else
        echo "❌ Rust tests FAILED"
        echo "   Run: cd src/workers/continuum-core && cargo test --lib"
        echo "   Last output:"
        tail -10 /tmp/git-prepush-cargo.log | sed 's/^/      /'
        FAILED=1
    fi
else
    echo "⚠️  Rust directory not found (skipping)"
fi

# Phase 4: Native-arch Docker images (conditional)
# Fires only when the push touches Rust or Docker files. TS/docs/widget-
# only pushes skip — they don't affect the continuum-core/vulkan/cuda
# image binaries, so there's no point paying the ~20 min build cost.
#
# Background: CI's multi-arch QEMU builds (docker-images.yml) hit 5-6hr
# timeouts on PR #950 because linux/arm64 emulation on linux/amd64 GHA
# runners is pathologically slow. New strategy: each dev machine pushes
# its NATIVE arch, CI verifies coverage. See docs/architecture/
# PERSONA-AS-RUST-LIBRARY-PLAN.md and scripts/push-current-arch.sh.
echo ""
echo "📋 Phase 4: Native-arch Docker images (if Rust/docker changed)"
echo "---------------------------------------------------------------"

REPO_ROOT="$(cd "$SRC_DIR/.." && pwd)"
DOCKER_PUSH_START=$(date +%s)

# Git gives the pre-push hook a stdin stream of "local_ref local_sha
# remote_ref remote_sha" lines. Read each range; if any touches Rust or
# Docker paths, rebuild.
if [ -z "${PREPUSH_STDIN:-}" ]; then
    PREPUSH_STDIN="$(cat 2>/dev/null || true)"
fi

DOCKER_RELEVANT=0
ZERO_SHA="0000000000000000000000000000000000000000"
if [ -n "$PREPUSH_STDIN" ]; then
    while IFS=' ' read -r LOCAL_REF LOCAL_SHA REMOTE_REF REMOTE_SHA; do
        [ -z "$LOCAL_SHA" ] && continue
        [ "$LOCAL_SHA" = "$ZERO_SHA" ] && continue  # branch deletion
        if [ "$REMOTE_SHA" = "$ZERO_SHA" ]; then
            RANGE="$(git merge-base "$LOCAL_SHA" origin/main 2>/dev/null || echo "$LOCAL_SHA")..$LOCAL_SHA"
        else
            RANGE="$REMOTE_SHA..$LOCAL_SHA"
        fi
        CHANGED="$(git diff --name-only "$RANGE" 2>/dev/null || true)"
        if echo "$CHANGED" | grep -qE "^(src/workers/|docker/|src/shared/generated/|Cargo\.(toml|lock)$)"; then
            DOCKER_RELEVANT=1
            break
        fi
    done <<< "$PREPUSH_STDIN"
fi

if [ "$DOCKER_RELEVANT" -eq 0 ]; then
    echo "⏭️  No Rust/docker changes in this push — skipping native-arch build."
elif [ ! -x "$REPO_ROOT/scripts/push-current-arch.sh" ]; then
    echo "⚠️  scripts/push-current-arch.sh not found or not executable — skipping."
    echo "   CI will still gate via verify-architectures, but this machine's native"
    echo "   arch won't be pushed. Investigate the missing script."
else
    echo "→ Rust/docker changes detected. Building + pushing native-arch slices."
    echo "  This takes ~20 min per image (native, not QEMU)."
    echo "  Skip with: git push --no-verify (CI gate still catches missing arches)"
    echo ""
    if "$REPO_ROOT/scripts/push-current-arch.sh"; then
        echo "✅ Native-arch Docker push: done ($(( $(date +%s) - DOCKER_PUSH_START ))s)"
    else
        # Don't block the git push on docker push failure — verify-architectures
        # in CI gates the merge, so the user sees the miss at PR time. Better
        # to let the commit propagate with a loud warning than block on a
        # transient registry auth issue or Docker daemon hiccup.
        echo "⚠️  Native-arch Docker push FAILED — continuing with git push."
        echo "   CI's verify-architectures will block merge until resolved."
        echo "   Re-run manually: scripts/push-current-arch.sh"
    fi
fi

# Result
echo ""
echo "====================================="
TOTAL_TIME=$(( $(date +%s) - START_TIME ))

if [ $FAILED -ne 0 ]; then
    echo "❌ PRE-PUSH FAILED (${TOTAL_TIME}s)"
    echo "   Fix the errors above, then push again."
    echo "   Skip with: git push --no-verify"
    exit 1
fi

echo "✅ PRE-PUSH PASSED (${TOTAL_TIME}s)"
