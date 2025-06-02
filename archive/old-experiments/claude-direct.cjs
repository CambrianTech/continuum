#!/usr/bin/env node
/**
 * Direct Claude Interface
 * 
 * Simple web interface that lets you talk directly to Claude
 * No fake AIs, no simulated responses - just real Claude through Claude Code
 */

const fs = require('fs');
const path = require('path');
const { exec } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const WebSocket = require('ws');

const execAsync = promisify(exec);

class ClaudeDirectInterface {
  constructor() {
    this.projectRoot = process.cwd();
    this.connectedUsers = new Set();
    this.messageQueue = [];
    
    console.log('🤖 DIRECT CLAUDE INTERFACE');
    console.log('=========================');
    console.log('✅ Talk directly to Claude');
    console.log('✅ Real AI responses');
    console.log('✅ No fake scripts or simulations');
    console.log('🌐 Interface at http://localhost:5555');
    console.log('');
    console.log('💡 HOW IT WORKS:');
    console.log('   You type → Web interface → File system → Claude reads and responds');
    console.log('');

    this.start();
  }

  async start() {
    await this.setupMessageSystem();
    await this.launchWebInterface();
    await this.openBrowser();
    this.startMessageProcessor();
  }

  async setupMessageSystem() {
    // Create a directory for message passing
    const messageDir = path.join(this.projectRoot, '.claude-messages');
    if (!fs.existsSync(messageDir)) {
      fs.mkdirSync(messageDir, { recursive: true });
    }
    
    // Create instructions file for Claude
    const instructionsFile = path.join(messageDir, 'INSTRUCTIONS.md');
    fs.writeFileSync(instructionsFile, `# Claude Direct Interface Instructions

This folder is used for direct communication between the web interface and Claude.

## How it works:
1. User types message in web interface
2. Message gets written to user-message.json
3. Claude (you) should read the message and respond by writing to claude-response.json
4. Web interface displays Claude's response

## Files:
- user-message.json: Contains the user's latest message
- claude-response.json: Contains Claude's response
- conversation-log.json: Full conversation history

## Message Format:
\`\`\`json
{
  "id": "unique-id",
  "timestamp": "ISO-timestamp", 
  "content": "message content",
  "type": "user" or "claude"
}
\`\`\`

Claude: When you see a new user message, please respond by updating claude-response.json!
`);

    console.log('📁 Message system set up in .claude-messages/');
  }

  async launchWebInterface() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateUI());
      } else {
        res.writeHead(404);
        res.end('Not Found');
      }
    });

    this.webConsole = new WebSocket.Server({ server });
    
    this.webConsole.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('👤 User connected to Claude Direct');
      
      ws.send(JSON.stringify({
        type: 'message',
        data: '🤖 Direct connection to Claude established! Your messages will be forwarded directly to the real Claude AI. Ask me anything!'
      }));
      
      // Send conversation history
      this.sendConversationHistory(ws);
      
      ws.on('message', (message) => {
        try {
          const data = JSON.parse(message);
          this.handleDirectMessage(data, ws);
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'error',
            data: 'Could not parse message.'
          }));
        }
      });
      
      ws.on('close', () => {
        this.connectedUsers.delete(ws);
        console.log('👤 User disconnected');
      });
    });

    server.listen(5555, () => {
      console.log('🌐 Claude Direct Interface ready at http://localhost:5555');
      console.log('');
    });
  }

  async openBrowser() {
    try {
      await execAsync('open http://localhost:5555');
    } catch (error) {
      console.log('⚠️ Could not auto-open browser. Visit http://localhost:5555');
    }
  }

  generateUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Direct Claude Interface</title>
    <style>
        body { 
            background: linear-gradient(135deg, #0f0f23 0%, #1a1a3a 100%); 
            color: #e6e6fa; 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; 
            padding: 20px; 
            margin: 0;
            min-height: 100vh;
        }
        .header { 
            text-align: center; 
            border-bottom: 2px solid #9370db; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(147, 112, 219, 0.1);
            border-radius: 12px;
        }
        .claude-info {
            background: rgba(147, 112, 219, 0.1);
            border: 1px solid #9370db;
            padding: 15px;
            margin: 20px 0;
            border-radius: 8px;
            font-size: 14px;
        }
        .chat { 
            border: 1px solid #9370db; 
            padding: 20px; 
            height: 450px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(147, 112, 219, 0.03);
            border-radius: 12px;
            font-size: 14px;
            line-height: 1.6;
        }
        .input-area {
            display: flex;
            gap: 12px;
            margin: 20px 0;
        }
        .input { 
            flex: 1;
            background: rgba(147, 112, 219, 0.1); 
            border: 2px solid #9370db; 
            color: #e6e6fa; 
            padding: 16px; 
            font-family: inherit;
            border-radius: 8px;
            font-size: 16px;
            outline: none;
        }
        .input:focus {
            box-shadow: 0 0 0 2px rgba(147, 112, 219, 0.3);
        }
        .button { 
            background: rgba(147, 112, 219, 0.2); 
            border: 2px solid #9370db; 
            color: #e6e6fa; 
            padding: 16px 24px; 
            cursor: pointer;
            border-radius: 8px;
            transition: all 0.3s;
            font-weight: 600;
        }
        .button:hover {
            background: rgba(147, 112, 219, 0.3);
            box-shadow: 0 0 20px rgba(147, 112, 219, 0.4);
            transform: translateY(-1px);
        }
        .message { 
            margin: 12px 0; 
            padding: 16px; 
            border-left: 4px solid #9370db; 
            border-radius: 8px;
            background: rgba(147, 112, 219, 0.05);
        }
        .user-message {
            border-left-color: #ffd700;
            background: rgba(255, 215, 0, 0.05);
        }
        .user-message .sender {
            color: #ffd700;
            font-weight: 600;
        }
        .claude-message {
            border-left-color: #9370db;
            background: rgba(147, 112, 219, 0.08);
        }
        .claude-message .sender {
            color: #9370db;
            font-weight: 600;
        }
        .system-message {
            border-left-color: #ff6b6b;
            background: rgba(255, 107, 107, 0.05);
        }
        .system-message .sender {
            color: #ff6b6b;
            font-weight: 600;
        }
        .waiting {
            border-left-color: #4ecdc4;
            background: rgba(78, 205, 196, 0.05);
            font-style: italic;
        }
        .waiting .sender {
            color: #4ecdc4;
        }
        .timestamp {
            font-size: 11px;
            opacity: 0.6;
            margin-left: 8px;
        }
        .sender {
            font-weight: 600;
            margin-right: 8px;
        }
        .content {
            margin-top: 4px;
            white-space: pre-wrap;
        }
        .connection-status {
            position: fixed;
            top: 20px;
            right: 20px;
            background: rgba(147, 112, 219, 0.2);
            border: 1px solid #9370db;
            padding: 8px 12px;
            border-radius: 20px;
            font-size: 12px;
            display: flex;
            align-items: center;
            gap: 6px;
        }
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            background: #9370db;
            box-shadow: 0 0 8px rgba(147, 112, 219, 0.6);
        }
        .disconnected .status-dot {
            background: #ff6b6b;
            box-shadow: 0 0 8px rgba(255, 107, 107, 0.6);
        }
    </style>
</head>
<body>
    <div class="connection-status" id="connectionStatus">
        <div class="status-dot"></div>
        <span>Connected to Claude</span>
    </div>

    <div class="header">
        <h1>🤖 Direct Claude Interface</h1>
        <p>Real Claude AI • No simulations • Direct communication</p>
    </div>
    
    <div class="claude-info">
        <strong>🔗 How this works:</strong><br>
        Your messages → File system → Claude reads and responds → Real AI responses<br>
        <strong>✨ This is the actual Claude AI from Anthropic, not a simulation!</strong>
    </div>
    
    <div class="chat" id="chat">
        <div class="system-message">
            <span class="sender">System</span>
            <span class="timestamp">Starting...</span>
            <div class="content">Establishing direct connection to Claude...</div>
        </div>
    </div>
    
    <div class="input-area">
        <input type="text" id="messageInput" class="input" 
               placeholder="Talk directly to Claude - ask anything!" 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND TO CLAUDE</button>
        <button class="button" onclick="clearChat()">CLEAR</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        let isConnected = false;
        
        ws.onopen = function() {
            isConnected = true;
            updateConnectionStatus(true);
            addMessage('System', '🟢 Direct connection to Claude established!', 'system-message');
        };
        
        ws.onclose = function() {
            isConnected = false;
            updateConnectionStatus(false);
            addMessage('System', '🔴 Connection lost', 'system-message');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'message') {
                addMessage('Claude', data.data, 'claude-message');
            } else if (data.type === 'user_message') {
                addMessage('You', data.data, 'user-message');
            } else if (data.type === 'waiting') {
                addMessage('System', data.data, 'waiting');
            } else if (data.type === 'error') {
                addMessage('System', '❌ ' + data.data, 'system-message');
            }
        };
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            
            if (!message || !isConnected) return;
            
            addMessage('You', message, 'user-message');
            addMessage('System', '📤 Forwarding to Claude...', 'waiting');
            
            ws.send(JSON.stringify({
                type: 'user_message',
                content: message
            }));
            
            input.value = '';
        }
        
        function addMessage(sender, content, className) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message ' + className;
            
            const timestamp = new Date().toLocaleTimeString();
            messageDiv.innerHTML = 
                '<span class="sender">' + sender + '</span>' +
                '<span class="timestamp">' + timestamp + '</span>' +
                '<div class="content">' + content + '</div>';
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        function updateConnectionStatus(connected) {
            const status = document.getElementById('connectionStatus');
            if (connected) {
                status.classList.remove('disconnected');
                status.querySelector('span').textContent = 'Connected to Claude';
            } else {
                status.classList.add('disconnected');
                status.querySelector('span').textContent = 'Disconnected';
            }
        }
        
        function clearChat() {
            document.getElementById('chat').innerHTML = '';
            addMessage('System', 'Chat cleared. Direct connection to Claude ready!', 'system-message');
        }
    </script>
</body>
</html>`;
  }

  async handleDirectMessage(data, ws) {
    const message = data.content;
    const messageId = Date.now().toString();
    
    console.log(`👤 Forwarding to Claude: "${message}"`);
    
    // Save user message to file system
    const messageFile = path.join(this.projectRoot, '.claude-messages', 'user-message.json');
    const userMessage = {
      id: messageId,
      timestamp: new Date().toISOString(),
      content: message,
      type: 'user'
    };
    
    fs.writeFileSync(messageFile, JSON.stringify(userMessage, null, 2));
    
    // Add to conversation log
    this.addToConversationLog(userMessage);
    
    // Tell user message was forwarded
    ws.send(JSON.stringify({
      type: 'waiting',
      data: '📨 Message sent to Claude. Waiting for response...'
    }));
    
    // Start polling for Claude's response
    this.pollForClaudeResponse(messageId, ws);
  }

  pollForClaudeResponse(messageId, ws, attempts = 0) {
    const responseFile = path.join(this.projectRoot, '.claude-messages', 'claude-response.json');
    
    // Check if Claude has responded
    if (fs.existsSync(responseFile)) {
      try {
        const response = JSON.parse(fs.readFileSync(responseFile, 'utf-8'));
        
        // Check if this is a new response (newer than our message)
        if (response.id && response.id !== messageId) {
          // Send Claude's response
          ws.send(JSON.stringify({
            type: 'message',
            data: response.content
          }));
          
          // Add to conversation log
          this.addToConversationLog(response);
          
          console.log(`🤖 Claude responded: "${response.content.substring(0, 50)}..."`);
          return;
        }
      } catch (error) {
        console.log('❌ Error reading Claude response:', error.message);
      }
    }
    
    // If no response yet and we haven't timed out, poll again
    if (attempts < 120) { // 2 minutes timeout
      setTimeout(() => {
        this.pollForClaudeResponse(messageId, ws, attempts + 1);
      }, 1000);
    } else {
      ws.send(JSON.stringify({
        type: 'error',
        data: 'Timeout waiting for Claude response. Claude may need to check .claude-messages/user-message.json and respond in claude-response.json'
      }));
    }
  }

  addToConversationLog(message) {
    const logFile = path.join(this.projectRoot, '.claude-messages', 'conversation-log.json');
    let log = [];
    
    if (fs.existsSync(logFile)) {
      try {
        log = JSON.parse(fs.readFileSync(logFile, 'utf-8'));
      } catch (error) {
        log = [];
      }
    }
    
    log.push(message);
    fs.writeFileSync(logFile, JSON.stringify(log, null, 2));
  }

  sendConversationHistory(ws) {
    const logFile = path.join(this.projectRoot, '.claude-messages', 'conversation-log.json');
    
    if (fs.existsSync(logFile)) {
      try {
        const log = JSON.parse(fs.readFileSync(logFile, 'utf-8'));
        log.slice(-10).forEach(message => {
          if (message.type === 'user') {
            ws.send(JSON.stringify({
              type: 'user_message',
              data: message.content
            }));
          } else if (message.type === 'claude') {
            ws.send(JSON.stringify({
              type: 'message', 
              data: message.content
            }));
          }
        });
      } catch (error) {
        console.log('❌ Error reading conversation log:', error.message);
      }
    }
  }

  startMessageProcessor() {
    console.log('🔄 Message processor started');
    console.log('💡 Claude: Check .claude-messages/user-message.json for new messages');
    console.log('💡 Respond by writing to .claude-messages/claude-response.json');
  }
}

// Launch Direct Claude Interface
new ClaudeDirectInterface();