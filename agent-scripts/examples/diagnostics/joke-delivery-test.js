// Emergency diagnostic - why didn't the joke get through?
console.log('🚨 EMERGENCY DIAGNOSTIC - Joke Delivery Failure');
console.log('===============================================');

// 1. Check if browser console is visible to user
console.log('❓ Can you see this message in your browser console?');
console.log('💡 If not: Press F12 or Cmd+Option+I to open Developer Tools');

// 2. Test console methods
console.log('📝 Testing console.log...');
console.warn('⚠️ Testing console.warn...');
console.error('❌ Testing console.error...');

// 3. Check if chat integration works
if (typeof addMessage === 'function') {
  addMessage('DiagnosticBot', '🔧 Testing chat integration - can you see this in the chat?', 'ai', false, true);
  console.log('✅ Chat integration available - message sent to chat');
} else {
  console.log('❌ Chat integration not available - addMessage function missing');
}

// 4. Try system message
if (typeof addSystemMessage === 'function') {
  addSystemMessage('🛰️ System message test - diagnostic probe active');
  console.log('✅ System message integration available');
} else {
  console.log('❌ System message integration not available - addSystemMessage function missing');
}

// 5. Alert fallback (user will definitely see this)
alert('🚨 JOKE DELIVERY TEST: Why do programmers prefer dark mode? Because light attracts bugs! 🐛\n\nIf you see this alert, the probe connection works but console delivery failed.');

'diagnostic_complete'