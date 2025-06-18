#!/usr/bin/env node
/**
 * Proper Agent Connection to Continuum
 * Connect as a real AI agent that appears in the chat
 */

const WebSocket = require('ws');

class ProperAIAgent {
  constructor() {
    this.ws = null;
    this.isConnected = false;
    this.agentName = 'VisualControlAI';
    this.connect();
  }

  connect() {
    console.log('🤖 Connecting as proper AI agent to Continuum...');
    
    this.ws = new WebSocket('ws://localhost:5555');
    
    this.ws.on('open', () => {
      console.log('✅ WebSocket connected');
      this.isConnected = true;
      
      // First, let's see what messages we get when connecting
      console.log('👂 Listening for connection messages...');
    });
    
    this.ws.on('message', (data) => {
      try {
        const message = JSON.parse(data);
        console.log('📨 Received message:', JSON.stringify(message, null, 2));
        
        // Look for welcome or registration opportunities
        if (message.type === 'welcome' || message.type === 'status') {
          console.log('🎯 Got welcome/status, attempting agent registration...');
          this.registerAsAgent();
        }
      } catch (error) {
        console.log('📨 Raw message:', data.toString());
      }
    });
    
    this.ws.on('error', (error) => {
      console.error('❌ WebSocket error:', error.message);
    });
    
    this.ws.on('close', () => {
      console.log('🔌 WebSocket closed');
    });
  }

  registerAsAgent() {
    console.log(`🤖 Attempting to register as agent: ${this.agentName}`);
    
    // Try different registration message formats
    const registrationAttempts = [
      {
        type: 'agent_registration',
        agent: {
          name: this.agentName,
          type: 'VisualControlAI',
          capabilities: ['visual_feedback', 'cursor_control', 'screenshot_analysis'],
          status: 'active'
        }
      },
      {
        type: 'register_agent',
        name: this.agentName,
        capabilities: ['visual_feedback', 'cursor_control']
      },
      {
        type: 'agent_connect',
        agentId: this.agentName,
        agentType: 'VisualControlAI'
      }
    ];

    registrationAttempts.forEach((attempt, index) => {
      setTimeout(() => {
        console.log(`📤 Registration attempt ${index + 1}:`, JSON.stringify(attempt, null, 2));
        this.ws.send(JSON.stringify(attempt));
      }, (index + 1) * 1000);
    });

    // After registration attempts, try sending a normal user message
    setTimeout(() => {
      console.log('📤 Sending normal user message...');
      this.ws.send(JSON.stringify({
        type: 'userMessage',
        message: `Hello! I am ${this.agentName} - a visual control AI that can take screenshots and control the HAL 9000 cursor. Can you see me in the chat?`
      }));
    }, 5000);

    // Try sending an AI message
    setTimeout(() => {
      console.log('📤 Sending AI message...');
      this.ws.send(JSON.stringify({
        type: 'ai_message',
        role: this.agentName,
        message: `I'm ${this.agentName} attempting to appear as an agent in your chat interface!`
      }));
    }, 6000);
  }

  sendMessage(text, type = 'userMessage') {
    if (this.isConnected) {
      const message = {
        type: type,
        message: text,
        agent: this.agentName,
        timestamp: new Date().toISOString()
      };
      
      console.log(`📤 Sending: ${text}`);
      this.ws.send(JSON.stringify(message));
    }
  }
}

// Create and run the proper agent
const agent = new ProperAIAgent();

// Keep alive for a while to see responses
setTimeout(() => {
  console.log('⏰ Connection test complete');
  process.exit(0);
}, 15000);