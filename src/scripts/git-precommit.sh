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
    cd ..
    git restore src/shared/version.ts 2>/dev/null || true
    cd src
    echo "✅ TypeScript compilation passed"
else
    echo "⏭️  Phase 1: TypeScript compilation SKIPPED (disabled in config)"
fi

# ============================================================================
# Phase 1.5: Strict Lint (MODIFIED FILES ONLY)
# ============================================================================
# This enforces strict rules on NEW code without breaking existing tech debt.
# Only staged files are checked - incrementally improve quality.
# ============================================================================
echo ""
echo "📋 Phase 1.5: Strict Lint (modified files only)"
echo "-------------------------------------"

# Get list of staged TypeScript files (excluding node_modules, dist, generated)
TS_FILES=$(cd .. && git diff --cached --name-only --diff-filter=ACMR | grep -E 'src/.*\.tsx?$' | grep -v 'node_modules' | grep -v 'dist/' | grep -v '/generated' | grep -v 'generated-command' || true)

# Get list of staged Rust files
RS_FILES=$(cd .. && git diff --cached --name-only --diff-filter=ACMR | grep -E 'src/workers/.*\.rs$' | grep -v 'target/' || true)

LINT_FAILED=false

if [ -n "$TS_FILES" ]; then
    echo "TypeScript files to lint:"
    echo "$TS_FILES" | sed 's/^/  • /' | head -10
    TS_COUNT=$(echo "$TS_FILES" | wc -l | tr -d ' ')
    [ "$TS_COUNT" -gt 10 ] && echo "  ... and $((TS_COUNT - 10)) more"
    echo ""

    # Run ESLint on modified files only (paths relative to jtag dir)
    LINT_OUTPUT=$(cd .. && echo "$TS_FILES" | xargs npx eslint --max-warnings 0 2>&1) || {
        echo ""
        echo "╔════════════════════════════════════════════════════════════════╗"
        echo "║  ❌ TYPESCRIPT LINT FAILED - BLOCKING COMMIT                   ║"
        echo "╠════════════════════════════════════════════════════════════════╣"
        echo "║  Common violations:                                            ║"
        echo "║  • Using 'any'          → Use specific types                   ║"
        echo "║  • Using ||             → Use ?? (nullish coalescing)          ║"
        echo "║  • Missing return type  → Add explicit return type             ║"
        echo "║  • Unused variables     → Remove or prefix with _              ║"
        echo "╚════════════════════════════════════════════════════════════════╝"
        echo ""
        echo "$LINT_OUTPUT"
        LINT_FAILED=true
    }
    [ "$LINT_FAILED" = false ] && echo "✅ TypeScript lint: PASSED"
else
    echo "⏭️  No TypeScript files staged - skipping ESLint"
fi

if [ -n "$RS_FILES" ]; then
    echo ""
    echo "Rust files to lint with clippy:"
    echo "$RS_FILES" | sed 's/^/  • /' | head -10
    echo ""

    # Run clippy on the workspace (warnings as errors)
    if ! (cd workers/continuum-core && cargo clippy --quiet -- -D warnings 2>&1); then
        echo ""
        echo "╔════════════════════════════════════════════════════════════════╗"
        echo "║  ❌ RUST CLIPPY FAILED - BLOCKING COMMIT                       ║"
        echo "╠════════════════════════════════════════════════════════════════╣"
        echo "║  Common violations:                                            ║"
        echo "║  • Dead code           → Remove unused functions/vars          ║"
        echo "║  • Unused imports      → Remove unused 'use' statements        ║"
        echo "║  • Unnecessary clone   → Remove or explain why needed          ║"
        echo "╚════════════════════════════════════════════════════════════════╝"
        LINT_FAILED=true
    else
        echo "✅ Rust clippy: PASSED"
    fi
else
    echo "⏭️  No Rust files staged - skipping clippy"
fi

if [ "$LINT_FAILED" = true ]; then
    echo ""
    echo "❌ STRICT LINT FAILED - Fix violations in modified files before committing"
    exit 1
fi
echo ""

# Detect if code changes require deployment
echo "🔍 Checking if code changes require deployment..."
cd ..
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

cd src

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
        REPO_ROOT=".."
        cd "$REPO_ROOT"
        git add "src/$VALIDATION_RUN_DIR" 2>/dev/null || true
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

# Phase 4: Cleanup artifacts from test run
echo ""
echo "🧹 Phase 4: Cleaning up test artifacts"
echo "-----------------------------------------------------------"

# Restore files that get auto-generated during npm start
cd ..
echo "🔄 Restoring auto-generated files to avoid commit noise..."
git restore src/package.json 2>/dev/null || true
git restore src/package-lock.json 2>/dev/null || true
git restore src/generated-command-schemas.json 2>/dev/null || true
git restore src/shared/version.ts 2>/dev/null || true
git restore src/.continuum/sessions/validation/test-output.txt 2>/dev/null || true
cd src
echo "✅ Test artifacts cleaned up"

# Final Summary
echo ""
echo "🎉 PRECOMMIT VALIDATION COMPLETE!"
echo "=================================================="
[ "$ENABLE_TYPESCRIPT_CHECK" = true ] && echo "✅ TypeScript compilation: PASSED"
[ "$ENABLE_SYSTEM_RESTART" = true ] && echo "✅ System restart: COMPLETED (strategy: $RESTART_STRATEGY)"
[ "$ENABLE_BROWSER_TEST" = true ] && echo "✅ Browser tests: PASSED"
echo "✅ Test artifacts cleaned up"
echo ""
echo "🚀 Commit approved - all enabled validations passed!"