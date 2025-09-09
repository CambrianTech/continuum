#!/bin/bash

# Comprehensive Chat Send Testing - All Scenarios
# Tests different patterns of message sending to identify duplication and attribution issues
# Run after `npm start` to test all chat functionality patterns

echo "🧪 Starting comprehensive chat send test - ALL SCENARIOS..."
echo "Make sure you ran 'npm start' first!"
echo ""

# Helper function to send a message
send_message() {
    local message="$1"
    local test_name="$2"
    
    echo "📝 $test_name: Sending \"$message\"..."
    ./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = '$message';
  chatWidget.sendMessage();
  return 'Message sent: $message';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"
}

# Helper function to send via Enter key
send_message_enter() {
    local message="$1"
    local test_name="$2"
    
    echo "📝 $test_name: Sending via ENTER key \"$message\"..."
    ./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input) {
  input.value = '$message';
  // Simulate Enter key press
  const event = new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 13 });
  input.dispatchEvent(event);
  return 'Message sent via Enter: $message';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"
}

# SCENARIO 1: Sequential sends (original pattern test)
echo "🔥 SCENARIO 1: Sequential Message Pattern (Original Issue)"
send_message "FIRST MESSAGE - should not be doubled" "Test 1A"
sleep 2

send_message "SECOND MESSAGE - check if doubled" "Test 1B"
sleep 2

send_message "THIRD MESSAGE - doubling pattern test" "Test 1C"
sleep 3

# SCENARIO 2: Rapid fire sends (stress test)
echo ""
echo "🔥 SCENARIO 2: Rapid Fire Sends (Stress Test)"
send_message "RAPID-1 - immediate send" "Test 2A"
send_message "RAPID-2 - no delay" "Test 2B" 
send_message "RAPID-3 - stress test" "Test 2C"
sleep 3

# SCENARIO 3: Different message lengths
echo ""
echo "🔥 SCENARIO 3: Variable Message Lengths"
send_message "short" "Test 3A - Short"
sleep 1

send_message "This is a medium length message to test different content sizes and see if length affects attribution or duplication patterns in any way" "Test 3B - Long"
sleep 1

send_message "📝🚀💬🎯✅" "Test 3C - Emojis"
sleep 3

# SCENARIO 4: Enter key vs Click button
echo ""
echo "🔥 SCENARIO 4: Enter Key vs Click Button"
send_message "CLICK-BUTTON-TEST - via sendMessage()" "Test 4A - Click"
sleep 2

send_message_enter "ENTER-KEY-TEST - via Enter keydown" "Test 4B - Enter Key"
sleep 3

# SCENARIO 5: Special characters and edge cases
echo ""
echo "🔥 SCENARIO 5: Special Characters & Edge Cases"
send_message "Test with 'quotes' and \"double quotes\"" "Test 5A - Quotes"
sleep 1

send_message "Test with newlines\nand\ttabs" "Test 5B - Whitespace"
sleep 1

send_message "{\"json\": true, \"test\": 123}" "Test 5C - JSON"
sleep 1

send_message "" "Test 5D - Empty Message"
sleep 3

# SCENARIO 6: Mixed timing patterns
echo ""
echo "🔥 SCENARIO 6: Mixed Timing Patterns"
send_message "TIMING-TEST-1 - short delay next" "Test 6A"
sleep 0.5

send_message "TIMING-TEST-2 - medium delay next" "Test 6B"
sleep 2

send_message "TIMING-TEST-3 - long delay next" "Test 6C"
sleep 5

send_message "TIMING-TEST-4 - after long delay" "Test 6D"
sleep 3

echo ""
echo "⏳ Waiting for all messages to process..."
sleep 5

# COMPREHENSIVE ANALYSIS
echo "📊 Analyzing comprehensive chat state..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

if (!chatWidget) return { error: 'Chat widget not found' };

const messages = chatWidget.messages || [];
const currentSessionId = chatWidget.currentSessionId;

// Count messages by content to detect duplicates  
const messageCounts = {};
const testMessages = [];
const duplicates = [];

messages.forEach(msg => {
  messageCounts[msg.content] = (messageCounts[msg.content] || 0) + 1;
  if (msg.content.includes('TEST') || msg.content.includes('MESSAGE') || msg.content.includes('RAPID') || msg.content.includes('TIMING')) {
    testMessages.push({
      content: msg.content.slice(0, 50) + (msg.content.length > 50 ? '...' : ''),
      type: msg.type,
      senderId: msg.senderId,
      count: messageCounts[msg.content],
      isCurrentUser: currentSessionId && msg.senderId === currentSessionId
    });
  }
});

// Find duplicated messages
Object.entries(messageCounts).forEach(([content, count]) => {
  if (count > 1 && (content.includes('TEST') || content.includes('MESSAGE') || content.includes('RAPID') || content.includes('TIMING'))) {
    duplicates.push({ content: content.slice(0, 50) + '...', count });
  }
});

const currentUserMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId
).length;

const wrongSideMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId && msg.type !== 'user'
).length;

console.log('');
console.log('🏁 COMPREHENSIVE TEST RESULTS:');
console.log('===============================');
console.log('Total messages:', messages.length);
console.log('Test messages found:', testMessages.length);
console.log('Current user messages:', currentUserMessages);  
console.log('Messages on wrong side:', wrongSideMessages);
console.log('Attribution working:', currentUserMessages > 0 && wrongSideMessages === 0);
console.log('Duplicated test messages:', duplicates.length);
console.log('Current session ID:', currentSessionId || 'NOT SET');
console.log('');

if (duplicates.length > 0) {
  console.log('🔍 DUPLICATED MESSAGES:');
  duplicates.forEach(dup => {
    console.log(\`  ⚠️  \"\${dup.content}\" appears \${dup.count} times\`);
  });
  console.log('');
}

console.log('🎯 MESSAGE ATTRIBUTION ANALYSIS:');
testMessages.slice(0, 10).forEach((msg, i) => {
  const side = msg.isCurrentUser ? 'RIGHT (✅)' : 'LEFT (❌)';
  const attribution = msg.type === 'user' ? 'USER' : 'ASSISTANT';
  console.log(\`  [\${i+1}] \"\${msg.content}\" - Type: \${attribution}, Side: \${side}, Count: \${msg.count}\`);
});

if (testMessages.length > 10) {
  console.log(\`  ... and \${testMessages.length - 10} more test messages\`);
}

return {
  totalMessages: messages.length,
  testMessages: testMessages.length,
  currentUserMessages,
  wrongSideMessages,
  duplicates: duplicates.length,
  attributionWorking: currentUserMessages > 0 && wrongSideMessages === 0,
  sessionIdSet: !!currentSessionId
};
" --environment="browser"

echo ""
echo "📸 Taking comprehensive screenshot of final state..."
./jtag screenshot --querySelector="chat-widget" --filename="comprehensive-chat-test-results.png"

echo ""
echo "✅ Comprehensive chat send test completed!"
echo "📊 SCENARIOS TESTED:"
echo "   1. Sequential sends (original duplication pattern)"
echo "   2. Rapid fire sends (stress test)"
echo "   3. Variable message lengths" 
echo "   4. Enter key vs click button"
echo "   5. Special characters & edge cases"
echo "   6. Mixed timing patterns"
echo ""
echo "📸 Screenshot: comprehensive-chat-test-results.png"
echo "🔍 Check console output above for detailed analysis"
echo "🎯 Focus areas: duplication patterns, attribution logic, timing effects"