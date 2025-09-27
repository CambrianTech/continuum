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

# Phase 4: Test Results & Session Artifacts Collection
echo ""
echo "📦 Phase 4: Collecting test results and session artifacts"
echo "--------------------------------------------------------"

# Generate unique run ID for this validation
RUN_ID=$(date +%s)_$(head /dev/urandom | tr -dc A-Za-z0-9 | head -c8)
VALIDATION_DIR=".continuum/test/validation/run_${RUN_ID}"

# Create validation directory structure
mkdir -p "$VALIDATION_DIR"

# Save the complete test output for commit inclusion
echo "$TEST_OUTPUT" > "$VALIDATION_DIR/test-results.txt"
echo "✅ Test results saved to validation directory"

# Find and copy latest session artifacts if they exist
LATEST_VALIDATION=".continuum/test-validation/latest"
if [ -d "$LATEST_VALIDATION" ]; then
    echo "🔍 Found existing validation artifacts at $LATEST_VALIDATION"

    # Copy logs and screenshots from latest validation
    if [ -d "$LATEST_VALIDATION/logs" ]; then
        cp -r "$LATEST_VALIDATION/logs" "$VALIDATION_DIR/"
        echo "✅ Copied validation logs"
    fi

    if [ -d "$LATEST_VALIDATION/screenshots" ]; then
        cp -r "$LATEST_VALIDATION/screenshots" "$VALIDATION_DIR/"
        echo "✅ Copied validation screenshots"
    fi
else
    echo "⚠️ No existing validation artifacts found - screenshots will be captured fresh"
fi

# Also check system logs directory
SYSTEM_LOGS=".continuum/jtag/system/logs"
if [ -d "$SYSTEM_LOGS" ]; then
    mkdir -p "$VALIDATION_DIR/system-logs"
    cp "$SYSTEM_LOGS"/*.log "$VALIDATION_DIR/system-logs/" 2>/dev/null || true
    echo "✅ Copied system logs"
fi

# Create comprehensive session-info.json with test results
cat > "$VALIDATION_DIR/session-info.json" << EOF
{
  "runId": "$RUN_ID",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
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
    "Test Results & Session Artifacts Collection"
  ],
  "artifacts": {
    "testResults": "test-results.txt",
    "logs": "logs/",
    "screenshots": "screenshots/",
    "systemLogs": "system-logs/"
  }
}
EOF

# Add ALL validation artifacts to git (force add to override .gitignore)
git add -f "$VALIDATION_DIR/"

echo "✅ Test results and session artifacts collected for commit"
echo "📋 Validation run: $VALIDATION_DIR"
echo "📁 Artifacts included in commit:"
echo "   - Complete test output: test-results.txt"
echo "   - Session logs: logs/"
echo "   - Screenshots: screenshots/"
echo "   - System logs: system-logs/"
echo "   - Validation metadata: session-info.json"

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

# Create validation summary matching existing commit format
VALIDATION_SUMMARY=$(cat << EOF

🔍 JTAG INTEGRATION TEST: ✅ $TEST_SUMMARY - All validation phases completed
🛡️ Git Hook Validation: ✅ All 5 phases passed (TypeScript → Artifacts Collection)
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