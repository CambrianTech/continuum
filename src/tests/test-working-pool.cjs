#!/usr/bin/env node
/**
 * TEST WORKING POOL
 * 
 * Tests if the Working Pool actually responds with real Claude
 */

const WebSocket = require('ws');

class WorkingPoolTest {
  constructor() {
    this.responses = [];
    this.claudeResponses = [];
  }

  async test() {
    console.log('🧪 TESTING WORKING POOL');
    console.log('=======================');
    
    return new Promise((resolve, reject) => {
      const ws = new WebSocket('ws://localhost:5555');
      
      ws.on('open', () => {
        console.log('✅ Connected to Working Pool');
        
        // Wait for pool status, then send test message
        setTimeout(() => {
          console.log('📤 Sending test message...');
          ws.send(JSON.stringify({
            type: 'user_message',
            content: 'What is 7 times 8?'
          }));
        }, 1000);
      });
      
      ws.on('message', (data) => {
        try {
          const message = JSON.parse(data);
          console.log(`📨 Received: ${message.type}`);
          
          this.responses.push(message);
          
          if (message.type === 'claude_response') {
            const response = message.data.response;
            console.log(`🤖 Claude responded: "${response}"`);
            
            this.claudeResponses.push(response);
            
            // Check if it's a real mathematical response
            if (response.includes('56')) {
              console.log('✅ CORRECT ANSWER! Claude is working!');
            } else if (response.match(/\\d+/)) {
              console.log('⚠️  Got a number but not the right one');
            } else {
              console.log('❓ Response doesn\'t contain expected answer');
            }
            
            ws.close();
            resolve(true);
          }
          
        } catch (error) {
          console.log(`📨 Raw: ${data}`);
        }
      });
      
      ws.on('error', (error) => {
        console.error('❌ WebSocket error:', error.message);
        reject(error);
      });
      
      ws.on('close', () => {
        console.log('🔌 Connection closed');
        
        if (this.claudeResponses.length === 0) {
          console.log('❌ No Claude responses received');
          resolve(false);
        }
      });
      
      // Timeout after 30 seconds
      setTimeout(() => {
        if (this.claudeResponses.length === 0) {
          console.log('⏰ Test timed out - no Claude responses');
          ws.close();
          resolve(false);
        }
      }, 30000);
    });
  }
}

// Run the test
const tester = new WorkingPoolTest();
tester.test().then(success => {
  if (success) {
    console.log('\\n🎉 WORKING POOL IS FUNCTIONAL!');
    console.log('✅ Real Claude responses confirmed');
  } else {
    console.log('\\n💥 Working Pool test failed');
    console.log('❌ No real Claude responses detected');
  }
}).catch(error => {
  console.error('💥 Test error:', error.message);
});