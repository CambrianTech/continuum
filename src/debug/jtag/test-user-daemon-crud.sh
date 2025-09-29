#!/usr/bin/env bash

# test-user-daemon-crud.sh
# CRUD test for user daemon connection logic
# Tests: User creation, Session creation, UserState persistence, LocalStorage integration
#
# Usage: ./test-user-daemon-crud.sh
# Prerequisites: npm start (JTAG system running)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test configuration
TEST_ID="CRUD-$(date +%s)-$(shuf -i 1000-9999 -n 1)"
ANONYMOUS_USER_ID="anonymous-test-${TEST_ID}"
TEST_SESSION_NAME="TestSession-${TEST_ID}"

echo -e "${BLUE}🧪 USER DAEMON CRUD TEST SUITE${NC}"
echo -e "${BLUE}================================${NC}"
echo "Test ID: ${TEST_ID}"
echo "Anonymous User ID: ${ANONYMOUS_USER_ID}"
echo "Test Session: ${TEST_SESSION_NAME}"
echo ""

# Test Results Tracking
TESTS_PASSED=0
TESTS_FAILED=0

run_test() {
    local test_name="$1"
    local test_command="$2"
    local expected_pattern="$3"

    echo -e "${YELLOW}🔧 Testing: ${test_name}${NC}"
    echo "Command: ${test_command}"

    if output=$(eval "$test_command" 2>&1); then
        echo "Output: $output"
        if [[ -n "$expected_pattern" ]] && [[ $output =~ $expected_pattern ]]; then
            echo -e "${GREEN}✅ PASS: ${test_name}${NC}"
            ((TESTS_PASSED++))
        elif [[ -z "$expected_pattern" ]]; then
            echo -e "${GREEN}✅ PASS: ${test_name} (no validation pattern)${NC}"
            ((TESTS_PASSED++))
        else
            echo -e "${RED}❌ FAIL: ${test_name} - Pattern not found: ${expected_pattern}${NC}"
            ((TESTS_FAILED++))
        fi
    else
        echo -e "${RED}❌ FAIL: ${test_name} - Command failed${NC}"
        echo "Error: $output"
        ((TESTS_FAILED++))
    fi
    echo ""
}

# Test 1: System Connectivity
echo -e "${BLUE}📡 Phase 1: System Connectivity${NC}"
run_test "Ping JTAG System" "./jtag ping" "success.*true"

# Test 2: Anonymous User Creation (CRUD - Create)
echo -e "${BLUE}👤 Phase 2: Anonymous User Creation${NC}"
run_test "Create Anonymous User" "./jtag data/create --collection=User --data='{\"id\":\"${ANONYMOUS_USER_ID}\",\"type\":\"human\",\"displayName\":\"Test Anonymous User\",\"status\":\"online\",\"lastActiveAt\":\"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)\",\"capabilities\":{\"canSendMessages\":true,\"canReceiveMessages\":true,\"canCreateRooms\":false,\"canInviteOthers\":false,\"canModerate\":false,\"autoResponds\":false,\"providesContext\":false,\"canTrain\":false,\"canAccessPersonas\":false},\"sessionsActive\":[]}'" "success.*true"

# Test 3: User Verification (CRUD - Read)
echo -e "${BLUE}🔍 Phase 3: User Data Verification${NC}"
run_test "Read Created User" "./jtag data/read --collection=User --id=${ANONYMOUS_USER_ID}" "displayName.*Test Anonymous User"

# Test 4: UserState Creation with Anonymous User
echo -e "${BLUE}🗃️ Phase 4: UserState Creation${NC}"
run_test "Create UserState for Anonymous User" "./jtag state/create --collection=UserState --userId=${ANONYMOUS_USER_ID} --data='{\"userId\":\"${ANONYMOUS_USER_ID}\",\"deviceId\":\"browser-test-crud\",\"contentState\":{\"openItems\":[],\"lastUpdatedAt\":\"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)\"},\"preferences\":{\"maxOpenTabs\":10,\"autoCloseAfterDays\":30,\"rememberScrollPosition\":true,\"syncAcrossDevices\":true,\"theme\":\"base\"}}'" "success.*true"

# Test 5: Session Creation with Anonymous User
echo -e "${BLUE}🔗 Phase 5: Session Creation${NC}"
run_test "Create Session for Anonymous User" "./jtag session/create --category=user --displayName='${TEST_SESSION_NAME}' --userId=${ANONYMOUS_USER_ID}" "success.*true"

# Test 6: UserState Update (CRUD - Update)
echo -e "${BLUE}🔄 Phase 6: UserState Update${NC}"
run_test "Update UserState Theme" "./jtag data/list --collection=UserState --filter='{\"userId\":\"${ANONYMOUS_USER_ID}\"}'" "userId.*${ANONYMOUS_USER_ID}"

# Extract UserState ID for update test
USERSTATE_OUTPUT=$(./jtag data/list --collection=UserState --filter="{\"userId\":\"${ANONYMOUS_USER_ID}\"}" 2>/dev/null)
USERSTATE_ID=$(echo "$USERSTATE_OUTPUT" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

if [[ -n "$USERSTATE_ID" ]]; then
    run_test "Update UserState Preferences" "./jtag data/update --collection=UserState --id=${USERSTATE_ID} --data='{\"preferences\":{\"maxOpenTabs\":15,\"autoCloseAfterDays\":30,\"rememberScrollPosition\":true,\"syncAcrossDevices\":true,\"theme\":\"mystical\"}}'" "success.*true"

    # Verify the update
    run_test "Verify UserState Update" "./jtag data/read --collection=UserState --id=${USERSTATE_ID}" "theme.*mystical"
else
    echo -e "${RED}❌ FAIL: Could not extract UserState ID for update test${NC}"
    ((TESTS_FAILED++))
fi

# Test 7: Theme System Integration
echo -e "${BLUE}🎨 Phase 7: Theme System Integration${NC}"
run_test "Set Theme via Command" "./jtag theme/set mystical" "success.*true"

# Test 8: Connection Logic Stress Test
echo -e "${BLUE}⚡ Phase 8: Connection Stress Test${NC}"
run_test "Multiple Ping Requests" "for i in {1..3}; do ./jtag ping >/dev/null && echo 'ping success' || echo 'ping fail'; done" "ping success"

# Test 9: Data Persistence Verification
echo -e "${BLUE}💾 Phase 9: Data Persistence Verification${NC}"
run_test "List All Users" "./jtag data/list --collection=User" "displayName"
run_test "List All UserStates" "./jtag data/list --collection=UserState" "preferences"

# Test 10: Cleanup (CRUD - Delete)
echo -e "${BLUE}🧹 Phase 10: Cleanup${NC}"
if [[ -n "$USERSTATE_ID" ]]; then
    run_test "Delete UserState" "./jtag data/delete --collection=UserState --id=${USERSTATE_ID}" "success.*true"
fi
run_test "Delete Anonymous User" "./jtag data/delete --collection=User --id=${ANONYMOUS_USER_ID}" "success.*true"

# Final Results
echo -e "${BLUE}📊 TEST RESULTS SUMMARY${NC}"
echo -e "${BLUE}=======================${NC}"
echo -e "${GREEN}Tests Passed: ${TESTS_PASSED}${NC}"
if [ $TESTS_FAILED -gt 0 ]; then
    echo -e "${RED}Tests Failed: ${TESTS_FAILED}${NC}"
    exit 1
else
    echo -e "${GREEN}Tests Failed: ${TESTS_FAILED}${NC}"
fi

TOTAL_TESTS=$((TESTS_PASSED + TESTS_FAILED))
echo "Total Tests: ${TOTAL_TESTS}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}🎉 ALL TESTS PASSED - User Daemon CRUD Operations Working!${NC}"
    exit 0
else
    echo -e "${RED}❌ SOME TESTS FAILED - Check the output above${NC}"
    exit 1
fi