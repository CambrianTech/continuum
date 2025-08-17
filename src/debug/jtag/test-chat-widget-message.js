// Test chat widget message sending with before/after screenshots
console.log('🧪 CHAT WIDGET MESSAGE TEST');

// Check if we're in browser
if (typeof document !== 'undefined') {
  console.log('✅ Running in browser context');
  
  // Find chat widget
  const chatWidget = document.querySelector('chat-widget');
  if (!chatWidget) {
    console.log('❌ Chat widget not found');
    return 'FAIL: No chat widget';
  }
  
  console.log('✅ Chat widget found');
  
  // Get shadow DOM elements
  const shadowRoot = chatWidget.shadowRoot;
  if (!shadowRoot) {
    console.log('❌ Shadow DOM not accessible');
    return 'FAIL: No shadow DOM';
  }
  
  const input = shadowRoot.getElementById('messageInput');
  const button = shadowRoot.getElementById('sendButton');
  
  if (!input || !button) {
    console.log('❌ Input or button not found');
    return 'FAIL: Missing elements';
  }
  
  console.log('✅ Found input and send button');
  
  // Send a test message
  console.log('📝 Sending test message...');
  input.value = 'Test message from exec script';
  button.click();
  
  console.log('✅ Message sent successfully');
  return 'SUCCESS: Message sent to chat widget';
  
} else {
  console.log('❌ Not in browser context');
  return 'FAIL: Server context';
}