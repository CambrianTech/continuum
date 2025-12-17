#!/bin/bash
set -e  # Exit immediately on any error

# Navigate to the correct working directory
cd "$(dirname "$0")/.."

# ==============================================================================
# LOAD CONFIGURATION
# ==============================================================================
# Source the modular configuration file
if [ -f "scripts/precommit-config.sh" ]; then
    source scripts/precommit-config.sh
    echo "✅ Loaded precommit configuration from scripts/precommit-config.sh"
else
    echo "❌ Configuration file not found: scripts/precommit-config.sh"
    echo "   Using default settings"
    export ENABLE_TYPESCRIPT_CHECK=true
    export ENABLE_BROWSER_TEST=true
    export RESTART_STRATEGY="on_code_change"
    export PRECOMMIT_TESTS="tests/precommit/browser-ping.test.ts"
fi

echo "🔒 GIT PRECOMMIT: Modular validation (config-driven)"
echo "=================================================="
echo "📋 Active phases:"
[ "$ENABLE_TYPESCRIPT_CHECK" = true ] && echo "  ✅ TypeScript compilation"
[ "$ENABLE_SYSTEM_RESTART" = true ] && echo "  ✅ System restart (strategy: $RESTART_STRATEGY)"
[ "$ENABLE_BROWSER_TEST" = true ] && echo "  ✅ Browser tests ($PRECOMMIT_TESTS)"
echo ""

# Phase 1: Foundation Validation
if [ "$ENABLE_TYPESCRIPT_CHECK" = true ]; then
    echo ""
    echo "📋 Phase 1: TypeScript Compilation"
    echo "-------------------------------------"

    echo "🔨 Running TypeScript compilation..."
    npm run build:ts
    # Restore version.ts to avoid timestamp-only changes in commit
    cd ../../..
    git restore src/debug/jtag/shared/version.ts 2>/dev/null || true
    cd src/debug/jtag
    echo "✅ TypeScript compilation passed"
else
    echo "⏭️  Phase 1: TypeScript compilation SKIPPED (disabled in config)"
fi

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

# Determine if restart is needed based on strategy
if [ "$ENABLE_SYSTEM_RESTART" = true ]; then
    echo "🏓 Checking if system restart is required (strategy: $RESTART_STRATEGY)..."
    NEED_RESTART=false

    case "$RESTART_STRATEGY" in
        always)
            echo "📝 Always restart (strategy: always)"
            NEED_RESTART=true
            ;;
        on_code_change)
            if [ "$CODE_CHANGED" = true ]; then
                echo "📝 Code changed - restart required to test new code"
                NEED_RESTART=true
            elif ! ./jtag ping >/dev/null 2>&1; then
                echo "❌ System not responding to ping - restart required"
                NEED_RESTART=true
            else
                echo "✅ System running and no code changes - no restart needed"
            fi
            ;;
        on_ping_fail)
            if ! ./jtag ping >/dev/null 2>&1; then
                echo "❌ System not responding to ping - restart required"
                NEED_RESTART=true
            else
                echo "✅ System responding to ping - no restart needed"
            fi
            ;;
        never)
            echo "⏭️  Restart disabled (strategy: never)"
            NEED_RESTART=false
            ;;
        *)
            echo "⚠️  Unknown restart strategy: $RESTART_STRATEGY (defaulting to on_code_change)"
            NEED_RESTART=$CODE_CHANGED
            ;;
    esac
else
    echo "⏭️  System restart SKIPPED (disabled in config)"
    NEED_RESTART=false
fi

# Start system if ping failed
if [ "$NEED_RESTART" = true ]; then
    echo "🚀 Starting deployment..."
    npm start &
    DEPLOY_PID=$!

    echo "⏳ Waiting for system to be ready..."
    TIMEOUT=90  # Generous timeout for initial startup
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
else
    echo "⚡ System already running - no restart needed"
fi

# Phase 2: Browser Tests
if [ "$ENABLE_BROWSER_TEST" = true ]; then
    echo ""
    echo "🧪 Phase 2: Browser Tests"
    echo "-----------------------------------------------------------"

    echo "🧪 Running precommit tests: $PRECOMMIT_TESTS"

    # Ensure test output directory exists
    mkdir -p .continuum/sessions/validation

    # Run all configured tests
    TEST_EXIT_CODE=0
    TEST_SUMMARY=""

    for TEST_FILE in $PRECOMMIT_TESTS; do
        echo "=================================================="
        echo "🧪 Running: $TEST_FILE"
        echo "=================================================="

        npx tsx "$TEST_FILE" 2>&1 | tee .continuum/sessions/validation/test-output.txt
        CURRENT_EXIT_CODE=${PIPESTATUS[0]}

        if [ $CURRENT_EXIT_CODE -ne 0 ]; then
            TEST_EXIT_CODE=$CURRENT_EXIT_CODE
            echo ""
            echo "❌ TEST FAILED - BLOCKING COMMIT"
            echo "=================================================="
            echo "❌ Test FAILED (exit code: $CURRENT_EXIT_CODE)"
            echo "   Test file: $TEST_FILE"
            echo "   Output shown above"
            echo ""
            echo "🔍 Fix the failing test before committing"
            echo "=================================================="
            exit 1
        else
            echo "✅ Test passed: $TEST_FILE"
            TEST_SUMMARY="$TEST_SUMMARY $TEST_FILE:PASSED"
        fi
    done

    echo ""
    echo "✅ All precommit tests: PASSED"
    echo "📊 Test results: $TEST_SUMMARY"
else
    echo "⏭️  Phase 2: Browser tests SKIPPED (disabled in config)"
    TEST_SUMMARY="Browser tests: SKIPPED"
fi

# Phase 3: Session Artifacts Collection
if [ "$ENABLE_ARTIFACTS_COLLECTION" = true ]; then
    echo ""
    echo "📦 Phase 3: Collecting session artifacts"
    echo "---------------------------------------------------------------------"

# Use a stable validation ID based on timestamp for this precommit run
VALIDATION_ID="$(date +%Y%m%d-%H%M%S)-$$"
VALIDATION_RUN_DIR=".continuum/sessions/validation/run_${VALIDATION_ID}"

# Find the active browser session (where screenshots were saved by integration tests)
# Don't rely on currentUser symlink - it may point to wrong session if system restarted
SCREENSHOT_SESSION=$(find examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/*.png 2>/dev/null | head -1)
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
    echo "❌ No screenshots found from integration tests"
    echo "   Expected: examples/widget-ui/.continuum/jtag/sessions/user/*/screenshots/*.png"
    echo "   This means the integration test didn't capture screenshots properly"
    exit 1
fi
else
    echo "⏭️  Phase 3: Artifacts collection SKIPPED (disabled in config)"
fi

# Final Summary
echo ""
echo "🎉 PRECOMMIT VALIDATION COMPLETE!"
echo "=================================================="
[ "$ENABLE_TYPESCRIPT_CHECK" = true ] && echo "✅ TypeScript compilation: PASSED"
[ "$ENABLE_SYSTEM_RESTART" = true ] && echo "✅ System restart: COMPLETED (strategy: $RESTART_STRATEGY)"
[ "$ENABLE_BROWSER_TEST" = true ] && echo "✅ Browser tests: PASSED"
echo ""
echo "🚀 Commit approved - all enabled validations passed!"