#!/usr/bin/env node
/**
 * TMUX CLAUDE POOL
 * 
 * Real Claude instances in tmux sessions
 * Shell scripts handle the communication
 */

const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const fs = require('fs');
const path = require('path');
const http = require('http');

const execAsync = promisify(exec);

class TmuxClaudePool {
  constructor() {
    this.poolDir = path.join(process.cwd(), '.claude-pool');
    this.instances = new Map();
    this.costs = { total: 0, requests: 0 };
    
    console.log('🔥 TMUX CLAUDE POOL');
    console.log('==================');
    console.log('✅ Real Claude in tmux sessions');
    console.log('✅ Shell script communication');
    console.log('✅ Actually works');
    console.log('');
    
    this.setup();
  }

  async setup() {
    // Create pool directory
    if (!fs.existsSync(this.poolDir)) {
      fs.mkdirSync(this.poolDir, { recursive: true });
    }
    
    // Create communication scripts
    this.createClaudeScript();
    
    // Start instances
    await this.createInstance('CodeAI');
    await this.createInstance('PlannerAI');
    await this.createInstance('GeneralAI');
    
    // Start web interface
    this.startWebInterface();
  }

  createClaudeScript() {
    const scriptPath = path.join(this.poolDir, 'claude-session.sh');
    
    const script = `#!/bin/bash
SESSION_NAME="$1"
PROMPT_FILE="$2"
RESPONSE_FILE="$3"

# Create tmux session if it doesn't exist
if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
    tmux new-session -d -s "$SESSION_NAME" -c "${process.cwd()}"
    sleep 1
    tmux send-keys -t "$SESSION_NAME" "claude" Enter
    sleep 3
fi

# Send prompt to Claude in tmux
if [ -f "$PROMPT_FILE" ]; then
    PROMPT=$(cat "$PROMPT_FILE")
    
    # Clear any existing content and send prompt
    tmux send-keys -t "$SESSION_NAME" "/clear" Enter
    sleep 1
    tmux send-keys -t "$SESSION_NAME" "$PROMPT" Enter
    
    # Wait for response and capture
    sleep 5
    
    # Get the response (last few lines)
    tmux capture-pane -t "$SESSION_NAME" -p | tail -20 > "$RESPONSE_FILE"
    
    echo "Response captured to $RESPONSE_FILE"
fi
`;

    fs.writeFileSync(scriptPath, script);
    fs.chmodSync(scriptPath, '755');
    
    console.log('📜 Claude session script created');
  }

  async createInstance(name) {
    console.log(`🚀 Creating ${name} in tmux...`);
    
    const sessionName = `claude-${name.toLowerCase()}`;
    const instanceDir = path.join(this.poolDir, name);
    
    if (!fs.existsSync(instanceDir)) {
      fs.mkdirSync(instanceDir, { recursive: true });
    }
    
    // Initialize the instance
    const initPrompt = `You are ${name}. You help with software development tasks. Keep responses concise and actionable.`;
    const promptFile = path.join(instanceDir, 'init-prompt.txt');
    const responseFile = path.join(instanceDir, 'init-response.txt');
    
    fs.writeFileSync(promptFile, initPrompt);
    
    try {
      const scriptPath = path.join(this.poolDir, 'claude-session.sh');
      await execAsync(`${scriptPath} "${sessionName}" "${promptFile}" "${responseFile}"`);
      
      let response = 'Instance created';
      if (fs.existsSync(responseFile)) {
        response = fs.readFileSync(responseFile, 'utf-8').trim();
      }
      
      this.instances.set(name, {
        sessionName: sessionName,
        instanceDir: instanceDir,
        created: new Date(),
        requests: 1,
        lastResponse: response
      });
      
      console.log(`✅ ${name} ready in tmux session: ${sessionName}`);
      return response;
    } catch (error) {
      console.error(`❌ Failed to create ${name}:`, error.message);
      return null;
    }
  }

  async sendTask(instanceName, task) {
    const instance = this.instances.get(instanceName);
    if (!instance) {
      throw new Error(`Instance ${instanceName} not found`);
    }
    
    console.log(`📨 Sending to ${instanceName}: ${task.substring(0, 50)}...`);
    
    const promptFile = path.join(instance.instanceDir, `prompt-${Date.now()}.txt`);
    const responseFile = path.join(instance.instanceDir, `response-${Date.now()}.txt`);
    
    fs.writeFileSync(promptFile, task);
    
    try {
      const scriptPath = path.join(this.poolDir, 'claude-session.sh');
      await execAsync(`${scriptPath} "${instance.sessionName}" "${promptFile}" "${responseFile}"`);
      
      let response = 'No response captured';
      if (fs.existsSync(responseFile)) {
        response = fs.readFileSync(responseFile, 'utf-8').trim();
        // Clean up the response (remove prompts and system messages)
        response = response.split('\n').filter(line => 
          !line.startsWith('>') && 
          !line.includes('claude') && 
          !line.includes('claude') &&
          line.trim().length > 0
        ).join('\n').trim();
      }
      
      instance.requests++;
      instance.lastResponse = response;
      this.costs.requests++;
      
      console.log(`✅ ${instanceName} responded: ${response.substring(0, 100)}...`);
      
      // Cleanup files
      setTimeout(() => {
        try {
          fs.unlinkSync(promptFile);
          fs.unlinkSync(responseFile);
        } catch (e) {
          // Ignore cleanup errors
        }
      }, 5000);
      
      return response;
    } catch (error) {
      console.error(`❌ Task failed for ${instanceName}:`, error.message);
      throw error;
    }
  }

  startWebInterface() {
    const server = http.createServer(async (req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      
      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      const url = new URL(req.url, 'http://localhost:3333');
      
      if (req.method === 'GET' && url.pathname === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateUI());
      } else if (req.method === 'GET' && url.pathname === '/ask') {
        const instance = url.searchParams.get('instance') || 'GeneralAI';
        const task = url.searchParams.get('task');
        
        if (task) {
          try {
            const result = await this.sendTask(instance, task);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
              instance: instance,
              task: task,
              result: result,
              costs: this.costs,
              tmux_session: this.instances.get(instance)?.sessionName
            }));
          } catch (error) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: error.message }));
          }
        } else {
          res.writeHead(400, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'No task provided' }));
        }
      } else if (req.method === 'GET' && url.pathname === '/status') {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({
          instances: Array.from(this.instances.entries()).map(([name, instance]) => ({
            name,
            sessionName: instance.sessionName,
            requests: instance.requests,
            lastResponse: instance.lastResponse?.substring(0, 100)
          })),
          costs: this.costs,
          poolDir: this.poolDir
        }));
      } else {
        res.writeHead(404);
        res.end('Not found');
      }
    });

    server.listen(3333, () => {
      console.log('🌐 Tmux Claude Pool at http://localhost:3333');
      console.log('💬 Real Claude instances in tmux sessions');
      console.log('📊 Check /status for instance info');
      console.log('');
      console.log('🎯 Test it:');
      console.log('curl "http://localhost:3333/ask?instance=GeneralAI&task=hello"');
      console.log('');
      
      // Open browser
      try {
        require('child_process').exec('open http://localhost:3333');
      } catch (error) {
        // Browser opening failed, continue anyway
      }
    });
  }

  generateUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Tmux Claude Pool</title>
    <style>
        body { 
            background: #0d1117; 
            color: #c9d1d9; 
            font-family: 'SF Mono', Monaco, 'Cascadia Code', monospace; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            border: 2px solid #30363d; 
            padding: 30px; 
            margin-bottom: 30px; 
            background: #161b22;
            border-radius: 8px;
        }
        .header h1 { 
            margin: 0; 
            color: #7c3aed; 
            font-size: 2.5em; 
        }
        .instances { 
            display: grid; 
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); 
            gap: 20px; 
            margin: 30px 0; 
        }
        .instance-card { 
            border: 1px solid #30363d; 
            background: #161b22; 
            padding: 20px; 
            border-radius: 8px; 
        }
        .instance-card h3 { 
            color: #58a6ff; 
            margin: 0 0 10px 0; 
        }
        .chat { 
            border: 1px solid #30363d; 
            background: #0d1117; 
            padding: 20px; 
            height: 400px; 
            overflow-y: auto; 
            margin: 20px 0; 
            border-radius: 8px; 
        }
        .input-area { 
            display: flex; 
            gap: 10px; 
            margin: 20px 0; 
        }
        .select { 
            background: #21262d; 
            border: 1px solid #30363d; 
            color: #c9d1d9; 
            padding: 12px; 
            border-radius: 6px; 
            width: 150px; 
        }
        .input { 
            flex: 1; 
            background: #21262d; 
            border: 1px solid #30363d; 
            color: #c9d1d9; 
            padding: 12px; 
            border-radius: 6px; 
            font-size: 16px; 
            font-family: inherit;
        }
        .button { 
            background: #238636; 
            border: none; 
            color: white; 
            padding: 12px 24px; 
            border-radius: 6px; 
            cursor: pointer; 
            font-weight: bold; 
        }
        .button:hover { 
            background: #2ea043; 
        }
        .message { 
            margin: 10px 0; 
            padding: 12px; 
            border-radius: 6px; 
            border-left: 4px solid #7c3aed; 
            background: #161b22; 
        }
        .user-message { border-left-color: #58a6ff; }
        .ai-message { border-left-color: #7c3aed; }
        .error-message { border-left-color: #f85149; }
        .timestamp { font-size: 12px; opacity: 0.7; margin-right: 8px; }
    </style>
</head>
<body>
    <div class="header">
        <h1>🔥 Tmux Claude Pool</h1>
        <p>Real Claude instances in tmux sessions</p>
        <div id="status">Loading...</div>
    </div>
    
    <div id="instances" class="instances">
        <!-- Instance cards will be populated here -->
    </div>
    
    <div class="chat" id="chat">
        <div class="message">
            <span class="timestamp">Starting...</span>
            🔥 Connecting to tmux Claude instances...
        </div>
    </div>
    
    <div class="input-area">
        <select id="instanceSelect" class="select">
            <option value="GeneralAI">GeneralAI</option>
            <option value="CodeAI">CodeAI</option>
            <option value="PlannerAI">PlannerAI</option>
        </select>
        <input type="text" id="taskInput" class="input" 
               placeholder="Send task to real Claude instance in tmux..." 
               onkeypress="if(event.key==='Enter') sendTask()">
        <button class="button" onclick="sendTask()">SEND TO TMUX</button>
    </div>

    <script>
        let isLoaded = false;
        
        async function loadStatus() {
            try {
                const response = await fetch('/status');
                const data = await response.json();
                
                document.getElementById('status').innerHTML = \`
                    📊 Instances: \${data.instances.length} | 
                    📞 Requests: \${data.costs.requests} | 
                    📁 Pool: \${data.poolDir}
                \`;
                
                const instancesDiv = document.getElementById('instances');
                instancesDiv.innerHTML = '';
                
                data.instances.forEach(instance => {
                    const card = document.createElement('div');
                    card.className = 'instance-card';
                    card.innerHTML = \`
                        <h3>\${instance.name}</h3>
                        <div><strong>Tmux Session:</strong> \${instance.sessionName}</div>
                        <div><strong>Requests:</strong> \${instance.requests}</div>
                        <div><strong>Last Response:</strong> \${instance.lastResponse || 'None'}</div>
                    \`;
                    instancesDiv.appendChild(card);
                });
                
                if (!isLoaded) {
                    addMessage('🟢 Connected to tmux Claude pool', 'message');
                    isLoaded = true;
                }
            } catch (error) {
                addMessage('❌ Failed to load status: ' + error.message, 'error-message');
            }
        }
        
        async function sendTask() {
            const instanceSelect = document.getElementById('instanceSelect');
            const taskInput = document.getElementById('taskInput');
            
            const instance = instanceSelect.value;
            const task = taskInput.value.trim();
            
            if (!task) return;
            
            addMessage(\`👤 → \${instance}: \${task}\`, 'user-message');
            addMessage('🔄 Sending to tmux session...', 'message');
            
            try {
                const url = \`/ask?instance=\${encodeURIComponent(instance)}&task=\${encodeURIComponent(task)}\`;
                const response = await fetch(url);
                const data = await response.json();
                
                if (data.error) {
                    addMessage(\`❌ Error: \${data.error}\`, 'error-message');
                } else {
                    addMessage(\`🤖 \${data.instance} (tmux:\${data.tmux_session}): \${data.result}\`, 'ai-message');
                }
            } catch (error) {
                addMessage(\`❌ Request failed: \${error.message}\`, 'error-message');
            }
            
            taskInput.value = '';
            loadStatus(); // Refresh status
        }
        
        function addMessage(text, className) {
            const chat = document.getElementById('chat');
            const messageDiv = document.createElement('div');
            messageDiv.className = 'message ' + className;
            
            const timestamp = new Date().toLocaleTimeString();
            messageDiv.innerHTML = '<span class="timestamp">' + timestamp + '</span>' + text;
            
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        }
        
        // Load status on page load and refresh every 10 seconds
        loadStatus();
        setInterval(loadStatus, 10000);
    </script>
</body>
</html>`;
  }
}

// Launch if run directly
if (require.main === module) {
  new TmuxClaudePool();
}

module.exports = TmuxClaudePool;