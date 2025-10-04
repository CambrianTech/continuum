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

# Step 2: Find the validation artifacts that were created
VALIDATION_DIR=$(ls -td .continuum/sessions/validation/run_* 2>/dev/null | head -1)
if [ -z "$VALIDATION_DIR" ]; then
    echo "❌ No validation artifacts found"
    exit 1
fi

echo "✅ Validation passed"
echo ""

# Step 3: Move validation artifacts to repo root (not copy)
echo "📋 Step 2: Staging validation artifacts..."
REPO_ROOT="../../../"
VALIDATION_ID=$(basename "$VALIDATION_DIR" | sed 's/run_//')
REPO_VALIDATION_DIR="${REPO_ROOT}.continuum/sessions/validation/run_${VALIDATION_ID}"

# Ensure parent directory exists
mkdir -p "${REPO_ROOT}.continuum/sessions/validation"

# Move the session directory to repo root
mv "$VALIDATION_DIR" "$REPO_VALIDATION_DIR"

# Stage the validation artifacts
cd "$REPO_ROOT"
git add ".continuum/sessions/validation/run_${VALIDATION_ID}"
cd - > /dev/null

echo "✅ Validation artifacts staged"
echo ""

# Step 4: Perform the actual commit
echo "📋 Step 3: Creating commit..."
cd "$REPO_ROOT"
git commit -m "$COMMIT_MSG"

echo ""
echo "✅ Commit created with validation artifacts"
