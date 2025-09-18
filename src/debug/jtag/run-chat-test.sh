#!/bin/bash

# Repeatable Chat Issues Test
# Run this after `npm start` to test chat functionality

echo "🧪 Starting repeatable chat issues test..."
echo "Make sure you ran 'npm start' first!"
echo ""

# Test 1: Send first message
echo "📝 Test 1: Sending FIRST message..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = 'FIRST MESSAGE - should not be doubled';
  chatWidget.sendMessage();
  return 'First message sent';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"

echo ""
sleep 2

# Test 2: Send second message  
echo "📝 Test 2: Sending SECOND message..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = 'SECOND MESSAGE - check if doubled';
  chatWidget.sendMessage();
  return 'Second message sent';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"

echo ""
sleep 2

# Test 3: Send third message
echo "📝 Test 3: Sending THIRD message..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = 'THIRD MESSAGE - doubling pattern test';
  chatWidget.sendMessage();  
  return 'Third message sent';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"

echo ""
echo "⏳ Waiting for messages to process..."
sleep 3

# Analyze results
echo "📊 Analyzing chat state..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

if (!chatWidget) return { error: 'Chat widget not found' };

const messages = chatWidget.messages || [];
const currentSessionId = chatWidget.currentSessionId;

// Count messages by content to detect duplicates  
const messageCounts = {};
messages.forEach(msg => {
  const content = typeof msg.content === 'string' ? msg.content : msg.content?.text || '';
  messageCounts[content] = (messageCounts[content] || 0) + 1;
});

// Find our test messages
const testMessages = messages.filter(msg => {
  const content = typeof msg.content === 'string' ? msg.content : msg.content?.text || '';
  return content.includes('FIRST MESSAGE') ||
         content.includes('SECOND MESSAGE') ||
         content.includes('THIRD MESSAGE');
});

const currentUserMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId
).length;

const wrongSideMessages = messages.filter(msg => 
  currentSessionId && msg.senderId === currentSessionId && msg.type !== 'user'
).length;

console.log('🏁 TEST RESULTS:');
console.log('================');
console.log('Total messages:', messages.length);
console.log('Current user messages:', currentUserMessages);  
console.log('Messages on wrong side:', wrongSideMessages);
console.log('Attribution working:', currentUserMessages > 0 && wrongSideMessages === 0);
console.log('');
console.log('🔍 DOUBLING PATTERN:');

// Check each test message
const testContents = ['FIRST MESSAGE', 'SECOND MESSAGE', 'THIRD MESSAGE'];
testContents.forEach((content, i) => {
  const matchingMsgs = messages.filter(msg => {
    const msgContent = typeof msg.content === 'string' ? msg.content : msg.content?.text || '';
    return msgContent.includes(content);
  });
  console.log(\`Message \${i+1}: \"\${content}...\" - appears \${matchingMsgs.length} times \${matchingMsgs.length > 1 ? '⚠️ DOUBLED' : '✅'}\`);
  
  if (matchingMsgs.length > 0) {
    matchingMsgs.forEach((msg, j) => {
      console.log(\`  [\${j+1}] type: \${msg.type}, senderId: \${msg.senderId === currentSessionId ? 'CURRENT_USER' : 'OTHER'}\`);
    });
  }
});

return 'Analysis complete - check console output above';
" --environment="browser"

echo ""
echo "📸 Taking screenshot of final state..."
./jtag screenshot --querySelector="chat-widget" --filename="chat-test-results.png"

echo ""
echo "✅ Repeatable test completed!"
echo "📸 Screenshot saved as: chat-test-results.png"
echo "🔍 Check the console output above for detailed analysis"