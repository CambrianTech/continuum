#!/bin/bash

# Comprehensive Test Suite Runner with Result Aggregation
# Runs all tests and provides comprehensive summary regardless of individual failures

# Remove set -e so we can continue after failures and aggregate results
# set -e  # REMOVED - we want to collect all results

# Ensure we have bash 4+ for associative arrays
if [ "${BASH_VERSION%%.*}" -lt 4 ]; then
    echo "⚠️  Warning: This script requires bash 4+ for full category support"
    echo "Using simplified reporting mode..."
    SIMPLE_MODE=true
else
    SIMPLE_MODE=false
fi

echo "🚀 AUTONOMOUS TEST SUITE - Full End-to-End"

# Initialize result tracking
declare -i TOTAL_TESTS=0
declare -i PASSED_TESTS=0
declare -i FAILED_TESTS=0
declare -a FAILED_TEST_NAMES=()

# Initialize temp file for category tracking using .continuum pattern
mkdir -p .continuum/tests
> .continuum/tests/test_results.tmp

# Category tracking using simple arrays (bash 3+ compatible)
CATEGORIES=()
CATEGORY_RESULTS=()

# Function to track category results
track_category() {
    local category="$1"
    local passed="$2"
    local total="$3"
    local failed_names="$4"
    
    CATEGORIES+=("$category")
    CATEGORY_RESULTS+=("$passed/$total|$failed_names")
}

# Function to run test and track results by category
run_test() {
    local test_name="$1"
    local test_command="$2"
    local category="$3"
    
    echo "▶️  Running: $test_name [$category]"
    ((TOTAL_TESTS++))
    
    if eval "$test_command" &>/dev/null; then
        echo "✅ PASSED: $test_name"
        ((PASSED_TESTS++))
        # Track success for category summary later
        echo "$category|PASS|$test_name" >> .continuum/tests/test_results.tmp
    else
        echo "❌ FAILED: $test_name"
        ((FAILED_TESTS++))
        FAILED_TEST_NAMES+=("$test_name")
        # Track failure for category summary later
        echo "$category|FAIL|$test_name" >> .continuum/tests/test_results.tmp
    fi
    echo ""
}

# Compiler checks first
run_test "Compiler Error Detection" "npx tsx tests/compiler-error-detection.test.ts" "Compiler & Build"

# Core system tests
run_test "Bootstrap Tests" "npx tsx tests/bootstrap-comprehensive.test.ts" "Core System"
run_test "Browser Integration" "npx tsx tests/integration/browser-automated-tests.test.ts" "Browser Integration"
run_test "Router Coordination" "npx tsx tests/integration/router-coordination-simple.test.ts" "Core System"

# Screenshot and visual tests
run_test "Server Screenshot" "npx tsx tests/server-screenshot.test.ts" "Screenshots & Visual"
run_test "Screenshot Verification" "npx tsx tests/screenshot-verification.test.ts" "Screenshots & Visual"
run_test "Screenshot Integration Advanced" "npx tsx tests/screenshot-integration-advanced.test.ts" "Screenshots & Visual"

# Signal system tests
run_test "Signal System" "npx tsx tests/signal-system.test.ts" "System Signals"

# Chat integration tests
run_test "Chat Daemon Integration" "npx tsx tests/chat-daemon-integration.test.ts" "Chat & Messaging"

# Unit tests
run_test "Router Broadcast Unit" "npx tsx tests/unit/router-broadcast.test.ts" "Unit Tests"
run_test "Room Scoped Event Routing" "npx tsx tests/unit/room-scoped-event-routing.test.ts" "Unit Tests"
run_test "Events Daemon Unit" "npx tsx tests/unit/events-daemon-unit.test.ts" "Unit Tests"

# Cross-environment event tests
run_test "Server-Browser Event Flow" "npx tsx tests/integration/server-browser-event-flow.test.ts" "Cross-Environment Events"
run_test "Browser-Server Event Flow" "npx tsx tests/integration/browser-server-event-flow.test.ts" "Cross-Environment Events"
run_test "Chat Widget Room Events" "npx tsx tests/integration/chat-widget-room-events.test.ts" "Cross-Environment Events"
run_test "Cross-Environment Events Working" "npx tsx tests/integration/cross-environment-events-working.test.ts" "Cross-Environment Events"

# Multi-user chat tests
run_test "Simple Multiuser Chat" "npx tsx tests/integration/simple-multiuser-chat.test.ts" "Chat & Messaging"
run_test "Server to Browser Chat Proof" "npx tsx tests/integration/server-to-browser-chat-proof.test.ts" "Chat & Messaging"

# COMPREHENSIVE TEST SUMMARY WITH CATEGORY BREAKDOWN
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "🎯 COMPREHENSIVE TEST SUITE RESULTS"
echo "═══════════════════════════════════════════════════════════════════════════════"
echo "📊 Overall Summary:"
echo "   Total Tests: $TOTAL_TESTS"
echo "   ✅ Passed: $PASSED_TESTS"
echo "   ❌ Failed: $FAILED_TESTS"
echo "   📈 Success Rate: $(( PASSED_TESTS * 100 / TOTAL_TESTS ))%"
echo ""

echo "📋 Results by Category:"
if [ -f .continuum/tests/test_results.tmp ]; then
    # Process category results from temp file
    for category in "Compiler & Build" "Core System" "Browser Integration" "Screenshots & Visual" "System Signals" "Chat & Messaging" "Unit Tests" "Cross-Environment Events"; do
        total=$(grep "^$category|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        passed=$(grep "^$category|PASS|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        failed=$(grep "^$category|FAIL|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
        
        if [ $total -gt 0 ]; then
            rate=$(( passed * 100 / total ))
            printf "   %-25s %2d/%2d tests (%3d%%) " "$category:" $passed $total $rate
            if [ $failed -eq 0 ]; then
                echo "✅ All passing"
            else
                echo "❌ $failed failed"
            fi
        fi
    done
fi
echo ""

if [ $FAILED_TESTS -gt 0 ]; then
    echo "❌ DETAILED FAILURE BREAKDOWN:"
    if [ -f .continuum/tests/test_results.tmp ]; then
        for category in "Compiler & Build" "Core System" "Browser Integration" "Screenshots & Visual" "System Signals" "Chat & Messaging" "Unit Tests" "Cross-Environment Events"; do
            failed_tests=$(grep "^$category|FAIL|" .continuum/tests/test_results.tmp | cut -d'|' -f3)
            if [ ! -z "$failed_tests" ]; then
                failed_count=$(echo "$failed_tests" | wc -l | tr -d ' ')
                echo "   🔴 $category ($failed_count failed):"
                echo "$failed_tests" | sed 's/^/      • /'
                echo ""
            fi
        done
    fi
    
    echo "🔍 Recommended Next Steps:"
    echo "   1. Focus on categories with highest failure rates"
    echo "   2. Run individual tests for detailed output:"
    echo "      npm run test:compiler-error-detection"
    echo "      npm run test:simple"
    echo "   3. Check system logs: .continuum/jtag/system/logs/"
    echo "   4. Verify system health: npm run agent:quick"
    echo ""
    
    # Clean up temp file
    rm -f .continuum/tests/test_results.tmp
    exit 1
else
    echo "🎉 ALL TESTS PASSED!"
    echo "✅ Every category is fully functional:"
    if [ -f .continuum/tests/test_results.tmp ]; then
        for category in "Compiler & Build" "Core System" "Browser Integration" "Screenshots & Visual" "System Signals" "Chat & Messaging" "Unit Tests" "Cross-Environment Events"; do
            total=$(grep "^$category|" .continuum/tests/test_results.tmp | wc -l | tr -d ' ')
            if [ $total -gt 0 ]; then
                echo "   ✅ $category: $total/$total tests passing"
            fi
        done
    fi
    echo ""
    echo "🚀 System is fully functional and ready for autonomous development"
    
    # Clean up temp file
    rm -f .continuum/tests/test_results.tmp
fi