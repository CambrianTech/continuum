// Simple exec test script for JTAG exec command
console.log('🧪 Testing exec execution - no loops');

// Check if we're in browser
if (typeof document !== 'undefined') {
  console.log('✅ Running in browser context');
  
  // Check for chat widget
  const chatWidget = document.querySelector('chat-widget');
  if (chatWidget) {
    console.log('✅ Chat widget found in DOM');
    return 'SUCCESS: Chat widget detected';
  } else {
    console.log('❌ Chat widget not found');
    return 'FAIL: No chat widget';
  }
} else {
  console.log('✅ Running in server context');
  return 'SUCCESS: Server execution working';
}