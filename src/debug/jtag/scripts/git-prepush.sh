#!/bin/bash
# Git pre-push hook - Lightweight placeholder for future enhancements
set -e

START_TIME=$(date +%s)

echo "🚀 GIT PRE-PUSH: Lightweight validation"
echo "======================================"

# Phase 1: Basic Branch Protection
echo ""
echo "📋 Phase 1: Branch protection check"
echo "-----------------------------------"

CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
echo "🌿 Current branch: $CURRENT_BRANCH"

# Uncomment to block direct main branch pushes:
# if [ "$CURRENT_BRANCH" = "main" ]; then
#     echo "❌ Direct push to main branch blocked - use PR workflow"
#     exit 1
# fi

echo "✅ Branch protection: OK (no restrictions currently)"

# Phase 2: Quick Connectivity Check
echo ""
echo "📋 Phase 2: System health check"
echo "-------------------------------"

# Quick ping to ensure system is responsive
if ./jtag ping >/dev/null 2>&1; then
    echo "✅ JTAG system: responsive"
else
    echo "⚠️ JTAG system: not responsive (continuing anyway)"
fi

# Future enhancement placeholders:
# Phase 3: Remote integration tests
# Phase 4: Version consistency checks
# Phase 5: Security scans

echo ""
echo "🎉 PRE-PUSH VALIDATION COMPLETE!"
echo "================================"
echo "✅ All checks passed - proceeding with push"
echo "⏱️  Pre-push took: $(( $(date +%s) - START_TIME ))s (kept minimal)"

echo ""
echo "💡 Note: Comprehensive validation was already done in pre-commit"
echo "   This hook is just a lightweight safety net for future enhancements"