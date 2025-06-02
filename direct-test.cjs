#!/usr/bin/env node
/**
 * DIRECT TEST
 * 
 * Send multiple messages directly to test conversation
 */
const WebSocket = require('ws');

const testMessages = [
  { message: "Hello, how are you?", persona: "QuestionerClaude" },
  { message: "I want to build a React app", persona: "PlannerClaude" },
  { message: "Help me debug this code", persona: "ImplementerClaude" }
];

let messageIndex = 0;

function runDirectTest() {
  console.log('🧪 Running direct conversation test...');
  
  const ws = new WebSocket('ws://localhost:5559');
  
  ws.on('open', () => {
    console.log('✅ Connected to Simple AI');
    sendNextMessage();
  });
  
  ws.on('message', (data) => {
    const response = JSON.parse(data);
    const testMsg = testMessages[messageIndex - 1];
    
    console.log(`\n📨 Test ${messageIndex}:`);
    console.log(`   👤 User: "${testMsg.message}" (to ${testMsg.persona})`);
    console.log(`   🤖 AI: "${response.response}"`);
    
    if (messageIndex < testMessages.length) {
      setTimeout(sendNextMessage, 1000);
    } else {
      console.log('\n✅ All test messages completed');
      ws.close();
    }
  });
  
  ws.on('error', (error) => {
    console.log('❌ Error:', error.message);
  });
  
  function sendNextMessage() {
    if (messageIndex >= testMessages.length) return;
    
    const testMsg = testMessages[messageIndex];
    messageIndex++;
    
    console.log(`\n📤 Sending: "${testMsg.message}" to ${testMsg.persona}`);
    ws.send(JSON.stringify(testMsg));
  }
}

runDirectTest();