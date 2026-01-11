#!/bin/bash

# Continuum Emotional Feedback - End-to-End Test Script
#
# This script tests the continuum/set command and takes screenshots
# to visually verify the Continuum widget updates.
#
# Prerequisites: npm start must be running

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JTAG_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUTPUT_DIR="$JTAG_ROOT/.continuum/test-screenshots/continuum"

echo "🧪 Continuum Emotional Feedback E2E Tests"
echo "=========================================="
echo ""

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR"

cd "$JTAG_ROOT"

# Helper function to test continuum/set and screenshot
test_continuum() {
    local test_name="$1"
    local filename="$2"
    shift 2
    local args="$@"

    echo "📝 Test: $test_name"

    # Call continuum/set
    ./jtag continuum/set $args

    # Take screenshot
    ./jtag screenshot --querySelector="continuum-widget" --output="$OUTPUT_DIR/$filename"

    echo "✅ Screenshot saved: $OUTPUT_DIR/$filename"
    echo ""

    # Small delay for visual separation
    sleep 0.5
}

# Test 1: Emoji + Message + Color
test_continuum \
    "Emoji search status" \
    "status-emoji-search.png" \
    --emoji="🔍" --color="blue" --message="Searching codebase" --duration=10000

# Test 2: Color only (red error)
test_continuum \
    "Red error status" \
    "status-color-red.png" \
    --color="red" --message="Error detected" --duration=10000

# Test 3: Message only
test_continuum \
    "Message-only status" \
    "status-message-only.png" \
    --message="Processing request..." --duration=10000

# Test 4: Warning emoji
test_continuum \
    "Warning status" \
    "status-warning.png" \
    --emoji="⚠️" --color="yellow" --message="Warning: High memory usage" --duration=10000

# Test 5: Success emoji
test_continuum \
    "Success status" \
    "status-success.png" \
    --emoji="✅" --color="green" --message="Operation completed" --duration=10000

# Test 6: Robot emoji (AI active)
test_continuum \
    "AI active status" \
    "status-ai-active.png" \
    --emoji="🤖" --color="cyan" --message="AI system active" --duration=10000

# Test 7: Clear status
echo "📝 Test: Clear status"
./jtag continuum/set --clear=true
./jtag screenshot --querySelector="continuum-widget" --output="$OUTPUT_DIR/status-cleared.png"
echo "✅ Screenshot saved: $OUTPUT_DIR/status-cleared.png"
echo ""

# Test 8: Full page screenshot with status
./jtag continuum/set --emoji="🎯" --color="purple" --message="Testing complete" --duration=10000
./jtag screenshot --querySelector="body" --output="$OUTPUT_DIR/full-page-with-status.png"
echo "✅ Full page screenshot saved"
echo ""

# Summary
echo "=========================================="
echo "✅ All tests complete!"
echo ""
echo "📂 Screenshots saved to:"
echo "   $OUTPUT_DIR"
echo ""
echo "View screenshots:"
echo "   ls -la $OUTPUT_DIR"
echo "   open $OUTPUT_DIR  # macOS"
echo ""
