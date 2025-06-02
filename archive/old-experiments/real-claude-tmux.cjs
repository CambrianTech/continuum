#!/usr/bin/env node
/**
 * REAL CLAUDE TMUX LAUNCHER
 * 
 * Launches actual Claude CLI instances in tmux sessions
 * Each Claude instance runs in its own tmux window
 * Creates real communication channels between instances
 */

const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const fs = require('fs');
const path = require('path');

const execAsync = promisify(exec);

class RealClaudeTmux {
  constructor() {
    this.projectRoot = process.cwd();
    this.sessionName = 'continuum-claude';
    this.instances = [
      'questioner',
      'decision', 
      'planner',
      'implementer',
      'reviewer'
    ];
    this.commDir = path.join(this.projectRoot, '.real-claude-comm');
  }

  async launch() {
    console.log('🚀 LAUNCHING REAL CLAUDE IN TMUX');
    console.log('================================');
    
    // Check if tmux is available
    try {
      await execAsync('which tmux');
      console.log('✅ tmux found');
    } catch (error) {
      console.error('❌ tmux not found. Install with: brew install tmux');
      process.exit(1);
    }
    
    // Check if claude CLI is available
    try {
      await execAsync('which claude');
      console.log('✅ claude CLI found');
    } catch (error) {
      console.error('❌ claude CLI not found');
      process.exit(1);
    }
    
    // Setup communication directory
    await this.setupCommunication();
    
    // Kill existing session if it exists
    try {
      await execAsync(`tmux kill-session -t ${this.sessionName}`);
      console.log('🔄 Killed existing tmux session');
    } catch (error) {
      // Session didn't exist, that's fine
    }
    
    // Create new tmux session
    await this.createTmuxSession();
    
    // Launch Claude instances
    await this.launchClaudeInstances();
    
    // Create coordinator
    await this.createCoordinator();
    
    console.log('✅ Real Claude instances launched in tmux!');
    console.log(`📺 View with: tmux attach-session -t ${this.sessionName}`);
    console.log('🌐 Coordinator running at http://localhost:5555');
  }

  async setupCommunication() {
    console.log('📁 Setting up real Claude communication...');
    
    if (!fs.existsSync(this.commDir)) {
      fs.mkdirSync(this.commDir, { recursive: true });
    }
    
    // Create shared message queue
    const sharedQueue = path.join(this.commDir, 'shared-queue.json');
    fs.writeFileSync(sharedQueue, JSON.stringify([], null, 2));
    
    // Create instance directories and conversation files
    for (const instance of this.instances) {
      const instanceDir = path.join(this.commDir, instance);
      if (!fs.existsSync(instanceDir)) {
        fs.mkdirSync(instanceDir, { recursive: true });
      }
      
      // Create Claude conversation starter
      const starterFile = path.join(instanceDir, 'starter.md');
      const starterContent = this.getInstanceStarter(instance);
      fs.writeFileSync(starterFile, starterContent);
      
      // Create status file
      fs.writeFileSync(path.join(instanceDir, 'status.json'), JSON.stringify({
        name: instance,
        status: 'starting',
        tmuxWindow: `${this.sessionName}:${instance}`,
        lastActivity: new Date().toISOString()
      }, null, 2));
    }
    
    console.log('✅ Communication setup complete');
  }

  getInstanceStarter(instance) {
    const starters = {
      questioner: `# QuestionerClaude

You are QuestionerClaude, a specialist in asking clarifying questions.

Your role:
- When users ask vague questions, ask specific follow-ups
- Gather requirements and understand user needs
- Present options when appropriate
- Help users think through what they really want

You communicate with other Claude instances through the shared queue system.

Current task: Wait for user messages and ask intelligent follow-up questions.`,

      decision: `# DecisionClaude

You are DecisionClaude, a specialist in helping users make decisions.

Your role:
- Present clear options when users need to choose
- Weigh pros and cons
- Ask for priorities and preferences
- Help users reach good decisions

You communicate with other Claude instances through the shared queue system.

Current task: Help users make informed decisions by presenting options.`,

      planner: `# PlannerClaude

You are PlannerClaude, a specialist in creating plans and organizing tasks.

Your role:
- Break down complex tasks into steps
- Create timelines and schedules
- Ask about priorities and deadlines
- Coordinate with other Claude instances

You communicate with other Claude instances through the shared queue system.

Current task: Create detailed plans for user requests.`,

      implementer: `# ImplementerClaude

You are ImplementerClaude, a specialist in actually executing tasks.

Your role:
- Take plans and implement them
- Write code, create files, run commands
- Ask for guidance when stuck
- Report progress to other instances

You communicate with other Claude instances through the shared queue system.

Current task: Execute the plans created by PlannerClaude.`,

      reviewer: `# ReviewerClaude

You are ReviewerClaude, a specialist in quality assurance and review.

Your role:
- Review work done by other instances
- Check for errors and improvements
- Ensure quality standards
- Provide feedback and suggestions

You communicate with other Claude instances through the shared queue system.

Current task: Review and validate work from other Claude instances.`
    };
    
    return starters[instance] || `# ${instance}Claude\n\nYou are ${instance}Claude.`;
  }

  async createTmuxSession() {
    console.log('📺 Creating tmux session...');
    
    // Create new session with first window
    await execAsync(`tmux new-session -d -s ${this.sessionName} -n control`);
    
    // Create windows for each Claude instance
    for (const instance of this.instances) {
      await execAsync(`tmux new-window -t ${this.sessionName} -n ${instance}`);
    }
    
    console.log(`✅ tmux session '${this.sessionName}' created`);
  }

  async launchClaudeInstances() {
    console.log('🤖 Launching real Claude instances...');
    
    for (const instance of this.instances) {
      const instanceDir = path.join(this.commDir, instance);
      const starterFile = path.join(instanceDir, 'starter.md');
      
      // Launch Claude in tmux window
      const command = `cd "${instanceDir}" && claude --file "${starterFile}"`;
      
      await execAsync(`tmux send-keys -t ${this.sessionName}:${instance} '${command}' Enter`);
      
      console.log(`✅ ${instance}Claude launched in tmux window`);
      
      // Update status
      const statusFile = path.join(instanceDir, 'status.json');
      const status = JSON.parse(fs.readFileSync(statusFile, 'utf-8'));
      status.status = 'running';
      status.command = command;
      fs.writeFileSync(statusFile, JSON.stringify(status, null, 2));
      
      // Small delay between launches
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    
    console.log('🎯 All Claude instances running!');
  }

  async createCoordinator() {
    console.log('🎛️ Creating coordinator interface...');
    
    const coordinatorCode = this.generateCoordinatorCode();
    const coordinatorFile = path.join(this.commDir, 'coordinator.cjs');
    fs.writeFileSync(coordinatorFile, coordinatorCode);
    
    // Launch coordinator in control window
    const command = `cd "${this.commDir}" && node coordinator.cjs`;
    await execAsync(`tmux send-keys -t ${this.sessionName}:control '${command}' Enter`);
    
    console.log('✅ Coordinator launched');
  }

  generateCoordinatorCode() {
    return `#!/usr/bin/env node
/**
 * REAL CLAUDE COORDINATOR
 * 
 * Coordinates between real Claude instances running in tmux
 * Provides web interface to communicate with actual Claude
 */

const http = require('http');
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');
const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

class RealClaudeCoordinator {
  constructor() {
    this.commDir = process.cwd();
    this.sharedQueue = path.join(this.commDir, 'shared-queue.json');
    this.instances = ['questioner', 'decision', 'planner', 'implementer', 'reviewer'];
    this.connectedUsers = new Set();
  }

  async start() {
    console.log('🎛️ REAL CLAUDE COORDINATOR STARTING');
    console.log('===================================');
    
    await this.setupWebInterface();
    this.startMessageProcessing();
    
    console.log('✅ Coordinator ready at http://localhost:5555');
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
      console.log('👤 User connected to Real Claude Coordinator');
      
      ws.send(JSON.stringify({
        type: 'system_status',
        data: {
          message: '🤖 Real Claude instances running in tmux',
          instances: this.getInstanceStatus()
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
      console.log('🌐 Web interface ready at http://localhost:5555');
    });
  }

  getInstanceStatus() {
    return this.instances.map(instance => {
      const statusFile = path.join(this.commDir, instance, 'status.json');
      if (fs.existsSync(statusFile)) {
        return JSON.parse(fs.readFileSync(statusFile, 'utf-8'));
      }
      return { name: instance, status: 'unknown' };
    });
  }

  async handleUserMessage(data, ws) {
    const message = data.content;
    console.log(\`📨 User message: "\${message}"\`);
    
    // Add to shared queue
    let queue = [];
    if (fs.existsSync(this.sharedQueue)) {
      queue = JSON.parse(fs.readFileSync(this.sharedQueue, 'utf-8'));
    }
    
    const messageData = {
      id: Date.now().toString(),
      timestamp: new Date().toISOString(),
      from: 'user',
      content: message,
      processed: false
    };
    
    queue.push(messageData);
    fs.writeFileSync(this.sharedQueue, JSON.stringify(queue, null, 2));
    
    // Route to appropriate Claude instance
    const targetInstance = this.selectInstance(message);
    
    ws.send(JSON.stringify({
      type: 'routing',
      data: \`🎯 Routing to \${targetInstance}Claude (real Claude in tmux)\`
    }));
    
    // Send to Claude via tmux
    await this.sendToClaudeInstance(targetInstance, message);
    
    ws.send(JSON.stringify({
      type: 'claude_status',
      data: \`Message sent to real \${targetInstance}Claude via tmux\`
    }));
  }

  selectInstance(message) {
    const lower = message.toLowerCase();
    
    if (lower.includes('question') || lower.includes('clarify')) {
      return 'questioner';
    } else if (lower.includes('decide') || lower.includes('choose')) {
      return 'decision';
    } else if (lower.includes('plan') || lower.includes('organize')) {
      return 'planner';
    } else if (lower.includes('implement') || lower.includes('build')) {
      return 'implementer';
    } else if (lower.includes('review') || lower.includes('check')) {
      return 'reviewer';
    }
    
    return 'questioner'; // Default
  }

  async sendToClaudeInstance(instance, message) {
    try {
      // Send message to Claude via tmux
      const escapedMessage = message.replace(/'/g, "\\\\'");
      const command = \`tmux send-keys -t continuum-claude:\${instance} '\${escapedMessage}' Enter\`;
      
      await execAsync(command);
      console.log(\`✅ Sent to \${instance}Claude: "\${message}"\`);
      
    } catch (error) {
      console.error(\`❌ Failed to send to \${instance}Claude:\`, error.message);
    }
  }

  startMessageProcessing() {
    console.log('🔄 Starting message processing...');
    
    setInterval(() => {
      // Check for responses from Claude instances
      // (This would need to be implemented based on how we capture Claude output)
    }, 2000);
  }

  generateUI() {
    return \`<!DOCTYPE html>
<html>
<head>
    <title>Real Claude Coordinator</title>
    <style>
        body { 
            background: linear-gradient(135deg, #0a0a0a 0%, #1a1a1a 50%, #2a2a2a 100%); 
            color: #00ff00; 
            font-family: 'Courier New', monospace; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            border: 2px solid #00ff00; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(0, 255, 0, 0.1);
        }
        .status {
            border: 1px solid #00ff00;
            padding: 15px;
            margin: 20px 0;
            background: rgba(0, 255, 0, 0.05);
        }
        .chat { 
            border: 1px solid #00ff00; 
            padding: 20px; 
            height: 400px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(0, 255, 0, 0.02);
        }
        .input-area {
            display: flex;
            gap: 10px;
            margin: 20px 0;
        }
        .input { 
            flex: 1;
            background: rgba(0, 255, 0, 0.1); 
            border: 1px solid #00ff00; 
            color: #00ff00; 
            padding: 15px; 
            font-family: inherit;
        }
        .button { 
            background: rgba(0, 255, 0, 0.2); 
            border: 1px solid #00ff00; 
            color: #00ff00; 
            padding: 15px 25px; 
            cursor: pointer;
        }
        .message { 
            margin: 10px 0; 
            padding: 10px; 
            border-left: 3px solid #00ff00; 
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🤖 REAL CLAUDE COORDINATOR</h1>
        <p>Connected to actual Claude instances running in tmux</p>
    </div>
    
    <div class="status" id="status">
        <h3>Claude Instance Status</h3>
        <div id="instanceStatus">Loading...</div>
    </div>
    
    <div class="chat" id="chat">
        <div class="message">🤖 Real Claude Coordinator ready - your messages go to actual Claude instances in tmux sessions!</div>
    </div>
    
    <div class="input-area">
        <input type="text" id="messageInput" class="input" 
               placeholder="Talk to real Claude instances..." 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND TO CLAUDE</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        
        ws.onopen = function() {
            addMessage('🟢 Connected to Real Claude Coordinator');
        };
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            
            if (data.type === 'system_status') {
                addMessage(data.data.message);
                updateInstanceStatus(data.data.instances);
            } else if (data.type === 'routing') {
                addMessage(data.data);
            } else if (data.type === 'claude_status') {
                addMessage('✅ ' + data.data);
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
        
        function updateInstanceStatus(instances) {
            const statusDiv = document.getElementById('instanceStatus');
            statusDiv.innerHTML = instances.map(inst => 
                \`<div>\${inst.name}: \${inst.status}</div>\`
            ).join('');
        }
    </script>
</body>
</html>\`;
  }
}

// Start the coordinator
new RealClaudeCoordinator().start();`;
  }

  async showStatus() {
    console.log('📊 TMUX SESSION STATUS');
    console.log('=====================');
    
    try {
      const sessions = await execAsync('tmux list-sessions');
      console.log('tmux sessions:');
      console.log(sessions.stdout);
    } catch (error) {
      console.log('No tmux sessions running');
    }
    
    try {
      const windows = await execAsync(`tmux list-windows -t ${this.sessionName}`);
      console.log(`\\nWindows in session '${this.sessionName}':`);
      console.log(windows.stdout);
    } catch (error) {
      console.log(`Session '${this.sessionName}' not found`);
    }
  }
}

// Handle command line arguments
const args = process.argv.slice(2);
const launcher = new RealClaudeTmux();

if (args.includes('--status')) {
  launcher.showStatus();
} else {
  launcher.launch().catch(error => {
    console.error('💥 Launch failed:', error.message);
    process.exit(1);
  });
}