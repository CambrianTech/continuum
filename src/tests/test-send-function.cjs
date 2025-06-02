#!/usr/bin/env node
/**
 * TEST SEND FUNCTION
 * 
 * Simulates what happens when user clicks send in the web interface
 */
const WebSocket = require('ws');

async function testSendFunction() {
  console.log('🧪 Testing web interface send functionality...');
  
  const ws = new WebSocket('ws://localhost:5559');
  
  ws.on('open', () => {
    console.log('✅ Connected to Simple AI');
    
    // Simulate what the web interface does when user types and clicks send
    const simulatedUserInput = {
      message: "I want to build a website",
      persona: "PlannerClaude"
    };
    
    console.log('🎭 Simulating user selecting PlannerClaude');
    console.log('💬 Simulating user typing: "I want to build a website"');
    console.log('🖱️  Simulating user clicking Send button...');
    
    // This is exactly what the send() function in the web interface does
    ws.send(JSON.stringify(simulatedUserInput));
  });
  
  ws.on('message', (data) => {
    const response = JSON.parse(data);
    console.log('📨 AI Response received:');
    console.log(`   Persona: ${response.persona}`);
    console.log(`   Message: ${response.response}`);
    
    // Test if this looks like what PlannerClaude should respond
    if (response.persona === 'PlannerClaude' && response.response.includes('plan')) {
      console.log('✅ PlannerClaude responded correctly with planning focus');
    } else {
      console.log('⚠️  Response might not match expected persona behavior');
    }
    
    ws.close();
    process.exit(0);
  });
  
  ws.on('error', (error) => {
    console.log('❌ WebSocket error:', error.message);
    process.exit(1);
  });
}

testSendFunction();