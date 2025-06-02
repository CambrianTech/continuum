#!/usr/bin/env node
/**
 * TEST SIMPLE AI
 * 
 * Tests if the simple AI WebSocket actually works
 */
const WebSocket = require('ws');

async function testSimpleAI() {
  console.log('🧪 Testing Simple AI WebSocket connection...');
  
  try {
    const ws = new WebSocket('ws://localhost:5559');
    
    ws.on('open', () => {
      console.log('✅ WebSocket connected');
      
      // Test sending a message
      const testMessage = {
        message: "What is 2 + 2?",
        persona: "QuestionerClaude"
      };
      
      console.log('📨 Sending test message:', testMessage);
      ws.send(JSON.stringify(testMessage));
    });
    
    ws.on('message', (data) => {
      try {
        const response = JSON.parse(data);
        console.log('📨 Received response:', response);
        
        if (response.response && response.persona) {
          console.log('✅ Simple AI is working correctly!');
          console.log(`🎭 Persona: ${response.persona}`);
          console.log(`💬 Response: ${response.response}`);
        } else {
          console.log('⚠️  Unexpected response format');
        }
        
        ws.close();
        process.exit(0);
      } catch (error) {
        console.log('❌ Error parsing response:', error.message);
        ws.close();
        process.exit(1);
      }
    });
    
    ws.on('error', (error) => {
      console.log('❌ WebSocket error:', error.message);
      process.exit(1);
    });
    
    ws.on('close', () => {
      console.log('🔌 WebSocket connection closed');
    });
    
  } catch (error) {
    console.log('❌ Test failed:', error.message);
    process.exit(1);
  }
}

testSimpleAI();