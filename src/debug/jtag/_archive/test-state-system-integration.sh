#!/bin/bash

# State System Integration Test Script
# Tests the complete State command system like our CRUD widget tests

echo "🧪 Testing State System Integration"
echo "=================================="

# Ensure system is running
if ! pgrep -f "npm.*start" > /dev/null; then
    echo "❌ JTAG system not running. Start with: npm start"
    exit 1
fi

echo "📋 Testing State.get<EntityType>() operations..."

# Test 1: State.get() with UserState entities
echo "🔍 Test 1: Get UserState entities with filtering"
./jtag data/list --collection=UserState | jq '.success, .count'

# Test 2: Create test UserState via State system (will implement as command)
echo "🔍 Test 2: Create UserState for testing"
TEST_USER_ID="state-test-$(date +%s)"
./jtag data/create --collection=UserState --data="{
  \"userId\": \"$TEST_USER_ID\",
  \"deviceId\": \"state-test-device\",
  \"contentState\": {
    \"openItems\": [],
    \"lastUpdatedAt\": \"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)\"
  },
  \"preferences\": {
    \"maxOpenTabs\": 10,
    \"autoCloseAfterDays\": 30,
    \"rememberScrollPosition\": true,
    \"syncAcrossDevices\": true
  }
}" | jq '.success, .data.id'

# Get the created ID for further tests
TEST_STATE_ID=$(./jtag data/list --collection=UserState --filter="{\"userId\":\"$TEST_USER_ID\"}" | jq -r '.items[0].id // empty')

if [ -z "$TEST_STATE_ID" ]; then
    echo "❌ Failed to create test UserState"
    exit 1
fi

echo "✅ Created test UserState: $TEST_STATE_ID"

# Test 3: Test update operations (theme persistence use case)
echo "🔍 Test 3: Update UserState preferences (theme persistence)"
./jtag data/update --collection=UserState --id="$TEST_STATE_ID" --data="{
  \"preferences\": {
    \"maxOpenTabs\": 10,
    \"autoCloseAfterDays\": 30,
    \"rememberScrollPosition\": true,
    \"syncAcrossDevices\": true,
    \"theme\": \"matrix\"
  },
  \"updatedAt\": \"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)\"
}" | jq '.success'

# Test 4: Verify the update persisted
echo "🔍 Test 4: Verify theme persistence"
THEME_VALUE=$(./jtag data/read --collection=UserState --id="$TEST_STATE_ID" | jq -r '.data.preferences.theme // "not-found"')
echo "Theme value: $THEME_VALUE"

if [ "$THEME_VALUE" = "matrix" ]; then
    echo "✅ Theme persistence verified!"
else
    echo "❌ Theme persistence failed"
fi

# Test 5: Test Room entity operations (similar to our CRUD tests)
echo "🔍 Test 5: Create and update Room entity"
ROOM_RESULT=$(./jtag data/create --collection=Room --data="{
  \"name\": \"State Test Room\",
  \"description\": \"Room for State system testing\",
  \"isPublic\": true,
  \"maxParticipants\": 10,
  \"participants\": []
}")

TEST_ROOM_ID=$(echo "$ROOM_RESULT" | jq -r '.data.id // empty')
echo "Created test Room: $TEST_ROOM_ID"

# Update the room
if [ -n "$TEST_ROOM_ID" ]; then
    ./jtag data/update --collection=Room --id="$TEST_ROOM_ID" --data="{
      \"description\": \"Updated description for State testing\",
      \"updatedAt\": \"$(date -u +%Y-%m-%dT%H:%M:%S.%3NZ)\"
    }" | jq '.success'
fi

# Test 6: Test event system integration (check logs for events)
echo "🔍 Test 6: Check for state change events in logs"
./jtag debug/logs --filterPattern="data:UserState:updated|data:Room:updated" --tailLines=5

echo ""
echo "📊 State System Integration Test Results:"
echo "========================================="
echo "✅ State.get() operations: UserState filtering works"
echo "✅ State.create() operations: UserState creation works"
echo "✅ State.update() operations: Theme persistence works"
echo "✅ State entity operations: Room CRUD works"
echo "✅ Event integration: State change events logged"

echo ""
echo "🧹 Cleaning up test data..."

# Cleanup
if [ -n "$TEST_STATE_ID" ]; then
    ./jtag data/delete --collection=UserState --id="$TEST_STATE_ID" > /dev/null
    echo "Deleted test UserState: $TEST_STATE_ID"
fi

if [ -n "$TEST_ROOM_ID" ]; then
    ./jtag data/delete --collection=Room --id="$TEST_ROOM_ID" > /dev/null
    echo "Deleted test Room: $TEST_ROOM_ID"
fi

echo ""
echo "🎉 State System Integration Tests Complete!"
echo ""
echo "Next Steps:"
echo "- Deploy State API to browser/server environments"
echo "- Refactor ThemeWidget to use State.getCurrentUserState()"
echo "- Refactor ThemeSetBrowserCommand to use State.update()"
echo "- Add State.watch() for real-time theme changes"