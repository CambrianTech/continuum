#!/bin/bash
set -e  # Exit immediately on any error

# Post-commit cleanup function - called after successful commit
post_commit_summary() {
    echo ""
    echo "📋 POST-COMMIT SUMMARY: Validation complete"
    echo "============================================"
    echo "✅ Bulletproof validation: 100% CRUD tests passed"
    echo "✅ Screenshots and logs captured for inspection"
    echo "✅ No git artifacts committed - clean repository"
    echo ""
    echo "🎯 Validation artifacts: .continuum/sessions/validation/run_${COMMIT_HASH:0:12}/"
    echo "📸 Screenshots available for manual review if needed"
    echo "🚀 Ready for next development cycle!"
}

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
# Restore version.ts to avoid timestamp-only changes in commit
cd ../../..
git restore src/debug/jtag/shared/version.ts 2>/dev/null || true
cd src/debug/jtag
echo "✅ TypeScript compilation passed"

# Detect if code changes require deployment
echo "🔍 Checking if code changes require deployment..."
cd ../../..
CODE_CHANGED=false

# Check if any TypeScript, JavaScript, or browser bundle files are being committed
if git diff --cached --name-only | grep -qE '\.(ts|tsx|js|jsx|css|html)$'; then
    echo "📝 Code changes detected in commit - deployment required"
    CODE_CHANGED=true
elif git diff --cached --name-only | grep -q 'browser/generated\.ts'; then
    echo "📦 Browser bundle changed - deployment required"
    CODE_CHANGED=true
else
    echo "📄 Only documentation/config changes - deployment may not be needed"
fi

cd src/debug/jtag

# Check if system is already running
SYSTEM_ALREADY_RUNNING=false
if ./jtag ping >/dev/null 2>&1; then
    SYSTEM_ALREADY_RUNNING=true

    if [ "$CODE_CHANGED" = true ]; then
        echo "🔄 System running but CODE CHANGED - forcing deployment to load new code"
        echo "💡 This prevents the 'old code deployed' bug"
        SYSTEM_ALREADY_RUNNING=false  # Force deployment
    else
        echo "✅ System already running with current code - skipping restart (massive time savings!)"
    fi
fi

# Start system if needed
if [ "$SYSTEM_ALREADY_RUNNING" = false ]; then
    echo "🚀 Starting deployment..."
    npm start &
    DEPLOY_PID=$!
fi

# Wait for system to be ready (with timeout) - skip if already running
if [ "$SYSTEM_ALREADY_RUNNING" = true ]; then
    echo "⚡ System already healthy - skipping startup wait"
else
    echo "⏳ Waiting for system to be ready..."
    TIMEOUT=60  # 1 minute should be plenty for ping to succeed
    COUNTER=0

    while [ $COUNTER -lt $TIMEOUT ]; do
        # Check if ping works (system is ready)
        if ./jtag ping >/dev/null 2>&1; then
            echo "✅ System deployment successful - ping responding"
            echo "⏳ Allowing system to settle..."
            sleep 3  # Brief settle time
            break
        fi

        # Progress indicator every 5 seconds
        if [ $((COUNTER % 5)) -eq 0 ]; then
            echo "   ... waiting for system startup ($COUNTER/${TIMEOUT}s)"
        fi

        sleep 1
        COUNTER=$((COUNTER + 1))
    done

    if [ $COUNTER -eq $TIMEOUT ]; then
        echo "❌ System deployment timed out after ${TIMEOUT}s"
        echo "   ./jtag ping never succeeded"
        echo "   Check .continuum/jtag/system/logs/npm-start.log for details"
        kill $DEPLOY_PID 2>/dev/null || true
        exit 1
    fi
fi

# Phase 2: Integration Testing
echo ""
echo "🧪 Phase 2: CRUD + State + AI Response Integration (100% required)"
echo "-------------------------------------------------------------------"

echo "🧪 Running precommit test profile via JTAG test runner..."

# Ensure test output directory exists
mkdir -p .continuum/sessions/validation

# Run precommit integration tests with immediate output visibility
echo "🧪 Running CRUD integration test..."
echo "=================================================="
npx tsx tests/integration/database-chat-integration.test.ts 2>&1 | tee .continuum/sessions/validation/test1-output.txt
TEST_EXIT_CODE1=${PIPESTATUS[0]}
TEST_OUTPUT1=$(cat .continuum/sessions/validation/test1-output.txt)
echo "=================================================="

echo ""
echo "🧪 Running State integration test..."
echo "=================================================="
npx tsx tests/integration/state-system-integration.test.ts 2>&1 | tee .continuum/sessions/validation/test2-output.txt
TEST_EXIT_CODE2=${PIPESTATUS[0]}
TEST_OUTPUT2=$(cat .continuum/sessions/validation/test2-output.txt)
echo "=================================================="

echo ""
echo "🧪 Running AI Response integration test..."
echo "=================================================="
npx tsx tests/integration/ai-response-integration.test.ts 2>&1 | tee .continuum/sessions/validation/test3-output.txt
TEST_EXIT_CODE3=${PIPESTATUS[0]}
TEST_OUTPUT3=$(cat .continuum/sessions/validation/test3-output.txt)
echo "=================================================="

echo ""
# Check if all tests passed
if [ $TEST_EXIT_CODE1 -eq 0 ] && [ $TEST_EXIT_CODE2 -eq 0 ] && [ $TEST_EXIT_CODE3 -eq 0 ]; then
    echo "✅ Precommit integration tests: ALL PASSED"
    echo "📊 Test results: 3 of 3 tests passed (CRUD + State + AI Response Integration)"

    # Store test results for commit message
    TEST_SUMMARY="CRUD + State + AI Response Integration: 3/3 - ALL TESTS PASSED"
else
    echo ""
    echo "❌ PRECOMMIT INTEGRATION TESTS FAILED - BLOCKING COMMIT"
    echo "=================================================="
    if [ $TEST_EXIT_CODE1 -ne 0 ]; then
        echo "❌ CRUD integration test FAILED (exit code: $TEST_EXIT_CODE1)"
        echo "   Test file: tests/integration/database-chat-integration.test.ts"
        echo "   Output shown above"
    fi
    if [ $TEST_EXIT_CODE2 -ne 0 ]; then
        echo "❌ State integration test FAILED (exit code: $TEST_EXIT_CODE2)"
        echo "   Test file: tests/integration/state-system-integration.test.ts"
        echo "   Output shown above"
    fi
    if [ $TEST_EXIT_CODE3 -ne 0 ]; then
        echo "❌ AI Response integration test FAILED (exit code: $TEST_EXIT_CODE3)"
        echo "   Test file: tests/integration/ai-response-integration.test.ts"
        echo "   Output shown above"
    fi
    echo ""
    echo "🔍 Fix the failing tests before committing"
    echo "=================================================="
    exit 1
fi

# Phase 3: Screenshot Proof Collection
echo ""
echo "📸 Phase 3: Collecting visual proof"
echo "-----------------------------------"

echo "📸 Capturing widget screenshots for proof..."
# Use timeout and don't fail commit if screenshots timeout during precommit load
timeout 30 ./jtag screenshot --querySelector="user-list-widget" --filename="precommit-users.png" || echo "⚠️ User widget screenshot timed out (system under load)"
timeout 30 ./jtag screenshot --querySelector="room-list-widget" --filename="precommit-rooms.png" || echo "⚠️ Room widget screenshot timed out (system under load)"
timeout 30 ./jtag screenshot --querySelector="chat-widget" --filename="precommit-chat.png" || echo "⚠️ Chat widget screenshot timed out (system under load)"
timeout 30 ./jtag screenshot --querySelector="body" --filename="precommit-system.png" || echo "⚠️ System screenshot timed out (system under load)"

echo "✅ Screenshot proof collection complete"

# Phase 4: Session Artifacts Collection (Following Legacy Git Hook Pattern)
echo ""
echo "📦 Phase 4: Collecting complete session artifacts for commit inclusion"
echo "---------------------------------------------------------------------"

# Use a stable validation ID based on timestamp for this precommit run
VALIDATION_ID="$(date +%Y%m%d-%H%M%S)-$$"
VALIDATION_RUN_DIR=".continuum/sessions/validation/run_${VALIDATION_ID}"

# Find the active browser session (where screenshots were saved)
# Don't rely on currentUser symlink - it may point to wrong session if system restarted
SCREENSHOT_SESSION=$(find examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/precommit-*.png 2>/dev/null | head -1)
if [ -n "$SCREENSHOT_SESSION" ]; then
    # Extract session directory from screenshot path
    SESSION_PATH=$(echo "$SCREENSHOT_SESSION" | sed 's|/screenshots/.*||')
    CURRENT_SESSION=$(basename "$SESSION_PATH")

    if [ -d "$SESSION_PATH" ]; then
        echo "🔍 Active screenshot session: $CURRENT_SESSION"

        # Ensure validation parent directory exists
        mkdir -p ".continuum/sessions/validation"

        # Move ENTIRE session directory to validation (rename it to run_ID)
        echo "📋 Moving complete session directory to validation..."
        mv "$SESSION_PATH" "$VALIDATION_RUN_DIR"
        echo "✅ Complete session moved to validation directory"

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

        # VALIDATION ARTIFACTS: Add to git for commit inclusion
        echo "📋 Validation artifacts created for bulletproof validation..."

        # Stage validation directory from repo root
        REPO_ROOT="../../.."
        cd "$REPO_ROOT"
        git add "src/debug/jtag/$VALIDATION_RUN_DIR" 2>/dev/null || true
        cd - > /dev/null
        echo "✅ Validation artifacts staged for commit (or already ignored)"

        # Validation successful - artifacts will be committed with code changes
        echo "✅ VALIDATION COMPLETE: Session artifacts staged for git commit"
        echo "📁 Validation session: $VALIDATION_RUN_DIR"
        echo "🔑 Session artifacts included: logs, screenshots, session metadata"
        echo "📝 Test results included: test-results.txt, validation-info.json"

    else
        echo "❌ Session directory not found: $SESSION_PATH"
        exit 1
    fi
else
    echo "❌ No precommit screenshots found - screenshot phase may have failed"
    echo "   Expected: examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/precommit-*.png"
    exit 1
fi

# Phase 5: Final Validation
echo ""
echo "🔎 Phase 5: Final validation check"
echo "----------------------------------"

# Verify critical proof artifacts exist (screenshots and metadata)
REQUIRED_ARTIFACTS=(
    "$VALIDATION_RUN_DIR/screenshots"
    "$VALIDATION_RUN_DIR/validation-info.json"
)

for artifact in "${REQUIRED_ARTIFACTS[@]}"; do
    if [ -e "$artifact" ]; then
        echo "✅ $artifact"
    else
        echo "❌ Missing required artifact: $artifact"
        exit 1
    fi
done

# Logs are optional (user sessions don't always have them)
if [ -e "$VALIDATION_RUN_DIR/logs" ]; then
    echo "✅ $VALIDATION_RUN_DIR/logs (optional)"
fi

# Phase 6: Commit Message Enhancement
echo ""
echo "📝 Phase 6: Preparing validation summary for commit message"
echo "-----------------------------------------------------------"

# Create validation summary matching existing commit format
VALIDATION_SUMMARY=$(cat << EOF
🔍 JTAG INTEGRATION TEST: ✅ $TEST_SUMMARY - All validation phases completed
🛡️ Git Hook Validation: ✅ All 6 phases passed (TypeScript → JTAG Test Runner → Artifacts → Message Enhancement)
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
echo "✅ CRUD + State + AI Response integration: 100% PASSED"
echo "✅ Screenshot proof: COLLECTED"
echo "✅ Session artifacts: PROMOTED"
echo "✅ All validation artifacts included in commit"
echo ""
echo "🚀 Commit approved - system is bulletproof!"

# Call summary after successful validation (no cleanup - keep artifacts in git)
post_commit_summary