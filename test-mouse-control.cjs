#!/usr/bin/env node
/**
 * Test Mouse Control Script
 * Connects to Continuum and tests the mouse control commands
 */

const WebSocket = require('ws');

class MouseControlTester {
  constructor() {
    this.ws = null;
    this.isConnected = false;
    this.connect();
  }

  connect() {
    console.log('🔌 Connecting to Continuum WebSocket...');
    
    this.ws = new WebSocket('ws://localhost:5555');
    
    this.ws.on('open', () => {
      console.log('✅ Connected to Continuum WebSocket');
      this.isConnected = true;
      this.runTests();
    });
    
    this.ws.on('message', (data) => {
      try {
        const message = JSON.parse(data);
        if (message.type === 'result' || message.type === 'response') {
          console.log(`📨 Continuum: ${message.data || message.message}`);
        }
      } catch (error) {
        console.log(`📨 Raw: ${data.toString().substring(0, 100)}`);
      }
    });
    
    this.ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error.message);
    });
  }

  sendCommand(command) {
    if (this.isConnected) {
      console.log(`📤 Sending: ${command}`);
      this.ws.send(JSON.stringify({
        type: 'userMessage',
        message: command
      }));
    }
  }

  async runTests() {
    console.log('🧪 Starting mouse control tests...');
    
    // Test 1: Take a screenshot with cursor
    console.log('\n📸 Test 1: Taking screenshot with cursor visible');
    this.sendCommand('[CMD:SCREENSHOT] low 800x600');
    
    await this.wait(3000);
    
    // Test 2: Get current mouse position
    console.log('\n🖱️ Test 2: Getting current mouse position');
    this.sendCommand('[CMD:EXEC] cliclick p');
    
    await this.wait(2000);
    
    // Test 3: Move mouse to center of screen (natural movement)
    console.log('\n🎯 Test 3: Moving mouse to center with natural Bezier curve');
    this.sendCommand('[CMD:MOVE] 640 360 natural');
    
    await this.wait(3000);
    
    // Test 4: Take another screenshot
    console.log('\n📸 Test 4: Taking another screenshot to see cursor moved');
    this.sendCommand('[CMD:SCREENSHOT] low 800x600');
    
    await this.wait(3000);
    
    // Test 5: Click at current position
    console.log('\n🖱️ Test 5: Clicking at current position');
    this.sendCommand('[CMD:CLICK] 640 360 left natural');
    
    await this.wait(2000);
    
    // Test 6: Type some text
    console.log('\n⌨️ Test 6: Typing test message');
    this.sendCommand('[CMD:TYPE] Hello! AI mouse control is working!');
    
    await this.wait(3000);
    
    // Test 7: Final screenshot
    console.log('\n📸 Test 7: Final screenshot to verify interaction');
    this.sendCommand('[CMD:SCREENSHOT] low 800x600');
    
    console.log('\n✅ Mouse control tests completed!');
    console.log('📂 Check .continuum/ directory for screenshots');
    
    setTimeout(() => process.exit(0), 5000);
  }

  wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

new MouseControlTester();