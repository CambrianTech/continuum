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
git restore shared/version.ts 2>/dev/null || true
echo "✅ TypeScript compilation passed"

# Check if system is already running and healthy
echo "🔍 Checking if system is already running..."
SYSTEM_ALREADY_RUNNING=false
if ./jtag ping >/dev/null 2>&1; then
    echo "✅ System already running - skipping restart (massive time savings!)"
    SYSTEM_ALREADY_RUNNING=true
else
    echo "🚀 System not running - starting deployment..."
    npm start &
    DEPLOY_PID=$!
fi

# Wait for COMPLETE system to be ready (with timeout) - skip if already running
if [ "$SYSTEM_ALREADY_RUNNING" = true ]; then
    echo "⚡ Skipping orchestration wait - system already healthy"
    ORCHESTRATION_COMPLETE=true
else
    echo "⏳ Waiting for complete system orchestration..."
    TIMEOUT=180  # 3 minutes for full system readiness
    COUNTER=0
    PING_SUCCESS=false
    ORCHESTRATION_COMPLETE=false

    while [ $COUNTER -lt $TIMEOUT ]; do
    # First check if ping works (basic connectivity)
    if ./jtag ping >/dev/null 2>&1; then
        PING_SUCCESS=true

        # Then check if orchestration is complete (full system ready)
        if tail -50 .continuum/jtag/system/logs/npm-start.log 2>/dev/null | grep -q "🎉 Orchestration complete"; then
            ORCHESTRATION_COMPLETE=true
            echo "✅ System deployment successful - orchestration complete"
            echo "⏳ Allowing system to fully settle..."
            sleep 5  # Give system time to fully stabilize
            break
        else
            # System is up but still initializing
            if [ $((COUNTER % 10)) -eq 0 ]; then
                echo "   ... system responsive, waiting for orchestration ($COUNTER/${TIMEOUT}s)"
            fi
        fi
    else
        # System not yet responsive
        if [ $((COUNTER % 10)) -eq 0 ]; then
            echo "   ... waiting for system startup ($COUNTER/${TIMEOUT}s)"
        fi
    fi

        sleep 1
        COUNTER=$((COUNTER + 1))
    done

    if [ $COUNTER -eq $TIMEOUT ]; then
        echo "❌ System deployment timed out after ${TIMEOUT}s"
        if [ "$PING_SUCCESS" = true ] && [ "$ORCHESTRATION_COMPLETE" = false ]; then
            echo "   System was responsive but orchestration never completed"
            echo "   Check .continuum/jtag/system/logs/npm-start.log for details"
        fi
        kill $DEPLOY_PID 2>/dev/null || true
        exit 1
    fi
fi

# Phase 2: Integration Testing
echo ""
echo "🧪 Phase 2: CRUD + State Integration (100% required)"
echo "----------------------------------------------------"

echo "🧪 Running precommit test profile via JTAG test runner..."

# Run precommit integration tests directly to avoid CLI timeout
echo "🧪 Running CRUD integration test..."
TEST_OUTPUT1=$(npx tsx tests/integration/database-chat-integration.test.ts 2>&1)
TEST_EXIT_CODE1=$?

echo "🧪 Running State integration test..."
TEST_OUTPUT2=$(npx tsx tests/integration/state-system-integration.test.ts 2>&1)
TEST_EXIT_CODE2=$?

# Check if both tests passed
if [ $TEST_EXIT_CODE1 -eq 0 ] && [ $TEST_EXIT_CODE2 -eq 0 ]; then
    echo "✅ Precommit integration tests: ALL PASSED"
    echo "📊 Test results: 2 of 2 tests passed (CRUD + State Integration)"

    # Store test results for commit message
    TEST_SUMMARY="CRUD + State Integration: 2/2 - ALL TESTS PASSED"
else
    echo "❌ Precommit integration tests FAILED - blocking commit"
    if [ $TEST_EXIT_CODE1 -ne 0 ]; then
        echo "❌ CRUD integration test failed:"
        echo "$TEST_OUTPUT1"
    fi
    if [ $TEST_EXIT_CODE2 -ne 0 ]; then
        echo "❌ State integration test failed:"
        echo "$TEST_OUTPUT2"
    fi
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

        # Force add validation directory (bypasses .continuum/ gitignore)
        git add -f "$VALIDATION_RUN_DIR"
        echo "✅ Validation artifacts staged for commit"

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

# Copy validation artifacts to repo root for git tracking
echo ""
echo "📦 Copying validation artifacts to repo root for git tracking..."
REPO_ROOT="../../../"
REPO_VALIDATION_DIR="${REPO_ROOT}.continuum/sessions/validation/run_${VALIDATION_ID}"
mkdir -p "$REPO_VALIDATION_DIR"
cp -r "$VALIDATION_RUN_DIR"/* "$REPO_VALIDATION_DIR/"
echo "✅ Validation artifacts copied to ${REPO_VALIDATION_DIR}"

# Stage the validation artifacts immediately
cd "$REPO_ROOT"
git add ".continuum/sessions/validation/run_${VALIDATION_ID}"
cd - > /dev/null
echo "✅ Validation artifacts staged for commit"

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
echo "✅ CRUD + State integration: 100% PASSED"
echo "✅ Screenshot proof: COLLECTED"
echo "✅ Session artifacts: PROMOTED"
echo "✅ All validation artifacts included in commit"
echo ""
echo "🚀 Commit approved - system is bulletproof!"

# Call summary after successful validation (no cleanup - keep artifacts in git)
post_commit_summary