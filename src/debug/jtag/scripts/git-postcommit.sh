#!/bin/bash
set -e  # Exit immediately on any error

echo "🧹 GIT POST-COMMIT: Cleaning up validation artifacts"
echo "================================================="

# Navigate to the correct working directory
cd "$(dirname "$0")/.."

# Phase 1: Cleanup validation artifacts from working directory
echo ""
echo "🧹 Phase 1: Cleaning up validation artifacts"
echo "-------------------------------------------"

# Find and remove validation run directories (but keep them in git history)
VALIDATION_DIRS=$(find .continuum/sessions/validation -name "run_*" -type d 2>/dev/null || true)

if [ -n "$VALIDATION_DIRS" ]; then
    echo "🔍 Found validation directories to clean:"
    echo "$VALIDATION_DIRS" | while read -r dir; do
        if [ -d "$dir" ]; then
            echo "   🗑️  Removing: $dir"
            rm -rf "$dir"
        fi
    done
    echo "✅ Validation artifacts cleaned from working directory"
else
    echo "ℹ️  No validation artifacts found to clean"
fi

# Phase 2: Clean up any orphaned validation summary files
echo ""
echo "🧹 Phase 2: Cleaning up validation summary files"
echo "-----------------------------------------------"

VALIDATION_SUMMARY=".continuum/sessions/validation/latest-validation-summary.txt"
if [ -f "$VALIDATION_SUMMARY" ]; then
    echo "🗑️  Removing validation summary: $VALIDATION_SUMMARY"
    rm -f "$VALIDATION_SUMMARY"
    echo "✅ Validation summary cleaned"
else
    echo "ℹ️  No validation summary found to clean"
fi

# Phase 3: Final verification
echo ""
echo "🔎 Phase 3: Verifying cleanup complete"
echo "------------------------------------"

REMAINING_VALIDATION=$(find .continuum/sessions/validation -name "run_*" -type d 2>/dev/null || true)
if [ -z "$REMAINING_VALIDATION" ]; then
    echo "✅ All validation artifacts successfully cleaned"
else
    echo "⚠️  Some validation artifacts remain:"
    echo "$REMAINING_VALIDATION"
fi

echo ""
echo "🎉 POST-COMMIT CLEANUP COMPLETE!"
echo "================================="
echo "✅ Validation artifacts preserved in git history"
echo "✅ Working directory cleaned of validation remnants"
echo "🚀 Ready for next development cycle!"