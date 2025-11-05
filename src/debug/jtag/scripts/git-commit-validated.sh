#!/bin/bash
set -e

# Validated Git Commit - runs validation, stages artifacts, then commits
# Usage: ./git-commit-validated.sh "commit message"

if [ -z "$1" ]; then
    echo "Usage: $0 \"commit message\""
    exit 1
fi

COMMIT_MSG="$1"

cd "$(dirname "$0")/.."

echo "🔒 VALIDATED GIT COMMIT"
echo "======================="
echo ""

# Step 1: Run validation (this will fail if tests fail)
echo "📋 Step 1: Running validation..."
./scripts/git-precommit.sh

# Step 2: Find the validation artifacts that were created in jtag validation dir
VALIDATION_DIR=$(ls -td .continuum/sessions/validation/run_* 2>/dev/null | head -1)
if [ -z "$VALIDATION_DIR" ]; then
    echo "❌ No validation artifacts found"
    exit 1
fi

echo "✅ Validation passed"
echo ""

# Step 3: Stage the validation artifacts from jtag location (no moving to repo root)
echo "📋 Step 2: Staging validation artifacts..."
REPO_ROOT="../../../"

cd "$REPO_ROOT"
# Stage the validation directory from jtag location
git add "src/debug/jtag/$VALIDATION_DIR"
cd - > /dev/null

echo "✅ Validation artifacts staged at src/debug/jtag/$VALIDATION_DIR"
echo ""

# Step 4: Perform the actual commit
echo "📋 Step 3: Creating commit..."
cd "$REPO_ROOT"
git commit -m "$COMMIT_MSG"

echo ""
echo "✅ Commit created with validation artifacts"
