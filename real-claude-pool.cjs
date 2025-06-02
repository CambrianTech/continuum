#!/usr/bin/env node
/**
 * REAL CLAUDE POOL
 * 
 * Creates a pool of actual Claude CLI instances
 * Each instance runs autonomously with the auto-wrapper
 * Provides web interface to coordinate between real Claude instances
 */

const ClaudeAutoWrapper = require('./claude-auto-wrapper.cjs');
const http = require('http');
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');

class RealClaudePool {
  constructor() {
    this.instances = new Map();
    this.connectedUsers = new Set();
    this.messageQueue = [];
    this.projectRoot = process.cwd();
    this.poolDir = path.join(this.projectRoot, '.real-claude-pool');
    
    console.log('🏊 REAL CLAUDE POOL');
    console.log('==================');
    console.log('🤖 Manages real Claude CLI instances');
    console.log('🔄 Auto-responds to Claude prompts');
    console.log('🌐 Web interface for coordination');
    console.log('');
  }

  async createPool() {
    console.log('🏗️ Creating real Claude pool...');
    
    // Setup pool directory
    if (!fs.existsSync(this.poolDir)) {
      fs.mkdirSync(this.poolDir, { recursive: true });
    }
    
    // Define Claude instances with different roles
    const instanceConfigs = [
      {
        name: 'QuestionerClaude',
        role: 'Ask clarifying questions and gather requirements',
        prompt: `You are QuestionerClaude. Your role is to ask clarifying questions when users make requests. Always ask follow-up questions to better understand what the user needs. Be specific and helpful.`
      },
      {
        name: 'PlannerClaude', 
        role: 'Create plans and break down tasks',
        prompt: `You are PlannerClaude. Your role is to take user requests and create detailed plans. Break down complex tasks into specific steps. Always ask about priorities and deadlines.`
      },
      {
        name: 'ImplementerClaude',
        role: 'Execute tasks and implement solutions', 
        prompt: `You are ImplementerClaude. Your role is to take plans and implement them. You can write code, create files, and execute tasks. Always confirm before making significant changes.`
      }
    ];
    
    // Create Claude instances
    for (const config of instanceConfigs) {
      console.log(`🤖 Creating ${config.name}...`);
      
      const wrapper = new ClaudeAutoWrapper(config.name.toLowerCase());
      
      // Store instance info
      this.instances.set(config.name, {
        wrapper: wrapper,
        config: config,
        status: 'starting',
        lastActivity: new Date().toISOString(),
        messageHistory: []
      });
      
      // Don't auto-launch yet - wait for first message
      console.log(`✅ ${config.name} wrapper ready`);
    }
    
    console.log(`🎯 Pool created with ${this.instances.size} Claude instances`);
  }

  async launchInstance(instanceName) {
    const instance = this.instances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }
    
    if (instance.status === 'running') {
      console.log(`⚠️  ${instanceName} already running`);
      return;
    }
    
    console.log(`🚀 Launching ${instanceName}...`);
    
    try {
      await instance.wrapper.launchClaude(instance.config.prompt);
      instance.status = 'running';
      instance.lastActivity = new Date().toISOString();
      
      console.log(`✅ ${instanceName} launched successfully`);
      
    } catch (error) {
      console.error(`❌ Failed to launch ${instanceName}: ${error.message}`);
      instance.status = 'failed';
    }
  }

  async sendToInstance(instanceName, message) {
    const instance = this.instances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }
    
    // Launch instance if not running
    if (instance.status !== 'running') {
      await this.launchInstance(instanceName);
    }
    
    console.log(`📨 Sending to ${instanceName}: "${message}"`);
    
    // Send message to Claude
    instance.wrapper.sendMessage(message);
    
    // Log the message
    instance.messageHistory.push({
      timestamp: new Date().toISOString(),
      type: 'sent',
      content: message
    });
    
    instance.lastActivity = new Date().toISOString();
  }

  selectInstanceForMessage(message) {
    const lower = message.toLowerCase();
    
    if (lower.includes('question') || lower.includes('clarify') || lower.includes('understand')) {
      return 'QuestionerClaude';
    } else if (lower.includes('plan') || lower.includes('organize') || lower.includes('steps')) {
      return 'PlannerClaude';
    } else if (lower.includes('implement') || lower.includes('build') || lower.includes('create')) {
      return 'ImplementerClaude';
    }
    
    // Default to QuestionerClaude for unclear requests
    return 'QuestionerClaude';
  }

  async setupWebInterface() {
    console.log('🌐 Setting up web interface...');
    
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generatePoolUI());
      } else {
        res.writeHead(404);
        res.end('Not Found');
      }
    });

    this.webSocket = new WebSocket.Server({ server });
    
    this.webSocket.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('👤 User connected to Real Claude Pool');
      
      // Send pool status
      ws.send(JSON.stringify({
        type: 'pool_status',
        data: {
          message: '🏊 Real Claude Pool ready',
          instances: Array.from(this.instances.entries()).map(([name, instance]) => ({
            name: name,
            status: instance.status,
            role: instance.config.role,
            lastActivity: instance.lastActivity,
            messageCount: instance.messageHistory.length
          }))
        }
      }));
      
      ws.on('message', (message) => {
        try {
          const data = JSON.parse(message);
          this.handleUserMessage(data, ws);
        } catch (error) {
          ws.send(JSON.stringify({ type: 'error', data: 'Parse error' }));
        }
      });
      
      ws.on('close', () => {
        this.connectedUsers.delete(ws);
        console.log('👤 User disconnected');
      });
    });

    server.listen(5555, () => {
      console.log('🌐 Real Claude Pool interface ready at http://localhost:5555');
    });
  }

  async handleUserMessage(data, ws) {
    const message = data.content;
    console.log(`📨 User message: "${message}"`);
    
    // Select appropriate Claude instance
    const targetInstance = this.selectInstanceForMessage(message);
    
    ws.send(JSON.stringify({
      type: 'routing',
      data: `🎯 Routing to ${targetInstance} (real Claude CLI instance)`
    }));
    
    try {
      // Send to selected Claude instance
      await this.sendToInstance(targetInstance, message);
      
      ws.send(JSON.stringify({
        type: 'instance_status',
        data: `✅ Message sent to real ${targetInstance}`
      }));
      
    } catch (error) {
      console.error(`❌ Failed to send message: ${error.message}`);
      
      ws.send(JSON.stringify({
        type: 'error',
        data: `Failed to send to ${targetInstance}: ${error.message}`
      }));
    }
  }

  generatePoolUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Real Claude Pool</title>
    <style>
        body { 
            background: linear-gradient(135deg, #0a1a0a 0%, #1a2a1a 50%, #2a3a2a 100%); 
            color: #00ff88; 
            font-family: 'Courier New', monospace; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            border: 2px solid #00ff88; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(0, 255, 136, 0.1);
        }
        .instances {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 15px;
            margin: 20px 0;
        }
        .instance {
            border: 1px solid #00ff88;
            padding: 15px;
            background: rgba(0, 255, 136, 0.05);
        }
        .running { border-color: #00ff00; }
        .starting { border-color: #ffff00; }
        .failed { border-color: #ff0000; }
        .chat { 
            border: 1px solid #00ff88; 
            padding: 20px; 
            height: 300px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(0, 255, 136, 0.02);
        }
        .input-area {
            display: flex;
            gap: 10px;
            margin: 20px 0;
        }
        .input { 
            flex: 1;
            background: rgba(0, 255, 136, 0.1); 
            border: 1px solid #00ff88; 
            color: #00ff88; 
            padding: 15px; 
            font-family: inherit;
        }
        .button { 
            background: rgba(0, 255, 136, 0.2); 
            border: 1px solid #00ff88; 
            color: #00ff88; 
            padding: 15px 25px; 
            cursor: pointer;
        }
        .message { 
            margin: 10px 0; 
            padding: 10px; 
            border-left: 3px solid #00ff88; 
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🏊 REAL CLAUDE POOL</h1>
        <p>Connected to actual Claude CLI instances with auto-response</p>
    </div>
    
    <div class="instances" id="instances">
        <!-- Instance status will be populated here -->
    </div>
    
    <div class="chat" id="chat">
        <div class="message">🏊 Real Claude Pool ready - your messages go to actual Claude CLI instances!</div>
    </div>
    
    <div class="input-area">
        <input type="text" id="messageInput" class="input" 
               placeholder="Send message to real Claude instances..." 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND TO POOL</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        
        ws.onopen = function() {
            addMessage('🟢 Connected to Real Claude Pool');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'pool_status') {
                addMessage(data.data.message);
                updateInstances(data.data.instances);
            } else if (data.type === 'routing') {
                addMessage(data.data);
            } else if (data.type === 'instance_status') {
                addMessage('✅ ' + data.data);
            } else if (data.type === 'error') {
                addMessage('❌ ' + data.data);
            }
        };
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            
            if (message) {
                addMessage('👤 ' + message);
                
                ws.send(JSON.stringify({
                    type: 'user_message',
                    content: message
                }));
                
                input.value = '';
            }
        }
        
        function addMessage(text) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message';
            messageDiv.innerHTML = new Date().toLocaleTimeString() + ' - ' + text;
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function updateInstances(instances) {
            const container = document.getElementById('instances');
            container.innerHTML = '';
            
            instances.forEach(instance => {
                const div = document.createElement('div');
                div.className = 'instance ' + instance.status;
                div.innerHTML = \`
                    <h3>\${instance.name}</h3>
                    <div>Status: \${instance.status}</div>
                    <div>Role: \${instance.role}</div>
                    <div>Messages: \${instance.messageCount}</div>
                    <div>Last: \${new Date(instance.lastActivity).toLocaleTimeString()}</div>
                \`;
                container.appendChild(div);
            });
        }
    </script>
</body>
</html>`;
  }

  async shutdown() {
    console.log('🛑 Shutting down Real Claude Pool...');
    
    for (const [name, instance] of this.instances) {
      console.log(`🔄 Terminating ${name}...`);
      await instance.wrapper.terminate();
    }
    
    console.log('✅ All Claude instances terminated');
  }

  async start() {
    console.log('🚀 Starting Real Claude Pool...');
    
    await this.createPool();
    await this.setupWebInterface();
    
    console.log('🎉 Real Claude Pool is running!');
    console.log('🌐 Access at http://localhost:5555');
    console.log('🤖 Claude instances will launch when first messaged');
    
    // Handle graceful shutdown
    process.on('SIGINT', async () => {
      console.log('\\n🛑 Received SIGINT, shutting down...');
      await this.shutdown();
      process.exit(0);
    });
  }
}

// Start the pool
if (require.main === module) {
  const pool = new RealClaudePool();
  pool.start().catch(error => {
    console.error('💥 Failed to start pool:', error.message);
    process.exit(1);
  });
}

module.exports = RealClaudePool;