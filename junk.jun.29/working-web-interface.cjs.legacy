#!/usr/bin/env node
const { exec } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const WebSocket = require('ws');

const execAsync = promisify(exec);

const server = http.createServer((req, res) => {
  if (req.url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!DOCTYPE html>
<html>
<head>
    <title>Working Continuum</title>
    <style>
        body { background: #1a1a1a; color: white; font-family: monospace; padding: 20px; }
        .chat { border: 1px solid #333; background: #222; padding: 20px; height: 400px; overflow-y: auto; margin: 20px 0; }
        .input { width: 70%; background: #333; border: 1px solid #555; color: white; padding: 10px; }
        .button { background: #0088ff; border: none; color: white; padding: 10px 20px; cursor: pointer; }
        .message { margin: 10px 0; padding: 10px; background: #2a2a2a; border-radius: 4px; }
    </style>
</head>
<body>
    <h1>🌌 Working Continuum</h1>
    <div class="chat" id="chat"></div>
    <div>
        <input type="text" id="input" class="input" placeholder="Talk to Claude..." onkeypress="if(event.key==='Enter') send()">
        <button class="button" onclick="send()">Send</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:3333');
        
        ws.onopen = () => addMessage('🟢 Connected to real Claude');
        
        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            if (data.type === 'response') {
                addMessage('🤖 Claude: ' + data.message);
            }
        };
        
        function send() {
            const input = document.getElementById('input');
            const message = input.value.trim();
            if (!message) return;
            
            addMessage('👤 You: ' + message);
            ws.send(JSON.stringify({ type: 'message', content: message }));
            input.value = '';
        }
        
        function addMessage(text) {
            const chat = document.getElementById('chat');
            const div = document.createElement('div');
            div.className = 'message';
            div.textContent = text;
            chat.appendChild(div);
            chat.scrollTop = chat.scrollHeight;
        }
    </script>
</body>
</html>`);
  } else {
    res.writeHead(404);
    res.end('Not found');
  }
});

const wss = new WebSocket.Server({ server });

wss.on('connection', (ws) => {
  console.log('User connected');
  
  ws.on('message', async (message) => {
    try {
      const data = JSON.parse(message);
      
      if (data.type === 'message') {
        console.log('User message:', data.content);
        
        // Call real Claude
        const cmd = `claude -p "${data.content}" --output-format json`;
        const result = await execAsync(cmd);
        const response = JSON.parse(result.stdout);
        
        console.log('Claude responded:', response.result);
        
        // Send response back
        ws.send(JSON.stringify({
          type: 'response',
          message: response.result
        }));
      }
    } catch (error) {
      console.error('Error:', error.message);
      ws.send(JSON.stringify({
        type: 'response',
        message: 'Error: ' + error.message
      }));
    }
  });
});

server.listen(3333, () => {
  console.log('🌐 Working interface at http://localhost:3333');
  console.log('💬 Real Claude responses guaranteed');
  
  // Open browser
  require('child_process').exec('open http://localhost:3333');
});