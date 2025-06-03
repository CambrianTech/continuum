/**
 * UI Generator
 * Generates the web interface HTML
 */

class UIGenerator {
  constructor(continuum) {
    this.continuum = continuum;
  }

  generateHTML() {
    const packageInfo = require('../../package.json');
    
    return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum - Real Claude Instances</title>
    <style>
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: #1a1a1a; 
            color: #e0e0e0; 
            margin: 0; 
            padding: 20px;
            line-height: 1.6;
        }
        .container { 
            max-width: 800px; 
            margin: 0 auto; 
        }
        .header { 
            text-align: center; 
            margin-bottom: 30px; 
            border-bottom: 1px solid #333; 
            padding-bottom: 20px; 
        }
        .header h1 { 
            color: #4CAF50; 
            margin: 0; 
            font-size: 2.5em; 
        }
        .header p { 
            color: #888; 
            margin: 10px 0; 
        }
        .status-indicator {
            position: fixed;
            top: 20px;
            right: 20px;
            display: flex;
            align-items: center;
            background: #2a2a2a;
            padding: 10px 15px;
            border-radius: 20px;
            font-size: 0.9em;
            z-index: 1000;
        }
        .status-dot {
            width: 8px;
            height: 8px;
            border-radius: 50%;
            margin-right: 8px;
        }
        .status-dot.green { background: #4CAF50; }
        .status-dot.yellow { background: #FF9800; }
        .status-dot.red { background: #f44336; }
        .chat-container { 
            background: #2a2a2a; 
            border-radius: 10px; 
            height: 400px; 
            overflow-y: auto; 
            padding: 20px; 
            margin-bottom: 20px;
            border: 1px solid #444;
        }
        .message { 
            margin-bottom: 15px; 
            padding: 12px 16px; 
            border-radius: 18px; 
            max-width: 70%;
            position: relative;
        }
        .message.user { 
            background: #007AFF; 
            color: white; 
            margin-left: auto; 
            text-align: right;
        }
        .message.ai { 
            background: #3a3a3a; 
            margin-right: auto; 
        }
        .message-header {
            font-size: 0.75em;
            color: #999;
            margin-bottom: 4px;
            font-weight: 500;
        }
        .message.user .message-header {
            text-align: right;
            color: rgba(255,255,255,0.7);
        }
        .message.ai .message-header {
            text-align: left;
            color: #999;
        }
        .input-container { 
            display: flex; 
            gap: 10px; 
            margin-bottom: 20px; 
        }
        input[type="text"] { 
            flex: 1; 
            padding: 12px 16px; 
            border: 1px solid #444; 
            border-radius: 20px; 
            background: #2a2a2a; 
            color: #e0e0e0; 
            font-size: 16px;
        }
        input[type="text"]:focus { 
            outline: none; 
            border-color: #007AFF; 
        }
        button { 
            padding: 12px 24px; 
            background: #007AFF; 
            color: white; 
            border: none; 
            border-radius: 20px; 
            cursor: pointer; 
            font-size: 16px;
            font-weight: 500;
        }
        button:hover { 
            background: #0056b3; 
        }
        button:disabled { 
            background: #555; 
            cursor: not-allowed; 
        }
        .working { 
            font-style: italic; 
            opacity: 0.7; 
            margin-right: 8px; 
        }
        .costs { 
            text-align: center; 
            padding: 15px; 
            background: #2a2a2a; 
            border-radius: 6px; 
            margin: 20px 0; 
            font-size: 0.9em;
            color: #ccc;
        }
        .instance-info {
            text-align: center;
            font-size: 0.8em;
            color: #666;
            margin-top: 5px;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="status-indicator" id="statusIndicator">
            <div class="status-dot red" id="statusDot"></div>
            <span id="statusText">Connecting...</span>
        </div>
        
        <div class="header">
            <h1>🌌 Continuum</h1>
            <p>Real Claude Instance Pool</p>
            <div class="costs" id="costs">
                Loading costs...
            </div>
            <div class="instance-info" id="instanceInfo">
                Loading instance info...
            </div>
        </div>
        
        <div class="chat-container" id="chat">
            <!-- Messages will appear here -->
        </div>
        
        <div class="input-container">
            <input type="text" id="messageInput" placeholder="Ask Claude anything..." autofocus>
            <button id="sendButton">Send</button>
        </div>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:${this.continuum.port}');
        const chat = document.getElementById('chat');
        const messageInput = document.getElementById('messageInput');
        const sendButton = document.getElementById('sendButton');
        const statusDot = document.getElementById('statusDot');
        const statusText = document.getElementById('statusText');
        let isWorking = false;

        ws.onopen = function() {
            statusDot.className = 'status-dot green';
            statusText.textContent = 'Connected';
        };

        ws.onclose = function() {
            statusDot.className = 'status-dot red';
            statusText.textContent = 'Disconnected';
        };

        ws.onerror = function() {
            statusDot.className = 'status-dot red';
            statusText.textContent = 'Error';
        };

        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'status') {
                updateStatus(data);
                updateCosts(data.data.costs);
                updateInstanceInfo(data.data);
            } else if (data.type === 'working') {
                statusDot.className = 'status-dot yellow';
                statusText.textContent = 'Working...';
                isWorking = true;
                sendButton.disabled = true;
                sendButton.textContent = 'Working...';
            } else if (data.type === 'result') {
                statusDot.className = 'status-dot green';
                statusText.textContent = 'Connected';
                isWorking = false;
                sendButton.disabled = false;
                sendButton.textContent = 'Send';
                
                addMessage('ai', data.data.result, data.data.role);
                updateCosts(data.data.costs);
            } else if (data.type === 'error') {
                statusDot.className = 'status-dot red';
                statusText.textContent = 'Error';
                isWorking = false;
                sendButton.disabled = false;
                sendButton.textContent = 'Send';
                
                addMessage('ai', 'Error: ' + data.data, 'System');
            }
        };

        function addMessage(sender, message, agentRole = null) {
            const messageDiv = document.createElement('div');
            messageDiv.className = \`message \${sender}\`;
            
            const headerDiv = document.createElement('div');
            headerDiv.className = 'message-header';
            
            if (sender === 'user') {
                headerDiv.textContent = 'You';
            } else {
                headerDiv.textContent = agentRole || 'AI';
            }
            
            const contentDiv = document.createElement('div');
            contentDiv.textContent = message;
            
            messageDiv.appendChild(headerDiv);
            messageDiv.appendChild(contentDiv);
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }

        function updateCosts(costs) {
            const costsDiv = document.getElementById('costs');
            if (costs && costs.total !== undefined) {
                costsDiv.innerHTML = \`📊 \${costs.requests} requests | 💰 $\${costs.total.toFixed(6)} | ⚡ $\${(costs.total/Math.max(costs.requests,1)).toFixed(6)}/req\`;
            }
        }
        
        function updateInstanceInfo(statusData) {
            const instanceDiv = document.getElementById('instanceInfo');
            if (statusData) {
                instanceDiv.innerHTML = \`
                    📦 v\${statusData.version} | 🔧 \${statusData.nodeVersion} | 
                    🆔 PID \${statusData.pid} | ⏱️ \${Math.floor(statusData.uptime)}s uptime
                \`;
            }
        }

        function updateStatus(statusData) {
            // Status updates handled in onmessage
        }

        function sendMessage() {
            const message = messageInput.value.trim();
            if (message && !isWorking) {
                addMessage('user', message);
                
                ws.send(JSON.stringify({
                    type: 'task',
                    role: 'PlannerAI',
                    task: message
                }));
                
                messageInput.value = '';
            }
        }

        sendButton.addEventListener('click', sendMessage);
        messageInput.addEventListener('keypress', function(e) {
            if (e.key === 'Enter') {
                sendMessage();
            }
        });
        
        // Focus input on load
        messageInput.focus();
    </script>
</body>
</html>`;
  }
}

module.exports = UIGenerator;