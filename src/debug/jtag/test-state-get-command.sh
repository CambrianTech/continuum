#!/bin/bash

# Quick State Get Command Test - Can be run via JTAG command system
# Similar to other command tests in the system

set -e

echo "🧪 Testing state/get command functionality..."

# Test 1: Basic command execution
echo "1. Testing basic command execution..."
RESULT=$(./jtag state/get --collection=UserState --limit=3)
if echo "$RESULT" | grep -q '"success": true'; then
    echo "✅ Basic execution test passed"
else
    echo "❌ Basic execution test failed"
    exit 1
fi

# Test 2: Response structure validation
echo "2. Testing response structure..."
if echo "$RESULT" | grep -q '"items":' && echo "$RESULT" | grep -q '"count":' && echo "$RESULT" | grep -q '"timestamp":'; then
    echo "✅ Response structure test passed"
else
    echo "❌ Response structure test failed"
    exit 1
fi

# Test 3: UserState entity structure (if data exists)
echo "3. Testing UserState entity structure..."
COUNT=$(echo "$RESULT" | grep -o '"count": [0-9]*' | grep -o '[0-9]*')
if [[ "$COUNT" -gt 0 ]]; then
    if echo "$RESULT" | grep -q '"userId":' && echo "$RESULT" | grep -q '"preferences":'; then
        echo "✅ UserState entity structure test passed"
    else
        echo "❌ UserState entity structure test failed"
        exit 1
    fi
else
    echo "✅ No UserState data found (valid for fresh system)"
fi

# Test 4: Theme preferences (if data exists with theme)
echo "4. Testing theme preferences..."
if echo "$RESULT" | grep -q '"theme":'; then
    echo "✅ Theme preferences found and structured correctly"
else
    echo "⚠️ No theme preferences found (valid if no themes set yet)"
fi

echo ""
echo "🎉 All state/get command tests passed!"
echo "Command is working correctly and ready for use."