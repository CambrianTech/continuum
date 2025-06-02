// TEST WEB INTERFACE
// Run this in browser console at http://localhost:5559

console.log('🧪 Testing Simple AI Web Interface...');

// Test 1: Check if elements exist
const chat = document.getElementById('chat');
const persona = document.getElementById('persona');
const message = document.getElementById('message');

if (!chat || !persona || !message) {
  console.error('❌ Missing required elements');
  console.log('chat:', !!chat, 'persona:', !!persona, 'message:', !!message);
} else {
  console.log('✅ All UI elements found');
}

// Test 2: Check WebSocket connection
let wsConnected = false;
const testWs = new WebSocket('ws://localhost:5559');

testWs.onopen = () => {
  console.log('✅ WebSocket connected successfully');
  wsConnected = true;
  
  // Test 3: Send a message
  const testMsg = {
    message: "Hello test",
    persona: "QuestionerClaude"
  };
  
  console.log('📨 Sending test message:', testMsg);
  testWs.send(JSON.stringify(testMsg));
};

testWs.onmessage = (e) => {
  console.log('📨 Received response:', JSON.parse(e.data));
  console.log('✅ Message exchange working!');
  testWs.close();
};

testWs.onerror = (error) => {
  console.error('❌ WebSocket error:', error);
};

// Test 4: Check if send function exists
setTimeout(() => {
  if (typeof send === 'function') {
    console.log('✅ send() function exists');
  } else {
    console.error('❌ send() function missing');
  }
  
  if (typeof addMessage === 'function') {
    console.log('✅ addMessage() function exists');
  } else {
    console.error('❌ addMessage() function missing');
  }
}, 1000);