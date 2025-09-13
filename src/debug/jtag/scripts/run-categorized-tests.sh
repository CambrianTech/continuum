#!/bin/bash

# Modular Categorized Test Runner
# Can be used by any test command to get consistent categorized output
# Usage: ./scripts/run-categorized-tests.sh [profile] [additional_args...]
#
# Profiles: 
#   - comprehensive: All tests (default)
#   - integration: Integration tests only
#   - unit: Unit tests only  
#   - chat: Chat-related tests only
#   - screenshots: Screenshot tests only
#   - themes: Theme system tests only
#   - transport: Transport tests only
#   - events: Event system tests only
#   - blocker: Blocker-level tests only
#   - critical: Critical tests only
#   - performance: Grid P2P performance tests with microsecond precision
#
# Browser Deployment Options:
#   - Set JTAG_DEPLOY_BROWSER=true to ensure browser deployment before tests
#   - Set JTAG_DEPLOY_BROWSER=false to skip deployment (assumes system running)
#   - Default: true for comprehensive, transport, screenshots; false for others

PROFILE="${1:-comprehensive}"
TEST_FILE="$2"
shift  # Remove profile from args, pass rest to test functions

# Determine if browser deployment is needed
DEPLOY_BROWSER="${JTAG_DEPLOY_BROWSER:-auto}"

# Auto-detect deployment needs based on profile
if [ "$DEPLOY_BROWSER" = "auto" ]; then
    case "$PROFILE" in
        "comprehensive"|"transport"|"screenshots"|"themes"|"critical"|"widgets"|"database")
            DEPLOY_BROWSER="true"
            ;;
        *)
            DEPLOY_BROWSER="false"
            ;;
    esac
fi

# Remove set -e so we can continue after failures and aggregate results
SIMPLE_MODE=false

echo "🚀 CATEGORIZED TEST SUITE - Profile: $PROFILE"
echo "═══════════════════════════════════════════════════════════════════════════════"

# Handle build requirements if needed
if [ "$JTAG_FORCE_BUILD" = "true" ]; then
    echo "🔨 FORCED BUILD: Source code changes detected - rebuilding first..."
    echo "📋 Running: npm run smart-build (intelligent incremental build)"
    
    if npm run smart-build; then
        echo "✅ Smart build completed successfully"
    else
        echo "❌ FATAL: Smart build failed"
        echo "🔍 Check build logs for TypeScript/generation errors"
        exit 1
    fi
    echo ""
fi

# Handle browser deployment if needed  
if [ "$DEPLOY_BROWSER" = "true" ]; then
    echo "🌐 BROWSER DEPLOYMENT: Required for profile '$PROFILE'"
    echo "🔍 Checking if system is ready..."
    
    # Check if system is already running and has browser deployed
    if [ "$JTAG_FORCE_BROWSER_LAUNCH" = "true" ]; then
        echo "🔄 FORCE BROWSER LAUNCH: Restarting system to guarantee fresh browser"
        npm run system:stop > /dev/null 2>&1 || true
        echo "🚀 Starting system with forced fresh browser deployment..."
        echo "📋 Running: npm run system:start (launches browser + waits for ready)"
        
        if npm run system:start; then
            echo "✅ Forced browser deployment successful - proceeding with tests"
        else
            echo "❌ FATAL: Forced browser deployment failed"
            echo "🔍 Check system logs: .continuum/jtag/system/logs/npm-start.log"
            echo "🛠️  Try: npm run system:stop && npm run system:start"
            exit 1
        fi
    else
        # Use dynamic port checking 
        ACTIVE_PORTS=$(node -e "const { getActivePortsSync } = require('./system/shared/ExampleConfig'); const ports = getActivePortsSync(); console.log(\`\${ports.websocket_server} \${ports.http_server}\`);" 2>/dev/null || echo "9002 9003")
        WS_PORT=$(echo $ACTIVE_PORTS | cut -d' ' -f1)
        HTTP_PORT=$(echo $ACTIVE_PORTS | cut -d' ' -f2)
        
        if lsof -ti:$WS_PORT >/dev/null 2>&1 && lsof -ti:$HTTP_PORT >/dev/null 2>&1; then
            echo "✅ System already running and ready (verified by port check: WS=$WS_PORT, HTTP=$HTTP_PORT)"
        else
            echo "🚀 Starting system with browser deployment..."
            echo "📋 Running: npm run system:start (launches browser + waits for ready)"
            
            if npm run system:start; then
                echo "✅ Browser deployment successful - proceeding with tests"
            else
                echo "❌ FATAL: Browser deployment failed"
                echo "🔍 Check system logs: .continuum/jtag/system/logs/npm-start.log"
                echo "🛠️  Try: npm run system:stop && npm run system:start"
                exit 1
            fi
        fi
    fi
    echo ""
elif [ "$DEPLOY_BROWSER" = "false" ]; then
    echo "🔄 BROWSER DEPLOYMENT: Skipped - assuming system already running"
    echo "⚠️  If tests fail, run: JTAG_DEPLOY_BROWSER=true ./scripts/run-categorized-tests.sh $PROFILE"
    echo ""
fi

# Initialize result tracking
declare -i TOTAL_TESTS=0
declare -i PASSED_TESTS=0
declare -i FAILED_TESTS=0
declare -a FAILED_TEST_NAMES=()

# Initialize temp file for category tracking using .continuum pattern
mkdir -p .continuum/tests
> .continuum/tests/test_results.tmp

# Function to run test and track results by category
run_test() {
    local test_name="$1"
    local test_command="$2"  
    local category="$3"
    
    echo "▶️  Running: $test_name [$category]"
    ((TOTAL_TESTS++))
    
    # Handle output based on verbose mode
    local temp_output="/tmp/test_output_$$_$(date +%s).log"
    local test_result
    
    if [ "$JTAG_TEST_VERBOSE" = "true" ]; then
        # Verbose mode: show output in real-time and capture to file
        if eval "$test_command" 2>&1 | tee "$temp_output"; then
            test_result="pass"
        else
            test_result="fail"
        fi
    else
        # Quiet mode: capture output to file only
        if eval "$test_command" >"$temp_output" 2>&1; then
            test_result="pass"
        else
            test_result="fail"
        fi
    fi
    
    if [ "$test_result" = "pass" ]; then
        echo "✅ PASSED: $test_name"
        ((PASSED_TESTS++))
        echo "$category|PASS|$test_name|" >> .continuum/tests/test_results.tmp
    else
        echo "❌ FAILED: $test_name"
        echo "💥 ERROR DETAILS:"
        # Show last 10 lines of error output for debugging
        local error_details=""
        if [ -f "$temp_output" ]; then
            error_details=$(tail -10 "$temp_output" | sed 's/^/   /')
            echo "$error_details"
            echo ""
        fi
        ((FAILED_TESTS++))
        FAILED_TEST_NAMES+=("$test_name")
        # Store error details in results file for summary
        echo "$category|FAIL|$test_name|$error_details" >> .continuum/tests/test_results.tmp
    fi
    # Cleanup temp file
    rm -f "$temp_output"
    echo ""
}

# Profile-specific test selection
run_profile_tests() {
    case "$PROFILE" in
        "comprehensive")
            # Compiler checks first
            run_test "TypeScript Compilation" "npx tsc --noEmit --project ." "Compiler & Build"
            run_test "Import Resolution" "npx tsx tests/compiler-error-detection.test.ts" "Compiler & Build"
            
            # Core system tests
            run_test "Bootstrap Detection" "npx tsx tests/bootstrap-comprehensive.test.ts" "Core System"  
            run_test "System Signals" "npx tsx tests/signal-system.test.ts" "Core System"
            run_test "SystemReadySignaler Integration" "npx tsx tests/system-ready-signaler-integration.test.ts" "Core System"
            run_test "Router Coordination" "npx tsx tests/integration/router-coordination-simple.test.ts" "Core System"
            
            # Browser integration (no widget tests in comprehensive to avoid navigation issues)
            run_test "WebSocket Connection" "npx tsx tests/integration/browser-automated-tests.test.ts" "Browser Integration"
            run_test "Browser Automation" "npx tsx tests/layer-6-browser-integration/minimal-pure-jtag.test.ts" "Browser Integration"
            run_test "CLI to Browser Integration" "npx tsx tests/integration/cli-to-browser-integration.test.ts" "Browser Integration"
            
            # Chat & messaging
            run_test "Chat Message Send" "npx tsx tests/chat-daemon-integration.test.ts" "Chat & Messaging"
            run_test "Chat Widget Simple" "npx tsx tests/chat-widget-simple.test.ts" "Chat & Messaging"
            run_test "Chat TDD" "npx tsx tests/chat-daemon-tdd.test.ts" "Chat & Messaging"
            run_test "Multi-user Chat" "npx tsx tests/integration/simple-multiuser-chat.test.ts" "Chat & Messaging"
            run_test "Real Chat Functionality" "npx tsx tests/integration/chat-scenarios/real-chat-functionality.test.ts" "Chat & Messaging"
            
            # AI & persona integration
            run_test "AI Persona Integration" "npx tsx tests/integration/ai-persona-integration.test.ts" "AI & Personas"
            
            # Unit tests
            run_test "Transport Architecture Unit" "npx tsx tests/transport-architecture-unit.test.ts" "Unit Tests"
            run_test "Event Routing Unit" "npx tsx tests/unit/router-broadcast.test.ts" "Unit Tests"
            run_test "Router Broadcast Unit" "npx tsx tests/unit/room-scoped-event-routing.test.ts" "Unit Tests"
            run_test "Events Daemon Unit" "npx tsx tests/unit/events-daemon-unit.test.ts" "Unit Tests"
            
            # Screenshots & visual
            run_test "Screenshot Capture" "npx tsx tests/server-screenshot.test.ts" "Screenshots & Visual"
            run_test "Screenshot Verification" "npx tsx tests/screenshot-verification.test.ts" "Screenshots & Visual"
            run_test "Screenshot Advanced" "npx tsx tests/screenshot-integration-advanced.test.ts" "Screenshots & Visual"
            run_test "Screenshot Transport" "npx tsx tests/screenshot-transport-test.ts" "Screenshots & Visual"
            run_test "Automated Theme Screenshots" "npx tsx tests/integration/automated-theme-screenshot.test.ts" "Screenshots & Visual"
            run_test "Coordinate Calculation Unit" "npx tsx commands/screenshot/test/unit/CoordinateCalculation.test.ts" "Screenshots & Visual"
            run_test "Coordinate Validation" "npx tsx commands/screenshot/test/validation/SimpleCoordinateValidator.ts" "Screenshots & Visual"
            
            # Transport layer - these tests require full browser deployment via npm test
            # run_test "WebSocket Transport" "npx tsx tests/layer-3-transport/browser-websocket.test.ts" "Transport Tests"  # File doesn't exist
            run_test "Cross-Context Commands" "npx tsx tests/integration/transport/browser-server-commands.test.ts" "Transport Tests"  
            # run_test "Transport Flexibility" "npx tsx tests/integration/transport/transport-flexibility.test.ts" "Transport Tests"  # Still may hang
            
            # Grid P2P Performance Tests - Advanced testing with microsecond precision  
            # TEMPORARILY DISABLED: Grid Transport Foundation test hangs on UDP multicast discovery
            # TODO: Fix UDP multicast transport timeouts (Phase 3 - Distributed Systems)
            # run_test "Grid Transport Foundation" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-transport-foundation.test.ts" "Grid Performance"
            run_test "Grid Routing Backbone" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-routing-backbone.test.ts" "Grid Performance"
            run_test "Grid Distributed Chat Commands" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-distributed-chat-commands.test.ts" "Grid Performance"
            run_test "Grid Events All Layers" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-events-all-layers.test.ts" "Grid Performance"
            run_test "Grid Advanced Performance Analysis" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-advanced-performance-analysis.test.ts" "Grid Performance"
            
            # Database & persistence tests
            run_test "Database Persistence Validation" "npx tsx tests/integration/database/database-persistence-validation.test.ts" "Database Tests"
            run_test "Database Comprehensive Integration" "npx tsx tests/integration/database-comprehensive-integration.test.ts" "Database Tests"
            run_test "Data Daemon Unit Tests" "npx tsx tests/data-daemon/run-data-tests.ts" "Database Tests"
            run_test "Professional Data Architecture" "npx tsx tests/classified/ProfessionalDataArchitectureTest.ts" "Database Tests"
            run_test "Storage Configuration Integration" "npx tsx daemons/data-daemon/test/integration/StorageConfigurationIntegration.test.ts" "Database Tests"
            
            # Event system
            run_test "Server-Browser Events" "npx tsx tests/integration/server-browser-event-flow.test.ts" "Event Tests"
            run_test "Browser-Server Events" "npx tsx tests/integration/browser-server-event-flow.test.ts" "Event Tests" 
            run_test "Cross-Environment Events" "npx tsx tests/integration/cross-environment-events-working.test.ts" "Event Tests"
            ;;
            
        "integration")
            run_test "Transport Architecture Integration" "npx tsx tests/integration/transport-architecture-integration.test.ts" "Integration Tests"
            run_test "Browser Integration" "npx tsx tests/integration/browser-automated-tests.test.ts" "Integration Tests"
            run_test "CLI to Browser Integration" "npx tsx tests/integration/cli-to-browser-integration.test.ts" "Integration Tests"
            run_test "Server-Client Integration" "npx tsx tests/integration/server-client-integration.test.ts" "Integration Tests"
            run_test "Router Coordination" "npx tsx tests/integration/router-coordination-simple.test.ts" "Integration Tests"
            run_test "Cross-Context Commands" "npx tsx tests/integration/transport/browser-server-commands.test.ts" "Integration Tests"
            ;;
            
        "unit")
            run_test "Transport Architecture" "npx tsx tests/transport-architecture-unit.test.ts" "Unit Tests"
            run_test "Event Routing" "npx tsx tests/unit/router-broadcast.test.ts" "Unit Tests"
            run_test "Room Scoped Events" "npx tsx tests/unit/room-scoped-event-routing.test.ts" "Unit Tests"
            run_test "Events Daemon" "npx tsx tests/unit/events-daemon-unit.test.ts" "Unit Tests"
            run_test "Coordinate Calculation Unit" "npx tsx commands/screenshot/test/unit/CoordinateCalculation.test.ts" "Unit Tests"
            ;;
            
        "chat")
            run_test "Chat Daemon Integration" "npx tsx tests/chat-daemon-integration.test.ts" "Chat Tests"
            run_test "Chat Widget Simple" "npx tsx tests/chat-widget-simple.test.ts" "Chat Tests"
            run_test "Chat TDD" "npx tsx tests/chat-daemon-tdd.test.ts" "Chat Tests"
            run_test "Multi-user Chat" "npx tsx tests/integration/simple-multiuser-chat.test.ts" "Chat Tests"
            run_test "Real Chat Functionality" "npx tsx tests/integration/chat-scenarios/real-chat-functionality.test.ts" "Chat Tests"
            ;;
            
        "screenshots")
            run_test "Server Screenshot" "npx tsx tests/server-screenshot.test.ts" "Screenshot Tests"
            run_test "Screenshot Verification" "npx tsx tests/screenshot-verification.test.ts" "Screenshot Tests"
            run_test "Screenshot Advanced" "npx tsx tests/screenshot-integration-advanced.test.ts" "Screenshot Tests"
            run_test "Automated Theme Screenshots" "npx tsx tests/integration/automated-theme-screenshot.test.ts" "Screenshot Tests"
            run_test "Coordinate Calculation Unit" "npx tsx commands/screenshot/test/unit/CoordinateCalculation.test.ts" "Screenshot Tests"
            run_test "Coordinate Validation" "npx tsx commands/screenshot/test/validation/SimpleCoordinateValidator.ts" "Screenshot Tests"
            ;;
            
        "themes")
            run_test "Automated Theme Screenshots" "npx tsx tests/integration/automated-theme-screenshot.test.ts" "Theme Tests"
            ;;
            
        "transport")
            run_test "Transport Architecture Unit" "npx tsx tests/transport-architecture-unit.test.ts" "Transport Tests"
            run_test "Transport Architecture Integration" "npx tsx tests/integration/transport-architecture-integration.test.ts" "Transport Tests"
            run_test "Transport Diagnostic" "npx tsx tests/transport-diagnostic.test.ts" "Transport Tests"
            run_test "Cross-Context Commands" "npx tsx tests/integration/transport/browser-server-commands.test.ts" "Transport Tests"
            run_test "Transport Reliability Validation" "npx tsx tests/integration/transport/transport-reliability-validation.test.ts" "Transport Tests"
            # run_test "Transport Flexibility" "npx tsx tests/integration/transport/transport-flexibility.test.ts" "Transport Tests"  # Still may hang
            
            # Grid P2P Performance Tests
            run_test "Grid Transport Foundation" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-transport-foundation.test.ts" "Grid Performance"
            run_test "Grid Routing Backbone" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-routing-backbone.test.ts" "Grid Performance"
            run_test "Grid Distributed Chat Commands" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-distributed-chat-commands.test.ts" "Grid Performance"
            run_test "Grid Events All Layers" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-events-all-layers.test.ts" "Grid Performance"
            run_test "Grid Advanced Performance Analysis" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-advanced-performance-analysis.test.ts" "Grid Performance"
            ;;
            
        "events")
            run_test "Server-Browser Events" "npx tsx tests/integration/server-browser-event-flow.test.ts" "Event Tests"
            run_test "Browser-Server Events" "npx tsx tests/integration/browser-server-event-flow.test.ts" "Event Tests"
            run_test "Cross-Environment Events" "npx tsx tests/integration/cross-environment-events-working.test.ts" "Event Tests"
            ;;
            
        "blocker")
            run_test "System Bootstrap" "npx tsx tests/bootstrap-comprehensive.test.ts" "Blocker Tests"
            run_test "TypeScript Compilation" "npx tsc --noEmit --project ." "Blocker Tests"
            ;;
            
        "critical") 
            run_test "Browser Integration" "npx tsx tests/integration/browser-automated-tests.test.ts" "Critical Tests"
            run_test "Router Coordination" "npx tsx tests/integration/router-coordination-simple.test.ts" "Critical Tests"
            run_test "Screenshot System" "npx tsx tests/server-screenshot.test.ts" "Critical Tests"
            ;;
            
        "widgets")
            run_test "Clean Widget Test" "npx tsx tests/layer-6-browser-integration/clean-widget-test.ts" "Widget Tests"
            run_test "Widget Foundation" "npx tsx tests/layer-6-browser-integration/simplified-widget-demo.test.ts" "Widget Tests"
            ;;
            
        "database")
            run_test "Database Persistence Validation" "npx tsx tests/integration/database/database-persistence-validation.test.ts" "Database Tests"
            run_test "Database Comprehensive Integration" "npx tsx tests/integration/database-comprehensive-integration.test.ts" "Database Tests"
            run_test "Data Daemon Unit Tests" "npx tsx tests/data-daemon/run-data-tests.ts" "Database Tests"
            run_test "Professional Data Architecture" "npx tsx tests/classified/ProfessionalDataArchitectureTest.ts" "Database Tests"
            run_test "Storage Configuration Integration" "npx tsx daemons/data-daemon/test/integration/StorageConfigurationIntegration.test.ts" "Database Tests"
            ;;
            
        "ai")
            run_test "AI Persona Integration" "npx tsx tests/integration/ai-persona-integration.test.ts" "AI Tests"
            ;;
            
        "performance")
            # Grid P2P Performance Testing Suite - Microsecond precision measurements
            run_test "Grid Transport Foundation" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-transport-foundation.test.ts" "Grid Performance"
            run_test "Grid Routing Backbone" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/grid-routing-backbone.test.ts" "Grid Performance"  
            run_test "Grid Distributed Chat Commands" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-distributed-chat-commands.test.ts" "Grid Performance"
            run_test "Grid Events All Layers" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-events-all-layers.test.ts" "Grid Performance"
            run_test "Grid Advanced Performance Analysis" "JTAG_WORKING_DIR='examples/test-bench' npx tsx tests/integration/grid-advanced-performance-analysis.test.ts" "Grid Performance"
            ;;
            
        "single-test")
            # Single test runner - pass test file as second argument
            if [ -n "$TEST_FILE" ]; then
                run_test "Single Test" "npx tsx $TEST_FILE" "Single Test"
            else
                echo "❌ Usage: ./scripts/run-categorized-tests.sh single-test path/to/test.ts"
                exit 1
            fi
            ;;
            
        *)
            echo "❌ Unknown profile: $PROFILE"
            echo "Available profiles: comprehensive, integration, unit, chat, screenshots, themes, transport, events, blocker, critical, widgets, database, ai, performance, single-test"
            exit 1
            ;;
    esac
}

# Run the selected profile tests
run_profile_tests "$@"

# COMPREHENSIVE SUMMARY WITH CATEGORY BREAKDOWN
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🎯 TEST RESULTS - Profile: $PROFILE"
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "📊 Overall Summary:"
echo "   Total Tests: $TOTAL_TESTS"
echo "   ✅ Passed: $PASSED_TESTS"
echo "   ❌ Failed: $FAILED_TESTS"

if [ $TOTAL_TESTS -gt 0 ]; then
    echo "   📈 Success Rate: $(( PASSED_TESTS * 100 / TOTAL_TESTS ))%"
else
    echo "   📈 Success Rate: N/A (no tests run)"
fi
echo ""

# Show category breakdown first
echo "📋 Results by Category:"
if [ -f .continuum/tests/test_results.tmp ]; then
    # Get unique categories from results file - use while loop to handle spaces in names
    cut -d'|' -f1 .continuum/tests/test_results.tmp | sort -u | while read -r category; do
        total=$(grep "^$category|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        passed=$(grep "^$category|PASS|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        failed=$(grep "^$category|FAIL|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        
        if [ $total -gt 0 ]; then
            rate=$(( passed * 100 / total ))
            printf "   %-30s %2d/%2d tests (%3d%%) " "$category:" $passed $total $rate
            if [ $failed -eq 0 ]; then
                echo "✅ All passing"
            else
                echo "❌ $failed failed"
            fi
        fi
    done
else
    echo "   ⚠️  No results file found - categories not tracked"
fi
echo ""

# Show detailed failures if any
if [ $FAILED_TESTS -gt 0 ]; then
    echo "❌ DETAILED FAILURE BREAKDOWN:"
    if [ -f .continuum/tests/test_results.tmp ]; then
        categories=$(cut -d'|' -f1 .continuum/tests/test_results.tmp | sort -u)
        for category in $categories; do
            failed_entries=$(grep "^$category|FAIL|" .continuum/tests/test_results.tmp)
            if [ ! -z "$failed_entries" ]; then
                failed_count=$(echo "$failed_entries" | wc -l | tr -d ' ')
                echo "   🔴 $category ($failed_count failed):"
                echo "$failed_entries" | while IFS='|' read -r cat status test_name error_details; do
                    echo "      • $test_name"
                    if [ ! -z "$error_details" ]; then
                        echo "        💥 Error:"
                        echo "$error_details"
                    fi
                    echo ""
                done
                echo ""
            fi
        done
    fi
    
    echo "🔍 Recommended Next Steps:"
    echo "   1. Re-run with --verbose for detailed test output:"
    echo "      npx tsx scripts/test-with-server.ts --verbose"
    echo "   2. Check detailed logs in test session files (preserved above)"
    echo "   3. Run specific failing tests individually for focused debugging"
    echo "   4. Check system logs: .continuum/jtag/system/logs/"
    echo "   5. Verify system health: npm run agent:quick"
    echo ""
    
    # Keep temp file for debugging if needed
    echo "🔍 Full results available in: .continuum/tests/test_results.tmp"
    exit 1
else
    echo "🎉 ALL TESTS PASSED!"
    echo "✅ Profile '$PROFILE' is fully functional"
    echo ""
    echo "🚀 System ready for autonomous development"
    
    # Clean up temp file after successful run
    rm -f .continuum/tests/test_results.tmp
fi