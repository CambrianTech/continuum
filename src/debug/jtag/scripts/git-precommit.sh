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

# Phase 4: Session Artifacts Collection (Following Legacy Git Hook Pattern)
echo ""
echo "📦 Phase 4: Collecting complete session artifacts for commit inclusion"
echo "---------------------------------------------------------------------"

# Get current commit hash for validation directory naming (following legacy pattern)
COMMIT_HASH=$(git rev-parse HEAD 2>/dev/null || echo "fallback-$(date +%s)")
VALIDATION_RUN_DIR=".continuum/sessions/validation/run_${COMMIT_HASH:0:12}"

echo "🔍 Creating validation directory: $VALIDATION_RUN_DIR"
mkdir -p "$VALIDATION_RUN_DIR"

# Find the current active session using currentUser symlink (EXACT legacy pattern)
CURRENT_SESSION_LINK="examples/widget-ui/.continuum/jtag/currentUser"
if [ -L "$CURRENT_SESSION_LINK" ]; then
    CURRENT_SESSION=$(readlink "$CURRENT_SESSION_LINK")
    SESSION_PATH="examples/widget-ui/.continuum/jtag/$CURRENT_SESSION"

    if [ -d "$SESSION_PATH" ]; then
        echo "🔍 Current session: $CURRENT_SESSION"

        # Copy ENTIRE session directory to validation (following legacy pattern Line 210)
        echo "📋 Copying complete session directory to validation..."
        cp -r "$SESSION_PATH"/* "$VALIDATION_RUN_DIR/"
        echo "✅ Complete session copied to validation directory"

        # Add test results to the validation directory
        echo "$TEST_OUTPUT" > "$VALIDATION_RUN_DIR/test-results.txt"
        echo "✅ Test results added to session artifacts"

        # Create validation metadata (enhanced session-info.json)
        cat > "$VALIDATION_RUN_DIR/validation-info.json" << EOF
{
  "runId": "${COMMIT_HASH:0:12}",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "sessionId": "$CURRENT_SESSION",
  "validationType": "precommit",
  "status": "PASSED",
  "testSummary": "$TEST_SUMMARY",
  "testResults": {
    "exitCode": $TEST_EXIT_CODE,
    "outputLines": $(echo "$TEST_OUTPUT" | wc -l),
    "outputFile": "test-results.txt"
  },
  "validationPhases": [
    "TypeScript Compilation",
    "System Deployment",
    "CRUD + Chat Integration (100% Required)",
    "Screenshot Proof Collection",
    "Complete Session Copy"
  ]
}
EOF

        # CRITICAL: Force add validation files to git (following legacy pattern Line 258)
        echo "📋 Adding validation files to git (force add to override .gitignore)..."
        git add -f "$VALIDATION_RUN_DIR/"

        # Verify validation files are staged (following legacy pattern Lines 261-267)
        echo "🔍 Verifying validation files are staged for commit..."
        STAGED_FILES=$(git diff --cached --name-only)
        STAGED_VALIDATION_FILES=$(echo "$STAGED_FILES" | grep "$VALIDATION_RUN_DIR" || true)

        if [ -z "$STAGED_VALIDATION_FILES" ]; then
            echo "❌ COMMIT REJECTED: No validation files staged - validation session is your KEY to get in!"
            exit 1
        fi

        echo "✅ VALIDATION KEY ACCEPTED: Complete session artifacts staged for commit"
        echo "📁 Validation session: $VALIDATION_RUN_DIR"
        echo "🔑 Session artifacts included: logs, screenshots, session metadata"
        echo "📝 Test results included: test-results.txt, validation-info.json"

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
echo "📝 Phase 6: Preparing validation summary for commit message"
echo "-----------------------------------------------------------"

# Create validation summary matching existing commit format
VALIDATION_SUMMARY=$(cat << EOF
🔍 JTAG INTEGRATION TEST: ✅ $TEST_SUMMARY - All validation phases completed
🛡️ Git Hook Validation: ✅ All 6 phases passed (TypeScript → Artifacts → Message Enhancement)
EOF
)

# Save validation summary for prepare-commit-msg hook to use
VALIDATION_SUMMARY_DIR=".continuum/sessions/validation"
mkdir -p "$VALIDATION_SUMMARY_DIR"
echo "$VALIDATION_SUMMARY" > "$VALIDATION_SUMMARY_DIR/latest-validation-summary.txt"
echo "📝 Validation summary saved for commit message enhancement"

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