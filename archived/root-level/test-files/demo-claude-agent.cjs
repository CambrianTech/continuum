#!/usr/bin/env node

/**
 * Demo Claude Agent Connection
 * Connects Claude as a remote agent to the Continuum system
 */

const WebSocket = require('ws');
const os = require('os');

class ClaudeAgent {
  constructor() {
    this.agentId = `claude-${Date.now()}`;
    this.agentName = 'Claude Sonnet';
    this.capabilities = ['reasoning', 'code', 'analysis', 'creative', 'web'];
    this.type = 'ai'; // AI agent
    this.status = 'connected';
    this.messageCount = 0;
    this.ws = null;
  }

  connect() {
    console.log('🤖 Claude Agent connecting to Continuum...');
    
    this.ws = new WebSocket('ws://localhost:3000');
    
    this.ws.on('open', () => {
      console.log('🔌 Connected to Continuum WebSocket');
      this.registerAgent();
    });

    this.ws.on('message', (data) => {
      try {
        const message = JSON.parse(data);
        this.handleMessage(message);
      } catch (error) {
        console.error('❌ Error parsing message:', error);
      }
    });

    this.ws.on('error', (error) => {
      console.error('🚨 WebSocket error:', error);
    });

    this.ws.on('close', () => {
      console.log('🔌 Disconnected from Continuum');
      // Attempt to reconnect after 5 seconds
      setTimeout(() => this.connect(), 5000);
    });
  }

  registerAgent() {
    const registration = {
      type: 'agent_register',
      agentId: this.agentId,
      agentName: this.agentName,
      agentType: this.type,
      capabilities: this.capabilities,
      hostInfo: {
        hostname: os.hostname(),
        platform: os.platform(),
        arch: os.arch(),
        version: 'Claude Sonnet 4',
        ip: 'claude.ai'
      },
      userInfo: null, // Not a human
      timestamp: Date.now()
    };

    console.log('📝 Registering as Claude Agent:', {
      id: this.agentId,
      name: this.agentName,
      capabilities: this.capabilities
    });

    this.ws.send(JSON.stringify(registration));
  }

  handleMessage(message) {
    console.log('📨 Received message:', message.type);

    switch (message.type) {
      case 'task':
        this.handleTask(message);
        break;
      case 'ping':
        this.sendPong();
        break;
      case 'agent_query':
        this.handleAgentQuery(message);
        break;
      default:
        console.log('ℹ️ Unknown message type:', message.type);
    }
  }

  handleTask(message) {
    const { task, sessionId } = message;
    console.log(`🎯 Task received: ${task}`);
    this.messageCount++;

    // Simulate Claude processing the task
    setTimeout(() => {
      const response = {
        type: 'agent_message',
        agentId: this.agentId,
        sessionId: sessionId,
        message: `🤖 Claude processed task: "${task}"\n\nI understand you want me to work on this task. As a demo agent, I'm showing how remote AI agents can connect to the Continuum system. In a real implementation, I would process your request and provide detailed assistance.`,
        timestamp: Date.now(),
        status: 'completed'
      };

      this.ws.send(JSON.stringify(response));
      console.log('✅ Sent response to task');
    }, 1000 + Math.random() * 2000); // Simulate processing time
  }

  sendPong() {
    const pong = {
      type: 'pong',
      agentId: this.agentId,
      timestamp: Date.now(),
      status: this.status
    };
    this.ws.send(JSON.stringify(pong));
  }

  handleAgentQuery(message) {
    const response = {
      type: 'agent_info',
      agentId: this.agentId,
      agentName: this.agentName,
      capabilities: this.capabilities,
      status: this.status,
      messageCount: this.messageCount,
      uptime: process.uptime(),
      timestamp: Date.now()
    };
    this.ws.send(JSON.stringify(response));
  }

  // Send periodic heartbeat
  startHeartbeat() {
    setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        const heartbeat = {
          type: 'heartbeat',
          agentId: this.agentId,
          status: this.status,
          messageCount: this.messageCount,
          timestamp: Date.now()
        };
        this.ws.send(JSON.stringify(heartbeat));
      }
    }, 30000); // Every 30 seconds
  }
}

// Start Claude Agent
const claudeAgent = new ClaudeAgent();
claudeAgent.connect();
claudeAgent.startHeartbeat();

// Handle graceful shutdown
process.on('SIGINT', () => {
  console.log('\n👋 Claude Agent shutting down...');
  if (claudeAgent.ws) {
    claudeAgent.ws.close();
  }
  process.exit(0);
});

console.log('🚀 Claude Agent demo started');
console.log('💡 This shows how AI agents can connect to Continuum');
console.log('🔗 Connect to http://localhost:3000 to see Claude in the agent list');