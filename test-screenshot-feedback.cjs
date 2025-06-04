#!/usr/bin/env node
/**
 * Test Screenshot Feedback System
 * Demonstrates the glowing rectangle that appears when taking screenshots
 */

const WebSocket = require('ws');

class ScreenshotFeedbackDemo {
  constructor() {
    this.ws = null;
    this.isConnected = false;
  }

  async connect() {
    return new Promise((resolve, reject) => {
      console.log('🔌 Connecting to test screenshot feedback...');
      
      this.ws = new WebSocket('ws://localhost:5555');
      
      this.ws.on('open', () => {
        console.log('✅ Connected to Continuum');
        this.isConnected = true;
        resolve();
      });
      
      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data);
          if (message.type === 'result') {
            console.log(`📨 Response: ${message.data.result.substring(0, 80)}...`);
          }
        } catch (error) {
          // Ignore parsing errors
        }
      });
      
      this.ws.on('error', reject);
    });
  }

  sendMessage(text) {
    if (this.isConnected) {
      console.log(`📤 ${text}`);
      this.ws.send(JSON.stringify({
        type: 'userMessage',
        message: text
      }));
    }
  }

  async demo() {
    await this.connect();
    
    console.log('\n🎬 Screenshot Feedback Demo Starting...');
    
    this.sendMessage("I'm about to demonstrate the new screenshot feedback system! 📸");
    await this.wait(2000);
    
    this.sendMessage("When I take a screenshot, you should see a glowing green rectangle with HAL 9000 style effects appear and fade away smoothly.");
    await this.wait(3000);
    
    this.sendMessage("Watch your screen - here comes the first screenshot with visual feedback!");
    await this.wait(2000);
    
    // Trigger the visual feedback test
    this.sendMessage("triggerScreenshotFeedback()");
    await this.wait(3000);
    
    this.sendMessage("Did you see the glowing rectangle? It should have:");
    await this.wait(2000);
    
    this.sendMessage("• Started as a bright white flash 💥");
    await this.wait(1000);
    
    this.sendMessage("• Transitioned to bright green glow 🟢");  
    await this.wait(1000);
    
    this.sendMessage("• Faded to darker green and disappeared ✨");
    await this.wait(1000);
    
    this.sendMessage("• Plus 4 corner indicators that flash in sequence 📐");
    await this.wait(3000);
    
    this.sendMessage("Let me trigger it again so you can see the full effect!");
    await this.wait(2000);
    
    // Second demo
    this.sendMessage("triggerScreenshotFeedback()");
    await this.wait(4000);
    
    this.sendMessage("Perfect! The screenshot feedback system gives you immediate visual confirmation when I'm capturing the screen. No more wondering if a screenshot was taken! 🎯");
    
    console.log('\n✨ Screenshot feedback demo complete!');
  }

  wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// Run the demo
const demo = new ScreenshotFeedbackDemo();
demo.demo().then(() => {
  setTimeout(() => process.exit(0), 3000);
}).catch(console.error);