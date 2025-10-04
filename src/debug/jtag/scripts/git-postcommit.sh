#!/bin/bash

# Post-commit hook to rename validation artifacts with actual commit hash and amend

cd "$(dirname "$0")/.."
REPO_ROOT="../../../"

# Check if there's a validation run to process
TEMP_ID_FILE="${REPO_ROOT}.continuum/.precommit-validation-id"
if [ ! -f "$TEMP_ID_FILE" ]; then
    exit 0
fi

TEMP_VALIDATION_ID=$(cat "$TEMP_ID_FILE")
TEMP_DIR="${REPO_ROOT}.continuum/sessions/validation/run_${TEMP_VALIDATION_ID}"

if [ ! -d "$TEMP_DIR" ]; then
    rm -f "$TEMP_ID_FILE"
    exit 0
fi

# Get the actual commit hash
COMMIT_HASH=$(git rev-parse HEAD)
FINAL_DIR="${REPO_ROOT}.continuum/sessions/validation/run_${COMMIT_HASH:0:12}"

echo "🔄 Post-commit: Renaming validation run with actual commit hash..."
echo "   From: run_${TEMP_VALIDATION_ID}"
echo "   To: run_${COMMIT_HASH:0:12}"

# Rename the validation directory
mv "$TEMP_DIR" "$FINAL_DIR"

# Stage and amend the commit to include validation artifacts
cd "$REPO_ROOT"
git add ".continuum/sessions/validation/run_${COMMIT_HASH:0:12}"
git commit --amend --no-edit --no-verify

# Clean up
rm -f ".continuum/.precommit-validation-id"

echo "✅ Validation artifacts included in commit with hash ${COMMIT_HASH:0:12}"
