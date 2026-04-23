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

# Phase 1b: ESLint — zero tolerance for any, malformed types, etc.
echo ""
echo "📋 Phase 1b: ESLint"
echo "--------------------"
LINT_START=$(date +%s)
if cd "$SRC_DIR" && npx eslint . --max-warnings 0 --quiet > /dev/null 2>&1; then
    echo "✅ ESLint: clean ($(( $(date +%s) - LINT_START ))s)"
else
    echo "❌ ESLint FAILED — run: cd src && npm run lint"
    FAILED=1
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
echo ""
echo "📋 Phase 3: Rust tests"
echo "----------------------"
TEST_START=$(date +%s)
if [ -d "$RUST_DIR" ]; then
    if cd "$RUST_DIR" && cargo test --lib 2>/dev/null | tail -1 | grep -q "^test result: ok"; then
        echo "✅ Rust tests: passed ($(( $(date +%s) - TEST_START ))s)"
    else
        echo "❌ Rust tests FAILED"
        echo "   Run: cd src/workers/continuum-core && cargo test --lib"
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
