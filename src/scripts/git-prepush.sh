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
