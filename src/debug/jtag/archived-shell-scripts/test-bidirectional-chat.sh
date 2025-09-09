#!/bin/bash

# Bidirectional Chat Test - Server & Browser Message Sending
# Tests the complete chat pipeline: browser→server AND server→browser
# Run after `npm start` to test full bidirectional communication

echo "🧪 Starting BIDIRECTIONAL chat test - Server + Browser messaging..."
echo "Make sure you ran 'npm start' first!"
echo ""

# Helper function to send from browser
send_browser_message() {
    local message="$1"
    local test_name="$2"
    
    echo "📱 $test_name: BROWSER sending \"$message\"..."
    ./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
const input = chatWidget.shadowRoot?.querySelector('.message-input');

if (input && chatWidget.sendMessage) {
  input.value = '$message';
  chatWidget.sendMessage();
  return 'BROWSER message sent: $message';
} else {
  return 'ERROR: Widget or input not found';
}
" --environment="browser"
    sleep 1
}

# Helper function to send from server
send_server_message() {
    local message="$1"
    local test_name="$2"
    
    echo "🖥️  $test_name: SERVER sending \"$message\"..."
    ./jtag chat/send-message --roomId="general" --content="$message" --senderType="server"
    sleep 1
}

# Clear chat for clean test
echo "🧹 Clearing chat history for clean test..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
if (chatWidget) {
  chatWidget.messages = [];
  chatWidget.renderWidget();
  return 'Chat cleared for bidirectional test';
}
return 'Could not clear chat';
" --environment="browser"

sleep 2

echo ""
echo "🔥 BIDIRECTIONAL TEST SEQUENCE:"
echo "   Testing alternating browser/server message sending"
echo ""

# Test 1: Browser starts
send_browser_message "BROWSER-MSG-1: Starting bidirectional test" "Test 1A"

# Test 2: Server responds  
send_server_message "SERVER-MSG-1: Server received and responding" "Test 1B"

# Test 3: Browser continues conversation
send_browser_message "BROWSER-MSG-2: Browser acknowledges server response" "Test 2A"

# Test 4: Server sends multiple quickly
send_server_message "SERVER-MSG-2: Rapid server message 1" "Test 2B"
send_server_message "SERVER-MSG-3: Rapid server message 2" "Test 2C"

# Test 5: Browser responds to rapid messages
send_browser_message "BROWSER-MSG-3: Browser handling rapid server messages" "Test 3A"

# Test 6: Mixed timing test
send_browser_message "BROWSER-MSG-4: Testing mixed timing" "Test 4A"
sleep 0.5
send_server_message "SERVER-MSG-4: Server with short delay" "Test 4B"
sleep 2
send_browser_message "BROWSER-MSG-5: Browser with longer delay" "Test 4C"

echo ""
echo "⏳ Waiting for all bidirectional messages to process..."
sleep 3

# Comprehensive bidirectional analysis
echo "📊 Analyzing BIDIRECTIONAL chat state..."
./jtag exec --code="
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');

if (!chatWidget) return { error: 'Chat widget not found' };

const messages = chatWidget.messages || [];
const currentSessionId = chatWidget.currentSessionId;

// Separate browser vs server messages
const browserMessages = messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') && (currentSessionId && msg.senderId === currentSessionId)
);

const serverMessages = messages.filter(msg => 
  msg.content.includes('SERVER-MSG') && (!currentSessionId || msg.senderId !== currentSessionId)
);

// Count duplicates by type
const browserDuplicates = {};
const serverDuplicates = {};

messages.forEach(msg => {
  if (msg.content.includes('BROWSER-MSG')) {
    browserDuplicates[msg.content] = (browserDuplicates[msg.content] || 0) + 1;
  } else if (msg.content.includes('SERVER-MSG')) {
    serverDuplicates[msg.content] = (serverDuplicates[msg.content] || 0) + 1;
  }
});

const browserDups = Object.entries(browserDuplicates).filter(([_, count]) => count > 1);
const serverDups = Object.entries(serverDuplicates).filter(([_, count]) => count > 1);

// Attribution analysis
const wrongSideBrowser = messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') && msg.type !== 'user'
).length;

const wrongSideServer = messages.filter(msg => 
  msg.content.includes('SERVER-MSG') && msg.type !== 'assistant'  
).length;

console.log('');
console.log('🏁 BIDIRECTIONAL TEST RESULTS:');
console.log('==============================');
console.log('Total messages:', messages.length);
console.log('Browser messages (should be on RIGHT):', browserMessages.length);
console.log('Server messages (should be on LEFT):', serverMessages.length);
console.log('Current session ID:', currentSessionId || 'NOT SET');
console.log('');

console.log('🎯 ATTRIBUTION ANALYSIS:');
console.log('Browser messages on wrong side:', wrongSideBrowser);
console.log('Server messages on wrong side:', wrongSideServer);
console.log('Attribution working:', wrongSideBrowser === 0 && wrongSideServer === 0);
console.log('');

console.log('🔍 DUPLICATION ANALYSIS:');
console.log('Browser message duplicates:', browserDups.length);
if (browserDups.length > 0) {
  browserDups.forEach(([content, count]) => {
    console.log(\`  ⚠️  BROWSER: \"\${content.slice(0, 40)}...\" appears \${count} times\`);
  });
}
console.log('Server message duplicates:', serverDups.length);
if (serverDups.length > 0) {
  serverDups.forEach(([content, count]) => {
    console.log(\`  ⚠️  SERVER: \"\${content.slice(0, 40)}...\" appears \${count} times\`);
  });
}

console.log('');
console.log('💬 CONVERSATION FLOW:');
messages.filter(msg => 
  msg.content.includes('BROWSER-MSG') || msg.content.includes('SERVER-MSG')
).slice(0, 12).forEach((msg, i) => {
  const side = (currentSessionId && msg.senderId === currentSessionId) ? 'RIGHT' : 'LEFT';
  const type = msg.type === 'user' ? 'USER' : 'ASSISTANT';
  const source = msg.content.includes('BROWSER-MSG') ? 'BROWSER' : 'SERVER';
  console.log(\`  [\${i+1}] \${source}: \"\${msg.content.slice(0, 50)}...\" - Type: \${type}, Side: \${side}\`);
});

return {
  totalMessages: messages.length,
  browserMessages: browserMessages.length,
  serverMessages: serverMessages.length,
  browserDuplicates: browserDups.length,
  serverDuplicates: serverDups.length,
  attributionWorking: wrongSideBrowser === 0 && wrongSideServer === 0,
  sessionIdSet: !!currentSessionId
};
" --environment="browser"

echo ""
echo "📸 Taking bidirectional test screenshot..."
./jtag screenshot --querySelector="chat-widget" --filename="bidirectional-chat-test.png"

echo ""
echo "✅ Bidirectional chat test completed!"
echo "📊 TESTED SCENARIOS:"
echo "   • Browser→Server messaging"  
echo "   • Server→Browser messaging"
echo "   • Rapid sequential server messages"
echo "   • Mixed timing patterns"
echo "   • Attribution for both directions"
echo "   • Duplication detection for both sides"
echo ""
echo "📸 Screenshot: bidirectional-chat-test.png"
echo "🔍 Check console output above for detailed bidirectional analysis"
echo "🎯 Focus: Server messages should appear on LEFT, Browser messages on RIGHT"