#!/usr/bin/env node
/**
 * WORKING POOL
 * 
 * Creates a functional Claude pool using the simplest approach that works
 * Uses shell commands since we know claude --print works in bash
 */

const http = require('http');
const WebSocket = require('ws');
const { spawn } = require('child_process');

class WorkingPool {
  constructor() {
    this.connectedUsers = new Set();
    this.instances = new Map();
    
    // Define working instances
    this.instances.set('QuestionerClaude', {
      role: 'questioner',
      prompt: 'You are QuestionerClaude. Ask clarifying questions to understand what the user really needs.',
      messageCount: 0,
      lastActivity: new Date().toISOString()
    });
    
    this.instances.set('PlannerClaude', {
      role: 'planner', 
      prompt: 'You are PlannerClaude. Create detailed plans and break down complex tasks into steps.',
      messageCount: 0,
      lastActivity: new Date().toISOString()
    });
    
    this.instances.set('ImplementerClaude', {
      role: 'implementer',
      prompt: 'You are ImplementerClaude. Execute tasks and provide concrete implementation guidance.',
      messageCount: 0,
      lastActivity: new Date().toISOString()
    });
  }

  async start() {
    console.log('🏊 WORKING CLAUDE POOL');
    console.log('======================');
    console.log('✅ Uses claude --print for guaranteed responses');
    console.log('🌐 Simple web interface');
    console.log('');
    
    await this.setupWebInterface();
    
    console.log('🎉 Working Pool is running!');
    console.log('🌐 Access at http://localhost:5555');
  }

  async setupWebInterface() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateUI());
      } else {
        res.writeHead(404);
        res.end('Not Found');
      }
    });

    this.webSocket = new WebSocket.Server({ server });
    
    this.webSocket.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('👤 User connected');
      
      ws.send(JSON.stringify({
        type: 'pool_status',
        data: {
          message: '🏊 Working Claude Pool ready',
          instances: Array.from(this.instances.entries()).map(([name, instance]) => ({
            name: name,
            role: instance.role,
            messageCount: instance.messageCount,
            lastActivity: instance.lastActivity
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
      console.log('🌐 Working Pool interface ready at http://localhost:5555');
    });
  }

  async handleUserMessage(data, ws) {
    const message = data.content;
    console.log(`📨 User message: "${message}"`);
    
    // Select appropriate Claude instance
    const targetInstanceName = this.selectInstance(message);
    const targetInstance = this.instances.get(targetInstanceName);
    
    ws.send(JSON.stringify({
      type: 'routing',
      data: `🎯 Routing to ${targetInstanceName}`
    }));
    
    try {
      // Call Claude using shell command
      const response = await this.callClaudeWithShell(targetInstance, message);
      
      // Update instance stats
      targetInstance.messageCount++;
      targetInstance.lastActivity = new Date().toISOString();
      
      ws.send(JSON.stringify({
        type: 'claude_response',
        data: {
          from: targetInstanceName,
          response: response,
          timestamp: new Date().toISOString()
        }
      }));
      
      console.log(`✅ ${targetInstanceName} responded: "${response.substring(0, 100)}..."`);
      
    } catch (error) {
      console.error(`❌ ${targetInstanceName} failed: ${error.message}`);
      
      ws.send(JSON.stringify({
        type: 'error',
        data: `${targetInstanceName} failed: ${error.message}`
      }));
    }
  }

  async callClaudeWithShell(instance, userMessage) {
    return new Promise((resolve, reject) => {
      // Build the full prompt
      const fullPrompt = `${instance.prompt}\\n\\nUser: ${userMessage}\\n\\nAssistant:`;
      
      // Use bash to call claude
      const bashCommand = `claude --print "${fullPrompt.replace(/"/g, '\\"')}"`;
      
      console.log(`🔄 Calling ${instance.role} via shell...`);
      
      const process = spawn('bash', ['-c', bashCommand], {
        stdio: ['pipe', 'pipe', 'pipe']
      });
      
      let stdout = '';
      let stderr = '';
      
      process.stdout.on('data', (data) => {
        stdout += data.toString();
      });
      
      process.stderr.on('data', (data) => {
        stderr += data.toString();
      });
      
      process.on('close', (code) => {
        if (stdout.trim()) {
          resolve(stdout.trim());
        } else {
          reject(new Error(`No response. Code: ${code}, stderr: ${stderr}`));
        }
      });
      
      process.on('error', (error) => {
        reject(new Error(`Process error: ${error.message}`));
      });
      
      // Timeout after 20 seconds
      setTimeout(() => {
        process.kill();
        reject(new Error('Claude call timed out'));
      }, 20000);
    });
  }

  selectInstance(message) {
    const lower = message.toLowerCase();
    
    if (lower.includes('question') || lower.includes('clarify') || lower.includes('understand')) {
      return 'QuestionerClaude';
    } else if (lower.includes('plan') || lower.includes('organize') || lower.includes('steps')) {
      return 'PlannerClaude';
    } else if (lower.includes('implement') || lower.includes('build') || lower.includes('create') || lower.includes('code')) {
      return 'ImplementerClaude';
    }
    
    // Default to QuestionerClaude for unclear requests
    return 'QuestionerClaude';
  }

  generateUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Working Claude Pool</title>
    <style>
        body { 
            background: linear-gradient(135deg, #0a2a0a 0%, #1a3a1a 50%, #2a4a2a 100%); 
            color: #88ff88; 
            font-family: 'Courier New', monospace; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            border: 2px solid #88ff88; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(136, 255, 136, 0.1);
        }
        .instances {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 15px;
            margin: 20px 0;
        }
        .instance {
            border: 1px solid #88ff88;
            padding: 15px;
            background: rgba(136, 255, 136, 0.05);
        }
        .chat { 
            border: 1px solid #88ff88; 
            padding: 20px; 
            height: 300px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(136, 255, 136, 0.02);
        }
        .input-area {
            display: flex;
            gap: 10px;
            margin: 20px 0;
        }
        .input { 
            flex: 1;
            background: rgba(136, 255, 136, 0.1); 
            border: 1px solid #88ff88; 
            color: #88ff88; 
            padding: 15px; 
            font-family: inherit;
        }
        .button { 
            background: rgba(136, 255, 136, 0.2); 
            border: 1px solid #88ff88; 
            color: #88ff88; 
            padding: 15px 25px; 
            cursor: pointer;
        }
        .message { 
            margin: 10px 0; 
            padding: 10px; 
            border-left: 3px solid #88ff88; 
        }
        .claude-response {
            background: rgba(136, 255, 136, 0.1);
            border-left-color: #00ff00;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🏊 WORKING CLAUDE POOL</h1>
        <p>Guaranteed working Claude instances using shell commands</p>
    </div>
    
    <div class="instances" id="instances">
        <!-- Instance status will be populated here -->
    </div>
    
    <div class="chat" id="chat">
        <div class="message">🏊 Working Claude Pool ready - send messages to get real Claude responses!</div>
    </div>
    
    <div class="input-area">
        <input type="text" id="messageInput" class="input" 
               placeholder="Ask Claude anything..." 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        
        ws.onopen = function() {
            addMessage('🟢 Connected to Working Claude Pool');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'pool_status') {
                addMessage(data.data.message);
                updateInstances(data.data.instances);
            } else if (data.type === 'routing') {
                addMessage(data.data);
            } else if (data.type === 'claude_response') {
                addMessage(\`🤖 \${data.data.from}: \${data.data.response}\`, 'claude-response');
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
        
        function addMessage(text, className = 'message') {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message ' + className;
            messageDiv.innerHTML = new Date().toLocaleTimeString() + ' - ' + text;
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function updateInstances(instances) {
            const container = document.getElementById('instances');
            container.innerHTML = '';
            
            instances.forEach(instance => {
                const div = document.createElement('div');
                div.className = 'instance';
                div.innerHTML = \`
                    <h3>\${instance.name}</h3>
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
}

// Start the working pool
if (require.main === module) {
  const pool = new WorkingPool();
  pool.start().catch(error => {
    console.error('💥 Failed to start working pool:', error.message);
    process.exit(1);
  });
}

module.exports = WorkingPool;