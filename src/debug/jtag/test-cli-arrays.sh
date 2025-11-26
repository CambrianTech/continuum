#!/bin/bash
# Test CLI array parameter parsing with repeated flags

echo "🧪 Testing CLI Array Parameter Parsing"
echo "======================================"
echo ""

echo "Test 1: Single --media value (should remain string)"
./jtag chat/send --message="Test single image" --media ../../../test-images/image-1.webp --room="general" 2>&1 | grep -A 5 "success\|error\|media"
echo ""

echo "Test 2: Multiple --media values (should become array)"
./jtag chat/send --message="Test multiple images" \
  --media ../../../test-images/image-1.webp \
  --media ../../../test-images/image-3.jpg \
  --media ../../../test-images/image-6.png \
  --room="general" 2>&1 | grep -A 10 "success\|error\|media"
echo ""

echo "Test 3: Backward compat - JSON array syntax (should still work)"
./jtag chat/send --message="Test JSON array" \
  --media='["../../../test-images/image-1.webp","../../../test-images/image-3.jpg"]' \
  --room="general" 2>&1 | grep -A 10 "success\|error\|media"
echo ""

echo "Test 4: Mixed parameters (media array + other strings)"
./jtag chat/send --message="Mixed test" \
  --media ../../../test-images/image-1.webp \
  --room="general" \
  --media ../../../test-images/image-3.jpg 2>&1 | grep -A 10 "success\|error\|media\|room"
echo ""

echo "Test 5: Invalid path (should fail with clear error)"
./jtag chat/send --message="Test error handling" \
  --media /this/does/not/exist.png \
  --room="general" 2>&1 | grep -A 3 "success\|error"
echo ""

echo "✅ All CLI array tests complete!"
