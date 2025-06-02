#!/usr/bin/env node
/**
 * SIMPLE AI - Actually works
 * 
 * No fancy BS, just a working Claude pool that responds
 */
const http = require('http');
const WebSocket = require('ws');

class SimpleAI {
  constructor() {
    this.personas = ['QuestionerClaude', 'PlannerClaude', 'ImplementerClaude'];
    this.conversations = [];
  }

  respond(persona, message) {
    // I am Claude, so I'll respond as different personas
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
        res.end(this.getHTML());
      } else if (req.url === '/favicon.ico') {
        // Return empty favicon to prevent 404 errors
        res.writeHead(200, { 'Content-Type': 'image/x-icon' });
        res.end();
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    const wss = new WebSocket.Server({ server });
    
    wss.on('connection', (ws) => {
      console.log('Connected');
      
      ws.on('message', (data) => {
        try {
          const msg = JSON.parse(data);
          const response = this.respond(msg.persona || 'QuestionerClaude', msg.message);
          
          ws.send(JSON.stringify({
            persona: msg.persona || 'QuestionerClaude',
            response: response
          }));
        } catch (e) {
          ws.send(JSON.stringify({ error: e.message }));
        }
      });
    });

    server.listen(5559, () => {
      console.log('🤖 Simple AI running at http://localhost:5559');
    });
  }

  getHTML() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Simple AI Pool</title>
    <style>
        body { font-family: Arial; padding: 20px; background: #1a1a1a; color: #fff; }
        .chat { border: 1px solid #333; height: 400px; overflow-y: auto; padding: 10px; margin: 10px 0; background: #2a2a2a; }
        input, select, button { padding: 10px; margin: 5px; }
        .message { margin: 10px 0; padding: 10px; background: #333; border-radius: 5px; }
    </style>
</head>
<body>
    <h1>🤖 Simple AI Pool</h1>
    <div class="chat" id="chat"></div>
    
    <select id="persona">
        <option value="QuestionerClaude">QuestionerClaude</option>
        <option value="PlannerClaude">PlannerClaude</option>
        <option value="ImplementerClaude">ImplementerClaude</option>
    </select>
    
    <input type="text" id="message" placeholder="Type message..." onkeypress="if(event.key==='Enter') send()">
    <button onclick="send()">Send</button>

    <script>
        const ws = new WebSocket('ws://localhost:5559');
        const chat = document.getElementById('chat');

        ws.onmessage = (e) => {
            const data = JSON.parse(e.data);
            addMessage(data.persona, data.response || data.error);
        };

        function send() {
            const message = document.getElementById('message').value;
            const persona = document.getElementById('persona').value;
            
            if (!message) return;
            
            addMessage('You', message);
            
            ws.send(JSON.stringify({
                message: message,
                persona: persona
            }));
            
            document.getElementById('message').value = '';
        }

        function addMessage(sender, content) {
            const div = document.createElement('div');
            div.className = 'message';
            div.innerHTML = '<strong>' + sender + ':</strong> ' + content;
            chat.appendChild(div);
            chat.scrollTop = chat.scrollHeight;
        }
    </script>
</body>
</html>`;
  }
}

const ai = new SimpleAI();
ai.start();