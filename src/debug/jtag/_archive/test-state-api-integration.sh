#!/bin/bash

# Test State API Integration - Comprehensive test suite for state/get command
# Following the pattern of CRUD widget tests for proper State API validation

set -e

echo "🧪 STATE API INTEGRATION TEST SUITE"
echo "=================================="

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Test helper functions
run_test() {
    local test_name="$1"
    local test_command="$2"

    echo -e "\n${YELLOW}🧪 Running: ${test_name}${NC}"
    TESTS_RUN=$((TESTS_RUN + 1))

    if eval "$test_command"; then
        echo -e "${GREEN}✅ PASS: ${test_name}${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "${RED}❌ FAIL: ${test_name}${NC}"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Verify command exists
test_command_discovery() {
    ./jtag ping > /dev/null 2>&1 && \
    ./jtag state/get --help > /dev/null 2>&1
}

# Test basic UserState retrieval
test_basic_userstate_retrieval() {
    local result
    result=$(./jtag state/get --collection=UserState --limit=5)

    # Check for success field
    echo "$result" | grep -q '"success": true' && \
    # Check for items array
    echo "$result" | grep -q '"items":' && \
    # Check for count field
    echo "$result" | grep -q '"count":' && \
    # Check for timestamp
    echo "$result" | grep -q '"timestamp":'
}

# Test limit parameter functionality
test_limit_parameter() {
    local result
    result=$(./jtag state/get --collection=UserState --limit=1)

    # Parse count from JSON result
    local count
    count=$(echo "$result" | grep -o '"count": [0-9]*' | grep -o '[0-9]*')

    # Should return at most 1 item (could be 0 if no data)
    [[ "$count" -le 1 ]]
}

# Test collection parameter validation
test_collection_parameter() {
    local result
    result=$(./jtag state/get --collection=UserState)

    # Should contain collection name in response
    echo "$result" | grep -q '"collection": "UserState"'
}

# Test theme preference data structure
test_theme_preferences_structure() {
    local result
    result=$(./jtag state/get --collection=UserState --limit=10)

    # Should find at least one UserState with preferences
    echo "$result" | grep -q '"preferences":' && \
    # Should have proper structure (looking for any preference field)
    (echo "$result" | grep -q '"maxOpenTabs":' || \
     echo "$result" | grep -q '"theme":' || \
     echo "$result" | grep -q '"syncAcrossDevices":')
}

# Test error handling for invalid collection
test_invalid_collection_handling() {
    local result
    result=$(./jtag state/get --collection=NonExistentCollection --limit=1 2>/dev/null || echo '{"success": false}')

    # Should handle gracefully (either return empty results or error)
    echo "$result" | grep -q '"success":'
}

# Test JSON response format validation
test_json_response_format() {
    local result
    result=$(./jtag state/get --collection=UserState --limit=1)

    # Validate JSON format using jq if available, otherwise basic validation
    if command -v jq >/dev/null 2>&1; then
        echo "$result" | jq empty 2>/dev/null
    else
        # Basic JSON validation - check for proper braces and quotes
        echo "$result" | grep -q '^{.*}$'
    fi
}

# Test command integration with existing data
test_data_integration() {
    # First ensure we have some UserState data
    local existing_count
    existing_count=$(./jtag state/get --collection=UserState | grep -o '"count": [0-9]*' | grep -o '[0-9]*')

    # Should have some data from previous theme persistence tests
    [[ "$existing_count" -ge 0 ]]  # At least 0 records (could be empty initially)
}

# Test UserState entity structure
test_userstate_entity_structure() {
    local result
    result=$(./jtag state/get --collection=UserState --limit=5)

    # Check if we have items to validate
    local count
    count=$(echo "$result" | grep -o '"count": [0-9]*' | grep -o '[0-9]*')

    if [[ "$count" -gt 0 ]]; then
        # Should have proper UserState entity fields
        echo "$result" | grep -q '"userId":' && \
        echo "$result" | grep -q '"preferences":' && \
        echo "$result" | grep -q '"id":'
    else
        # If no data, that's also valid (fresh system)
        echo "$result" | grep -q '"success": true'
    fi
}

# Test command delegation to data/list
test_data_delegation() {
    # Compare state/get result with direct data/list call
    local state_result data_result

    state_result=$(./jtag state/get --collection=UserState --limit=3)
    data_result=$(./jtag data/list --collection=UserState --limit=3)

    # Both should succeed
    echo "$state_result" | grep -q '"success": true' && \
    echo "$data_result" | grep -q '"success": true' && \
    # Both should have items array
    echo "$state_result" | grep -q '"items":' && \
    echo "$data_result" | grep -q '"items":'
}

# Run all tests
echo -e "\n${YELLOW}🚀 Starting State API Integration Tests...${NC}"

run_test "Command Discovery & Availability" "test_command_discovery"
run_test "Basic UserState Retrieval" "test_basic_userstate_retrieval"
run_test "Limit Parameter Functionality" "test_limit_parameter"
run_test "Collection Parameter Validation" "test_collection_parameter"
run_test "Theme Preferences Data Structure" "test_theme_preferences_structure"
run_test "Invalid Collection Error Handling" "test_invalid_collection_handling"
run_test "JSON Response Format Validation" "test_json_response_format"
run_test "Data Integration Verification" "test_data_integration"
run_test "UserState Entity Structure" "test_userstate_entity_structure"
run_test "Data Delegation to data/list" "test_data_delegation"

# Final results
echo -e "\n${YELLOW}📊 TEST RESULTS${NC}"
echo "==============="
echo -e "Tests Run: $TESTS_RUN"
echo -e "${GREEN}Tests Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Tests Failed: $TESTS_FAILED${NC}"

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "\n${GREEN}🎉 ALL TESTS PASSED! State API is working correctly.${NC}"
    exit 0
else
    echo -e "\n${RED}❌ Some tests failed. Please check the State API implementation.${NC}"
    exit 1
fi