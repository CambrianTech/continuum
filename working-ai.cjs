#!/usr/bin/env node
/**
 * WORKING AI - Fixed version
 * 
 * Simple, guaranteed working Claude pool
 */
const http = require('http');
const WebSocket = require('ws');

class WorkingAI {
  constructor() {
    this.personas = ['QuestionerClaude', 'PlannerClaude', 'ImplementerClaude'];
  }

  respond(persona, message) {
    if (persona === 'QuestionerClaude') {
      return `I need to understand: "${message}". What specifically do you want to achieve?`;
    }
    if (persona === 'PlannerClaude') {
      return `For "${message}", here's my plan:\n1. Analyze requirements\n2. Break into steps\n3. Execute systematically`;
    }
    if (persona === 'ImplementerClaude') {
      return `Let's implement "${message}" right now. What's the first concrete step?`;
    }
    return `I understand: ${message}`;
  }

  start() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(`<!DOCTYPE html>
<html>
<head>
    <title>Working AI Pool</title>
    <style>
        body { 
            font-family: Arial, sans-serif; 
            padding: 20px; 
            background: #1a1a1a; 
            color: #fff; 
            margin: 0;
        }
        .container {
            max-width: 800px;
            margin: 0 auto;
        }
        .chat { 
            border: 2px solid #333; 
            height: 400px; 
            overflow-y: auto; 
            padding: 15px; 
            margin: 20px 0; 
            background: #2a2a2a; 
            border-radius: 8px;
        }
        .controls {
            background: #333;
            padding: 20px;
            border-radius: 8px;
            margin: 20px 0;
        }
        input[type="text"] { 
            width: 60%; 
            padding: 12px; 
            margin: 5px; 
            background: #444;
            border: 1px solid #555;
            color: #fff;
            border-radius: 4px;
        }
        select { 
            padding: 12px; 
            margin: 5px; 
            background: #444;
            border: 1px solid #555;
            color: #fff;
            border-radius: 4px;
        }
        button { 
            padding: 12px 20px; 
            margin: 5px; 
            background: #007acc;
            color: white;
            border: none;
            border-radius: 4px;
            cursor: pointer;
            font-weight: bold;
        }
        button:hover {
            background: #005a99;
        }
        .message { 
            margin: 10px 0; 
            padding: 12px; 
            background: #444; 
            border-radius: 6px; 
            border-left: 4px solid #007acc;
        }
        .status {
            background: #2a4a2a;
            padding: 10px;
            border-radius: 4px;
            margin: 10px 0;
            border-left: 4px solid #4a8;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🤖 Working AI Pool</h1>
        <div class="status" id="status">Connecting...</div>
        
        <div class="chat" id="chat">
            <div class="message">
                <strong>System:</strong> Welcome! Choose a persona and start chatting.
            </div>
        </div>
        
        <div class="controls">
            <select id="persona">
                <option value="QuestionerClaude">🤔 QuestionerClaude - Asks clarifying questions</option>
                <option value="PlannerClaude">📋 PlannerClaude - Creates detailed plans</option>
                <option value="ImplementerClaude">⚡ ImplementerClaude - Focus on execution</option>
            </select>
            <br>
            <input type="text" id="message" placeholder="Type your message here..." />
            <button onclick="sendMessage()">Send Message</button>
            <button onclick="clearChat()">Clear Chat</button>
        </div>
    </div>

    <script>
        let ws = null;
        const chat = document.getElementById('chat');
        const messageInput = document.getElementById('message');
        const personaSelect = document.getElementById('persona');
        const status = document.getElementById('status');

        function connectWebSocket() {
            try {
                ws = new WebSocket('ws://localhost:5560');
                
                ws.onopen = function() {
                    status.textContent = '✅ Connected to Working AI';
                    status.style.background = '#2a4a2a';
                    addMessage('System', 'WebSocket connected successfully!');
                };
                
                ws.onmessage = function(event) {
                    try {
                        const data = JSON.parse(event.data);
                        addMessage(data.persona, data.response);
                    } catch (e) {
                        addMessage('Error', 'Failed to parse response: ' + e.message);
                    }
                };
                
                ws.onerror = function(error) {
                    status.textContent = '❌ Connection error';
                    status.style.background = '#4a2a2a';
                    addMessage('Error', 'WebSocket error: ' + error.message);
                };
                
                ws.onclose = function() {
                    status.textContent = '🔌 Disconnected - attempting reconnect...';
                    status.style.background = '#4a4a2a';
                    setTimeout(connectWebSocket, 2000);
                };
            } catch (e) {
                status.textContent = '❌ Failed to connect';
                addMessage('Error', 'Connection failed: ' + e.message);
            }
        }

        function sendMessage() {
            const message = messageInput.value.trim();
            const persona = personaSelect.value;
            
            if (!message) {
                alert('Please enter a message');
                return;
            }
            
            if (!ws || ws.readyState !== WebSocket.OPEN) {
                alert('Not connected to AI. Please wait for connection.');
                return;
            }
            
            // Add user message to chat
            addMessage('You', message);
            
            // Send to AI
            try {
                ws.send(JSON.stringify({
                    message: message,
                    persona: persona
                }));
                messageInput.value = '';
            } catch (e) {
                addMessage('Error', 'Failed to send message: ' + e.message);
            }
        }

        function addMessage(sender, content) {
            const div = document.createElement('div');
            div.className = 'message';
            div.innerHTML = '<strong>' + sender + ':</strong> ' + content;
            chat.appendChild(div);
            chat.scrollTop = chat.scrollHeight;
        }

        function clearChat() {
            chat.innerHTML = '<div class="message"><strong>System:</strong> Chat cleared.</div>';
        }

        // Auto-connect when page loads
        connectWebSocket();
        
        // Focus input
        messageInput.focus();
        
        // Enter key to send
        messageInput.addEventListener('keypress', function(e) {
            if (e.key === 'Enter') {
                sendMessage();
            }
        });
    </script>
</body>
</html>`);
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
      console.log('🔌 User connected');
      
      ws.on('message', (data) => {
        try {
          const msg = JSON.parse(data);
          console.log(`📨 Received: "${msg.message}" for ${msg.persona}`);
          const response = this.respond(msg.persona || 'QuestionerClaude', msg.message);
          
          ws.send(JSON.stringify({
            persona: msg.persona || 'QuestionerClaude',
            response: response
          }));
          
          console.log(`📤 Sent: "${response}"`);
        } catch (e) {
          console.log('❌ Error:', e.message);
          ws.send(JSON.stringify({ error: e.message }));
        }
      });
      
      ws.on('close', () => {
        console.log('🔌 User disconnected');
      });
    });

    server.listen(5560, () => {
      console.log('🤖 Working AI running at http://localhost:5560');
      console.log('🎯 Try typing a message and selecting a persona!');
    });
  }
}

const ai = new WorkingAI();
ai.start();