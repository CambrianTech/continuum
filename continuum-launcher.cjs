#!/usr/bin/env node
/**
 * CONTINUUM LAUNCHER
 * 
 * Launches real Claude instances via Continuum and coordinates them
 * Uses actual claude CLI calls, not fake personas
 */
const { spawn, exec } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');
const WebSocket = require('ws');

class ContinuumLauncher {
  constructor() {
    this.claudeInstances = new Map();
    this.messageQueue = [];
    this.isInitialized = false;
  }

  async launchContinuum() {
    console.log('🌌 CONTINUUM LAUNCHER');
    console.log('===================');
    console.log('🚀 Launching real Claude instances via Continuum...');
    
    // Launch multiple Claude instances
    await this.spawnClaudeInstance('QuestionerClaude', 'Ask clarifying questions and explore requirements');
    await this.spawnClaudeInstance('PlannerClaude', 'Create detailed plans and break down tasks');
    await this.spawnClaudeInstance('ImplementerClaude', 'Focus on implementation and code execution');
    
    // Create coordination interface
    await this.createCoordinationInterface();
    
    this.isInitialized = true;
    console.log('🎉 Continuum pool ready at http://localhost:5561');
  }

  async spawnClaudeInstance(instanceName, role) {
    console.log(`🔄 Spawning ${instanceName}...`);
    
    try {
      // Create communication files for this instance
      const commDir = path.join(process.cwd(), `.continuum-${instanceName}`);
      if (!fs.existsSync(commDir)) {
        fs.mkdirSync(commDir);
      }

      const instance = {
        name: instanceName,
        role: role,
        commDir: commDir,
        inputFile: path.join(commDir, 'input.txt'),
        outputFile: path.join(commDir, 'output.txt'),
        statusFile: path.join(commDir, 'status.txt'),
        conversationHistory: []
      };

      // Initialize communication files
      fs.writeFileSync(instance.statusFile, 'ready');
      fs.writeFileSync(instance.inputFile, '');
      fs.writeFileSync(instance.outputFile, '');

      this.claudeInstances.set(instanceName, instance);
      console.log(`✅ ${instanceName} spawned with communication directory`);
      
    } catch (error) {
      console.error(`❌ Failed to spawn ${instanceName}:`, error.message);
    }
  }

  async callClaudeInstance(instanceName, message) {
    const instance = this.claudeInstances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }

    console.log(`📨 Calling ${instanceName}: "${message}"`);

    // Create role-specific prompt
    const prompt = `You are ${instanceName}. ${instance.role}

User message: ${message}

Respond as ${instanceName} would, focusing on your specific role.`;

    return new Promise((resolve, reject) => {
      // Use claude CLI to get response
      const escapedPrompt = prompt.replace(/"/g, '\\"').replace(/\n/g, ' ');
      const command = `claude --print "${escapedPrompt}"`;

      exec(command, { timeout: 15000 }, (error, stdout, stderr) => {
        if (error) {
          console.error(`❌ ${instanceName} error:`, error.message);
          reject(new Error(`Claude instance failed: ${error.message}`));
          return;
        }

        const response = stdout.trim();
        if (response && response.length > 0) {
          console.log(`📤 ${instanceName} responded: "${response.substring(0, 100)}..."`);
          
          // Store in conversation history
          instance.conversationHistory.push({
            timestamp: new Date().toISOString(),
            user: message,
            assistant: response
          });

          resolve(response);
        } else {
          reject(new Error('No response from Claude'));
        }
      });
    });
  }

  async routeMessage(message) {
    // Intelligent routing based on message content
    const lowerMessage = message.toLowerCase();
    
    if (lowerMessage.includes('plan') || lowerMessage.includes('strategy') || lowerMessage.includes('how to')) {
      return 'PlannerClaude';
    } else if (lowerMessage.includes('code') || lowerMessage.includes('implement') || lowerMessage.includes('build')) {
      return 'ImplementerClaude';
    } else {
      return 'QuestionerClaude'; // Default for clarification
    }
  }

  async processMessage(message) {
    if (!this.isInitialized) {
      return { error: 'Continuum not initialized' };
    }

    try {
      // Route to appropriate instance
      const targetInstance = await this.routeMessage(message);
      console.log(`🎯 Routing to: ${targetInstance}`);
      
      // Get response from Claude instance
      const response = await this.callClaudeInstance(targetInstance, message);
      
      return {
        instance: targetInstance,
        response: response,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      console.error('❌ Message processing error:', error.message);
      return { error: error.message };
    }
  }

  async createCoordinationInterface() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateInterface());
      } else if (req.url === '/favicon.ico') {
        res.writeHead(200, { 'Content-Type': 'image/x-icon' });
        res.end();
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    const wss = new WebSocket.Server({ server });
    
    wss.on('connection', (ws) => {
      console.log('🔌 Client connected to Continuum');
      
      ws.send(JSON.stringify({
        type: 'continuum_ready',
        instances: Array.from(this.claudeInstances.keys()),
        message: 'Continuum pool ready - messages will be automatically routed'
      }));

      ws.on('message', async (data) => {
        try {
          const { message } = JSON.parse(data);
          const result = await this.processMessage(message);
          
          ws.send(JSON.stringify({
            type: 'continuum_response',
            ...result
          }));
          
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'error',
            error: error.message
          }));
        }
      });
    });

    return new Promise((resolve) => {
      server.listen(5561, () => {
        console.log('🌐 Continuum coordination interface ready');
        resolve();
      });
    });
  }

  generateInterface() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Continuum - Claude Pool</title>
    <style>
        body { 
            font-family: 'Segoe UI', sans-serif; 
            padding: 20px; 
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); 
            color: #fff; 
            margin: 0;
        }
        .container { max-width: 900px; margin: 0 auto; }
        .header { 
            text-align: center; 
            background: rgba(255,255,255,0.1); 
            padding: 30px; 
            border-radius: 15px; 
            margin-bottom: 30px; 
        }
        .status { 
            background: rgba(255,255,255,0.1); 
            padding: 15px; 
            border-radius: 10px; 
            margin: 20px 0; 
        }
        .chat { 
            background: rgba(255,255,255,0.1); 
            height: 400px; 
            overflow-y: auto; 
            padding: 20px; 
            border-radius: 10px; 
            margin: 20px 0; 
        }
        .message { 
            margin: 15px 0; 
            padding: 15px; 
            background: rgba(255,255,255,0.2); 
            border-radius: 8px; 
            border-left: 4px solid #fff;
        }
        .input-area {
            background: rgba(255,255,255,0.1);
            padding: 20px;
            border-radius: 10px;
        }
        input[type="text"] { 
            width: calc(100% - 140px); 
            padding: 15px; 
            background: rgba(255,255,255,0.2); 
            border: none; 
            color: #fff; 
            border-radius: 8px; 
        }
        input::placeholder { color: rgba(255,255,255,0.7); }
        button { 
            padding: 15px 25px; 
            background: #fff; 
            color: #667eea; 
            border: none; 
            border-radius: 8px; 
            cursor: pointer; 
            font-weight: bold; 
            margin-left: 10px;
        }
        button:hover { background: #f0f0f0; }
        .instance-tag {
            display: inline-block;
            background: rgba(255,255,255,0.3);
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 0.8em;
            margin-right: 10px;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🌌 Continuum</h1>
            <p>Claude Pool with Intelligent Routing</p>
        </div>
        
        <div class="status" id="status">Connecting to Continuum...</div>
        
        <div class="chat" id="chat"></div>
        
        <div class="input-area">
            <input type="text" id="message" placeholder="Ask anything - Continuum will route to the right Claude instance..." />
            <button onclick="sendToContinuum()">Send to Pool</button>
        </div>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5561');
        const chat = document.getElementById('chat');
        const messageInput = document.getElementById('message');
        const status = document.getElementById('status');

        ws.onopen = () => {
            status.textContent = '🌌 Connected to Continuum';
        };

        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            
            if (data.type === 'continuum_ready') {
                status.innerHTML = '✅ Continuum Ready - Instances: ' + data.instances.join(', ');
                addMessage('Continuum', data.message);
            } else if (data.type === 'continuum_response') {
                if (data.error) {
                    addMessage('Error', data.error);
                } else {
                    addMessage(data.instance, data.response);
                }
            }
        };

        function sendToContinuum() {
            const message = messageInput.value.trim();
            if (!message) return;
            
            addMessage('You', message);
            
            ws.send(JSON.stringify({ message: message }));
            messageInput.value = '';
        }

        function addMessage(sender, content) {
            const div = document.createElement('div');
            div.className = 'message';
            
            let senderTag = '';
            if (sender === 'QuestionerClaude' || sender === 'PlannerClaude' || sender === 'ImplementerClaude') {
                senderTag = '<span class="instance-tag">' + sender + '</span>';
            }
            
            div.innerHTML = senderTag + '<strong>' + sender + ':</strong> ' + content;
            chat.appendChild(div);
            chat.scrollTop = chat.scrollHeight;
        }

        messageInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') sendToContinuum();
        });
        
        messageInput.focus();
    </script>
</body>
</html>`;
  }
}

// Launch Continuum
if (require.main === module) {
  const launcher = new ContinuumLauncher();
  launcher.launchContinuum().catch(console.error);
}

module.exports = ContinuumLauncher;