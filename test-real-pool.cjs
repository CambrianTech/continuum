#!/usr/bin/env node
/**
 * TEST REAL POOL
 * 
 * Tests if the Real Claude Pool actually works
 * Sends real messages and checks for real responses
 */

const WebSocket = require('ws');

class RealPoolTest {
  constructor() {
    this.ws = null;
    this.responses = [];
    this.connected = false;
    this.testStartTime = Date.now();
  }

  async testRealPool() {
    console.log('🧪 TESTING REAL CLAUDE POOL');
    console.log('===========================');
    console.log('🔌 Connecting to ws://localhost:5555...');
    
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket('ws://localhost:5555');
      
      this.ws.on('open', () => {
        console.log('✅ Connected to Real Claude Pool');
        this.connected = true;
        this.runTest().then(resolve).catch(reject);
      });
      
      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data);
          const timestamp = new Date().toLocaleTimeString();
          console.log(`📨 [${timestamp}] Received: ${message.type}`);
          console.log(`   Data: ${JSON.stringify(message.data, null, 2)}`);
          
          this.responses.push({
            timestamp: Date.now(),
            type: message.type,
            data: message.data
          });
          
        } catch (error) {
          console.log(`📨 Raw message: ${data}`);
        }
      });
      
      this.ws.on('error', (error) => {
        console.error('❌ WebSocket error:', error.message);
        reject(error);
      });
      
      this.ws.on('close', () => {
        console.log('🔌 WebSocket connection closed');
      });
      
      // Timeout after 45 seconds
      setTimeout(() => {
        if (this.responses.length === 0) {
          reject(new Error('No responses received within 45 seconds'));
        }
      }, 45000);
    });
  }

  async runTest() {
    console.log('🚀 Starting real pool test...');
    
    // Wait for initial status
    await this.waitForMessage('pool_status', 5000);
    console.log('✅ Received pool status');
    
    // Send a test message that should trigger a Claude instance
    console.log('📤 Sending test message...');
    const testMessage = 'Hello, I need help planning a software project';
    
    this.ws.send(JSON.stringify({
      type: 'user_message',
      content: testMessage
    }));
    
    console.log(`   Sent: "${testMessage}"`);
    
    // Wait for routing message
    await this.waitForMessage('routing', 5000);
    console.log('✅ Message was routed');
    
    // Wait for instance status (should confirm message was sent to Claude)
    await this.waitForMessage('instance_status', 10000);
    console.log('✅ Instance received message');
    
    // Now wait a bit and check if we get any actual Claude responses
    console.log('⏳ Waiting for Claude to respond (15 seconds)...');
    
    const responseCheckStart = Date.now();
    let claudeResponded = false;
    
    // Check for 15 seconds if Claude produces any output
    while (Date.now() - responseCheckStart < 15000) {
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Look for any signs that Claude is actually working
      const recentResponses = this.responses.filter(r => 
        r.timestamp > responseCheckStart - 1000
      );
      
      if (recentResponses.length > 0) {
        console.log('📨 Claude activity detected!');
        claudeResponded = true;
        break;
      }
    }
    
    // Report results
    this.reportResults(claudeResponded);
  }

  async waitForMessage(messageType, timeoutMs) {
    return new Promise((resolve, reject) => {
      const startTime = Date.now();
      
      const checkMessages = () => {
        const message = this.responses.find(r => r.type === messageType);
        if (message) {
          resolve(message);
          return;
        }
        
        if (Date.now() - startTime > timeoutMs) {
          reject(new Error(`Timeout waiting for message type: ${messageType}`));
          return;
        }
        
        setTimeout(checkMessages, 100);
      };
      
      checkMessages();
    });
  }

  reportResults(claudeResponded) {
    console.log('');
    console.log('📊 REAL POOL TEST RESULTS');
    console.log('=========================');
    
    console.log(`📈 Total messages received: ${this.responses.length}`);
    console.log(`⏱️  Test duration: ${Date.now() - this.testStartTime}ms`);
    
    // Analyze message types
    const messageTypes = {};
    this.responses.forEach(r => {
      messageTypes[r.type] = (messageTypes[r.type] || 0) + 1;
    });
    
    console.log('📋 Message types received:');
    Object.entries(messageTypes).forEach(([type, count]) => {
      console.log(`   ${type}: ${count}`);
    });
    
    if (claudeResponded) {
      console.log('');
      console.log('🎉 SUCCESS: Claude instances are responding!');
      console.log('✅ Real Claude Pool is working');
    } else {
      console.log('');
      console.log('❌ PROBLEM: Claude instances are not responding');
      console.log('🔧 Possible issues:');
      console.log('   - Claude CLI not working properly');
      console.log('   - Auto-wrapper not capturing output');
      console.log('   - Claude instances not actually launching');
      console.log('   - Communication pipeline broken');
    }
    
    // Check specific issues
    const hasRouting = this.responses.some(r => r.type === 'routing');
    const hasInstanceStatus = this.responses.some(r => r.type === 'instance_status');
    
    if (!hasRouting) {
      console.log('❌ Message routing failed');
    }
    
    if (!hasInstanceStatus) {
      console.log('❌ Instance communication failed');
    }
    
    if (hasRouting && hasInstanceStatus && !claudeResponded) {
      console.log('❌ Claude instances launched but not responding');
      console.log('💡 This suggests the auto-wrapper is not capturing Claude output');
    }
    
    // Close WebSocket
    if (this.ws) {
      this.ws.close();
    }
  }
}

// Run the test
const tester = new RealPoolTest();
tester.testRealPool().catch(error => {
  console.error('💥 Test failed:', error.message);
  process.exit(1);
});