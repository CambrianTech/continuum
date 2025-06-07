console.log('🔍 LIVE BROWSER INVESTIGATION');
console.log('============================');

// Check what chat functions actually exist
console.log('Chat functions available:');
console.log('- addMessage:', typeof addMessage);
console.log('- addSystemMessage:', typeof addSystemMessage);
console.log('- chat element:', document.getElementById('chat') ? 'EXISTS' : 'MISSING');

// Check current page state
console.log('Page state:');
console.log('- URL:', window.location.href);
console.log('- Title:', document.title);
console.log('- Visible elements:', document.querySelectorAll('.message, .chat-message, #chat-area').length);

// Find ANY way to display a message visibly
const possibleChatElements = [
  '#chat', '#chat-area', '.chat-container', '.messages',
  '#message-list', '.conversation', '#chat-messages'
];

console.log('Searching for chat interfaces...');
possibleChatElements.forEach(selector => {
  const el = document.querySelector(selector);
  if (el) {
    console.log('✅ Found potential chat:', selector, el);
    // Try to inject a test message directly
    const testMsg = document.createElement('div');
    testMsg.innerHTML = '🤣 TEST JOKE: Why do programmers prefer dark mode? Because light attracts bugs! 🐛';
    testMsg.style.cssText = 'background: #2a2a2a; color: #00ff41; padding: 10px; margin: 5px; border-radius: 5px;';
    el.appendChild(testMsg);
    console.log('✅ Injected test joke into:', selector);
  }
});

'investigation_complete'