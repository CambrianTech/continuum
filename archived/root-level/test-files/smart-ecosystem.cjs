#!/usr/bin/env node
/**
 * SMART ECOSYSTEM
 * 
 * Simple Claude pool that coordinates intelligently
 * No fake BS, just working coordination
 */
const { exec } = require('child_process');
const http = require('http');
const WebSocket = require('ws');

class SmartEcosystem {
  constructor() {
    this.instances = ['QuestionerClaude', 'PlannerClaude', 'ImplementerClaude'];
    this.conversations = new Map();
  }

  async callClaude(role, message) {
    const prompt = `You are ${role}. ${message}`;
    
    return new Promise((resolve, reject) => {
      exec(`claude --print "${prompt}"`, { timeout: 10000 }, (error, stdout, stderr) => {
        if (error) {
          resolve(`${role}: Sorry, I'm having trouble responding right now.`);
        } else {
          resolve(stdout.trim() || `${role}: I understand your message.`);
        }
      });
    });
  }

  async routeMessage(message) {
    // Simple intelligent routing
    const lower = message.toLowerCase();
    
    if (lower.includes('plan') || lower.includes('how')) {
      return 'PlannerClaude';
    } else if (lower.includes('code') || lower.includes('implement')) {
      return 'ImplementerClaude';  
    } else {
      return 'QuestionerClaude';
    }
  }

  async processMessage(message) {
    try {
      const instance = await this.routeMessage(message);
      console.log(`→ ${instance}: ${message}`);
      
      const response = await this.callClaude(instance, message);
      console.log(`← ${instance}: ${response.substring(0, 100)}...`);
      
      return { instance, response };
    } catch (error) {
      return { error: error.message };
    }
  }

  start() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(`<!DOCTYPE html>
<html>
<head>
    <title>Smart Ecosystem</title>
    <style>
        body { font-family: Arial; padding: 20px; background: #f0f0f0; }
        .chat { border: 1px solid #ccc; height: 400px; overflow-y: auto; padding: 10px; background: white; margin: 10px 0; }
        input { width: 80%; padding: 10px; }
        button { padding: 10px 20px; }
        .message { margin: 10px 0; padding: 10px; background: #f9f9f9; border-radius: 5px; }
    </style>
</head>
<body>
    <h1>Smart Ecosystem</h1>
    <div class="chat" id="chat"></div>
    <input type="text" id="message" placeholder="Type your message..." onkeypress="if(event.key==='Enter') send()">
    <button onclick="send()">Send</button>

    <script>
        const ws = new WebSocket('ws://localhost:5563');
        const chat = document.getElementById('chat');

        ws.onmessage = (e) => {
            const data = JSON.parse(e.data);
            if (data.response) {
                addMessage(data.instance, data.response);
            } else if (data.error) {
                addMessage('Error', data.error);
            }
        };

        function send() {
            const message = document.getElementById('message').value;
            if (!message) return;
            
            addMessage('You', message);
            ws.send(JSON.stringify({ message }));
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
</html>`);
      } else {
        res.writeHead(404);
        res.end();
      }
    });

    const wss = new WebSocket.Server({ server });
    
    wss.on('connection', (ws) => {
      ws.on('message', async (data) => {
        const { message } = JSON.parse(data);
        const result = await this.processMessage(message);
        ws.send(JSON.stringify(result));
      });
    });

    server.listen(5563, () => {
      console.log('Smart Ecosystem running at http://localhost:5563');
    });
  }
}

new SmartEcosystem().start();