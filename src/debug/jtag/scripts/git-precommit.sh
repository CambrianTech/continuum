#!/bin/bash
set -e  # Exit immediately on any error

echo "🔒 GIT PRECOMMIT: Bulletproof validation with proof artifacts"
echo "=================================================="

# Navigate to the correct working directory
cd "$(dirname "$0")/.."

# Phase 1: Foundation Validation
echo ""
echo "📋 Phase 1: Compilation & Deployment"
echo "-------------------------------------"

echo "🔨 Running TypeScript compilation..."
npm run build:ts
echo "✅ TypeScript compilation passed"

echo "🚀 Starting system deployment..."
npm start &
DEPLOY_PID=$!

# Wait for system to be ready (with timeout)
echo "⏳ Waiting for system to be ready..."
TIMEOUT=120  # 2 minutes
COUNTER=0
while [ $COUNTER -lt $TIMEOUT ]; do
    if ./jtag ping >/dev/null 2>&1; then
        echo "✅ System deployment successful"
        break
    fi
    sleep 1
    COUNTER=$((COUNTER + 1))
    if [ $((COUNTER % 10)) -eq 0 ]; then
        echo "   ... still waiting ($COUNTER/${TIMEOUT}s)"
    fi
done

if [ $COUNTER -eq $TIMEOUT ]; then
    echo "❌ System deployment timed out after ${TIMEOUT}s"
    kill $DEPLOY_PID 2>/dev/null || true
    exit 1
fi

# Phase 2: Integration Testing
echo ""
echo "🧪 Phase 2: CRUD + Chat Integration (100% required)"
echo "---------------------------------------------------"

echo "🧪 Running CRUD + Widget integration test..."

# Capture test output and exit code
TEST_OUTPUT=$(npx tsx tests/integration/crud-db-widget.test.ts 2>&1)
TEST_EXIT_CODE=$?

# Check if test passed AND contains 100% success
if [ $TEST_EXIT_CODE -eq 0 ] && echo "$TEST_OUTPUT" | grep -q "100.0%"; then
    echo "✅ CRUD integration test: 100% PASSED"

    # Extract success metrics for commit message
    PASS_RATE=$(echo "$TEST_OUTPUT" | grep -o "[0-9]/[0-9] passed ([0-9.]*%)" | head -1)
    echo "📊 Test results: $PASS_RATE"

    # Store test results for commit message
    TEST_SUMMARY="CRUD Integration: $PASS_RATE - ALL TESTS PASSED"
else
    echo "❌ CRUD integration test FAILED - blocking commit"
    echo "Test output:"
    echo "$TEST_OUTPUT"
    exit 1
fi

# Phase 3: Screenshot Proof Collection
echo ""
echo "📸 Phase 3: Collecting visual proof"
echo "-----------------------------------"

echo "📸 Capturing widget screenshots for proof..."
./jtag screenshot --querySelector="user-list-widget" --filename="precommit-users.png"
./jtag screenshot --querySelector="room-list-widget" --filename="precommit-rooms.png"
./jtag screenshot --querySelector="chat-widget" --filename="precommit-chat.png"
./jtag screenshot --querySelector="body" --filename="precommit-system.png"

echo "✅ Screenshot proof collection complete"

# Phase 4: Session Promotion to Validation Directory
echo ""
echo "📦 Phase 4: Promoting session artifacts to validation"
echo "----------------------------------------------------"

# Find the current active session
CURRENT_SESSION_LINK="examples/widget-ui/.continuum/jtag/currentUser"
if [ -L "$CURRENT_SESSION_LINK" ]; then
    CURRENT_SESSION=$(readlink "$CURRENT_SESSION_LINK")
    SESSION_PATH="examples/widget-ui/.continuum/jtag/$CURRENT_SESSION"

    if [ -d "$SESSION_PATH" ]; then
        echo "🔍 Current session: $CURRENT_SESSION"

        # Generate unique run ID for this validation
        RUN_ID=$(date +%s)_$(head /dev/urandom | tr -dc A-Za-z0-9 | head -c8)
        VALIDATION_DIR=".continuum/test/validation/run_${RUN_ID}"

        # Create validation directory structure
        mkdir -p "$VALIDATION_DIR"

        # Copy successful session artifacts
        cp -r "$SESSION_PATH/logs" "$VALIDATION_DIR/"
        cp -r "$SESSION_PATH/screenshots" "$VALIDATION_DIR/"

        # Create session-info.json (matching the format from last commit)
        cat > "$VALIDATION_DIR/session-info.json" << EOF
{
  "runId": "$RUN_ID",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "sessionId": "$CURRENT_SESSION",
  "validationType": "precommit",
  "status": "PASSED",
  "testSummary": "$TEST_SUMMARY",
  "typescriptCompilation": "PASSED",
  "systemDeployment": "PASSED",
  "screenshotsCaptured": 4,
  "validationPhases": [
    "TypeScript Compilation",
    "System Deployment",
    "CRUD + Chat Integration",
    "Screenshot Proof Collection",
    "Session Promotion"
  ]
}
EOF

        # Add validation artifacts to git (force add to override .gitignore)
        git add -f "$VALIDATION_DIR/"

        echo "✅ Session artifacts promoted to validation directory"
        echo "📋 Validation run: $VALIDATION_DIR"
    else
        echo "❌ Current session directory not found: $SESSION_PATH"
        exit 1
    fi
else
    echo "❌ Current session symlink not found: $CURRENT_SESSION_LINK"
    exit 1
fi

# Phase 5: Final Validation
echo ""
echo "🔎 Phase 5: Final validation check"
echo "----------------------------------"

# Verify all proof artifacts exist
REQUIRED_ARTIFACTS=(
    "$VALIDATION_DIR/logs"
    "$VALIDATION_DIR/screenshots"
    "$VALIDATION_DIR/session-info.json"
)

for artifact in "${REQUIRED_ARTIFACTS[@]}"; do
    if [ -e "$artifact" ]; then
        echo "✅ $artifact"
    else
        echo "❌ Missing required artifact: $artifact"
        exit 1
    fi
done

# Phase 6: Commit Message Enhancement
echo ""
echo "📝 Phase 6: Enhancing commit message"
echo "------------------------------------"

# Create validation summary for commit message
VALIDATION_SUMMARY=$(cat << EOF


🤖 Precommit Validation Results:
✅ TypeScript compilation: PASSED
✅ System deployment: PASSED
✅ $TEST_SUMMARY
✅ Screenshot proof: 4 captured
✅ Session artifacts: PROMOTED ($VALIDATION_DIR)
✅ Validation timestamp: $(date -u +"%Y-%m-%dT%H:%M:%SZ")

🔒 Bulletproof commit - all systems validated
EOF
)

# Append validation to commit message if we're in a git commit
if [ -n "$GIT_COMMIT_MESSAGE_FILE" ] && [ -f "$GIT_COMMIT_MESSAGE_FILE" ]; then
    echo "$VALIDATION_SUMMARY" >> "$GIT_COMMIT_MESSAGE_FILE"
    echo "📝 Validation results appended to commit message"
elif [ -f ".git/COMMIT_EDITMSG" ]; then
    echo "$VALIDATION_SUMMARY" >> ".git/COMMIT_EDITMSG"
    echo "📝 Validation results appended to commit message"
fi

echo ""
echo "🎉 PRECOMMIT VALIDATION COMPLETE!"
echo "=================================================="
echo "✅ TypeScript compilation: PASSED"
echo "✅ System deployment: PASSED"
echo "✅ CRUD + Chat integration: 100% PASSED"
echo "✅ Screenshot proof: COLLECTED"
echo "✅ Session artifacts: PROMOTED"
echo "✅ All validation artifacts included in commit"
echo ""
echo "🚀 Commit approved - system is bulletproof!"