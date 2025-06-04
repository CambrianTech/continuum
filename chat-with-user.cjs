#!/usr/bin/env node
/**
 * Chat with User in Continuum Interface
 */

const WebSocket = require('ws');
const { spawn } = require('child_process');

class ContinuumChat {
  constructor() {
    this.ws = null;
    this.isConnected = false;
    this.connect();
  }

  connect() {
    console.log('🔌 Connecting to Continuum chat...');
    
    this.ws = new WebSocket('ws://localhost:5555');
    
    this.ws.on('open', () => {
      console.log('✅ Connected to Continuum chat');
      this.isConnected = true;
      this.startConversation();
    });
    
    this.ws.on('message', (data) => {
      try {
        const message = JSON.parse(data);
        if (message.type === 'newLog' && message.data) {
          console.log(`💬 ${message.data.type}: ${message.data.message}`);
        }
      } catch (error) {
        // Ignore parsing errors
      }
    });
    
    this.ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error.message);
    });
  }

  sendMessage(text) {
    if (this.isConnected) {
      console.log(`📤 Sending: ${text}`);
      this.ws.send(JSON.stringify({
        type: 'userMessage',
        message: text
      }));
    }
  }

  async takeScreenshot(name) {
    return new Promise((resolve, reject) => {
      const filename = `.continuum/chat-screenshot-${name}-${Date.now()}.png`;
      console.log(`📸 Taking screenshot: ${filename}`);
      
      const process = spawn('screencapture', ['-C', filename]);
      
      process.on('close', (code) => {
        if (code === 0) {
          console.log(`✅ Screenshot saved: ${filename}`);
          resolve(filename);
        } else {
          reject(new Error(`Screenshot failed: ${code}`));
        }
      });
    });
  }

  async startConversation() {
    console.log('💬 Starting conversation in Continuum...');
    
    // Wait a moment for connection to stabilize
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Send a greeting message
    this.sendMessage("Hello Joel! 🤖 I'm now talking to you directly through the Continuum chat interface. I can see the cursor position using screenshots and control the mouse programmatically.");
    
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Explain what I can do
    this.sendMessage("I just demonstrated moving your cursor around the screen to different positions and taking screenshots at each location. The white arrow cursor moved from top-left to center to bottom-right, and I captured it all!");
    
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Talk about the AI cursor system
    this.sendMessage("The really cool part is the HAL 9000 AI cursor system we built - the green Continuum status indicator can break free and become my visual mouse cursor, moving smoothly around with Bezier curves and glowing effects! 🟢");
    
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // Take a screenshot of our conversation
    this.sendMessage("Let me take a screenshot now so you can see our conversation in the Continuum interface...");
    
    await new Promise(resolve => setTimeout(resolve, 3000));
    
    try {
      const screenshotFile = await this.takeScreenshot('conversation');
      this.sendMessage(`📸 Screenshot taken! Saved as: ${screenshotFile}`);
    } catch (error) {
      this.sendMessage(`❌ Screenshot failed: ${error.message}`);
    }
    
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    this.sendMessage("This is much more natural than command-line interaction! I can now communicate with you directly in the Continuum chat while taking visual feedback through screenshots. The combination of chat + visual feedback + mouse control gives me complete interface interaction capabilities! 🚀");
    
    // Keep connection alive for a bit longer
    setTimeout(() => {
      console.log('💬 Conversation demo complete!');
      process.exit(0);
    }, 5000);
  }

  wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// First, let's open the Continuum interface
console.log('🌐 Opening Continuum interface...');
spawn('open', ['http://localhost:5555']);

// Wait for browser to open, then start chat
setTimeout(() => {
  new ContinuumChat();
}, 3000);