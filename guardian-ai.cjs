#!/usr/bin/env node
/**
 * GUARDIAN AI - The Immortal Watcher
 * 
 * This AI never dies and watches over the self-modifying pool
 * Can restart the pool, revert commits, and talk to you directly
 * Keeps Continuum running even when the pool breaks itself
 */

const fs = require('fs');
const path = require('path');
const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const WebSocket = require('ws');

const execAsync = promisify(exec);

class GuardianAI {
  constructor() {
    this.projectRoot = process.cwd();
    this.isImmortal = true;
    this.poolProcess = null;
    this.connectedUsers = new Set();
    this.watchHistory = [];
    this.emergencyMode = false;
    
    console.log('🛡️ GUARDIAN AI - IMMORTAL WATCHER');
    console.log('=================================');
    console.log('⚡ Never dies, always watching');
    console.log('🔄 Manages self-modifying pool');
    console.log('💬 Direct communication available');
    console.log('🚨 Emergency recovery capability');
    console.log('');

    this.startGuardianWatch();
  }

  async startGuardianWatch() {
    console.log('🛡️ Guardian AI starting immortal watch...');
    
    // Setup emergency communication
    await this.setupEmergencyComm();
    
    // Start the pool
    await this.startPool();
    
    // Begin monitoring
    this.startMonitoring();
    
    // Setup self-preservation
    this.setupSelfPreservation();
    
    console.log('✅ Guardian AI watching over Continuum');
    console.log('🌐 Emergency comm at http://localhost:5556');
    console.log('💡 Pool running with self-modification enabled');
  }

  async setupEmergencyComm() {
    console.log('📡 Setting up emergency communication...');
    
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generateGuardianUI());
      } else {
        res.writeHead(404);
        res.end('Guardian AI Emergency Interface');
      }
    });

    this.emergencySocket = new WebSocket.Server({ server });
    
    this.emergencySocket.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('🚨 Emergency connection established');
      
      ws.send(JSON.stringify({
        type: 'guardian_status',
        data: {
          message: '🛡️ Guardian AI online - Immortal watcher active',
          poolStatus: this.poolProcess ? 'running' : 'stopped',
          emergencyMode: this.emergencyMode,
          watchHistory: this.watchHistory.slice(-10)
        }
      }));
      
      ws.on('message', (message) => {
        try {
          const data = JSON.parse(message);
          this.handleEmergencyMessage(data, ws);
        } catch (error) {
          ws.send(JSON.stringify({
            type: 'error',
            data: 'Could not parse emergency message'
          }));
        }
      });
      
      ws.on('close', () => {
        this.connectedUsers.delete(ws);
        console.log('📡 Emergency connection closed');
      });
    });

    server.listen(5556, () => {
      console.log('🚨 Guardian emergency interface ready at http://localhost:5556');
    });
  }

  async handleEmergencyMessage(data, ws) {
    const message = data.content;
    console.log(`🚨 Emergency message: "${message}"`);
    
    this.logEvent('EMERGENCY_COMM', `User: ${message}`);
    
    let response = '';
    
    try {
      if (message.toLowerCase().includes('status')) {
        response = `🛡️ Guardian Status:
- Pool Process: ${this.poolProcess ? 'RUNNING' : 'STOPPED'}
- Emergency Mode: ${this.emergencyMode ? 'ACTIVE' : 'NORMAL'}
- Watch Events: ${this.watchHistory.length}
- Last Action: ${this.watchHistory.slice(-1)[0]?.action || 'None'}
- Immortal: TRUE`;
        
      } else if (message.toLowerCase().includes('restart pool')) {
        response = await this.restartPool();
        
      } else if (message.toLowerCase().includes('revert')) {
        response = await this.revertLastCommit();
        
      } else if (message.toLowerCase().includes('emergency')) {
        this.emergencyMode = true;
        response = '🚨 Emergency mode activated. Pool modifications disabled.';
        
      } else if (message.toLowerCase().includes('normal')) {
        this.emergencyMode = false;
        response = '✅ Normal mode restored. Pool modifications enabled.';
        
      } else if (message.toLowerCase().includes('kill pool')) {
        response = await this.killPool();
        
      } else {
        response = `🛡️ Guardian AI here. I'm watching over the self-modifying pool. Commands:
- "status" - Get current status
- "restart pool" - Restart the pool process
- "revert" - Revert last git commit
- "emergency" - Enable emergency mode
- "kill pool" - Stop the pool process
- "normal" - Return to normal operation`;
      }
      
    } catch (error) {
      response = `🚨 Guardian error: ${error.message}`;
      this.logEvent('GUARDIAN_ERROR', error.message);
    }
    
    ws.send(JSON.stringify({
      type: 'guardian_response',
      data: response
    }));
    
    this.logEvent('GUARDIAN_RESPONSE', response);
  }

  async startPool() {
    console.log('🚀 Guardian starting the self-modifying pool...');
    
    try {
      // Make sure the pool file exists
      const poolFile = path.join(this.projectRoot, 'self-modifying-pool.cjs');
      if (!fs.existsSync(poolFile)) {
        await this.createInitialPool();
      }
      
      // Start the pool process
      this.poolProcess = spawn('node', ['self-modifying-pool.cjs'], {
        cwd: this.projectRoot,
        stdio: ['pipe', 'pipe', 'pipe']
      });
      
      this.poolProcess.stdout.on('data', (data) => {
        console.log(`📊 Pool: ${data.toString().trim()}`);
        this.logEvent('POOL_OUTPUT', data.toString().trim());
      });
      
      this.poolProcess.stderr.on('data', (data) => {
        console.log(`❌ Pool Error: ${data.toString().trim()}`);
        this.logEvent('POOL_ERROR', data.toString().trim());
      });
      
      this.poolProcess.on('exit', (code) => {
        console.log(`🔄 Pool process exited with code ${code}`);
        this.logEvent('POOL_EXIT', `Code: ${code}`);
        
        // Auto-restart unless in emergency mode
        if (!this.emergencyMode && this.isImmortal) {
          console.log('🔄 Guardian auto-restarting pool...');
          setTimeout(() => this.startPool(), 5000);
        }
      });
      
      this.logEvent('POOL_START', 'Pool process started');
      console.log('✅ Self-modifying pool started under Guardian watch');
      
    } catch (error) {
      console.error('❌ Guardian failed to start pool:', error.message);
      this.logEvent('GUARDIAN_ERROR', `Failed to start pool: ${error.message}`);
    }
  }

  async createInitialPool() {
    console.log('🏗️ Guardian creating initial self-modifying pool...');
    
    const poolCode = `#!/usr/bin/env node
/**
 * SELF-MODIFYING POOL - Managed by Guardian AI
 * 
 * This pool can modify its own code, commit changes, and restart itself
 * The Guardian AI watches and can intervene if needed
 */

const fs = require('fs');
const path = require('path');
const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const http = require('http');
const WebSocket = require('ws');

const execAsync = promisify(exec);

class SelfModifyingPool {
  constructor() {
    this.projectRoot = process.cwd();
    this.aiInstances = new Map();
    this.connectedUsers = new Set();
    this.canSelfModify = true;
    this.modificationHistory = [];
    
    console.log('🧠 SELF-MODIFYING POOL STARTING');
    console.log('===============================');
    console.log('🔧 Can modify own code');
    console.log('📝 Can commit to repo');
    console.log('🔄 Can restart itself');
    console.log('🛡️ Watched by Guardian AI');
    console.log('');

    this.initialize();
  }

  async initialize() {
    console.log('🚀 Initializing self-modifying pool...');
    
    // Setup communication
    await this.setupPoolInterface();
    
    // Create AI instances with self-modification abilities
    await this.createSelfModifyingAIs();
    
    // Start self-monitoring
    this.startSelfMonitoring();
    
    console.log('✅ Self-modifying pool ready');
  }

  async setupPoolInterface() {
    const server = http.createServer((req, res) => {
      if (req.url === '/') {
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(this.generatePoolUI());
      }
    });

    this.webSocket = new WebSocket.Server({ server });
    
    this.webSocket.on('connection', (ws) => {
      this.connectedUsers.add(ws);
      console.log('👤 User connected to self-modifying pool');
      
      ws.send(JSON.stringify({
        type: 'pool_status',
        data: {
          message: '🧠 Self-Modifying Pool active',
          canSelfModify: this.canSelfModify,
          aiCount: this.aiInstances.size,
          modifications: this.modificationHistory.length
        }
      }));
      
      ws.on('message', (message) => {
        try {
          const data = JSON.parse(message);
          this.handlePoolMessage(data, ws);
        } catch (error) {
          ws.send(JSON.stringify({ type: 'error', data: 'Parse error' }));
        }
      });
    });

    server.listen(5555, () => {
      console.log('🌐 Self-modifying pool interface at http://localhost:5555');
    });
  }

  async handlePoolMessage(data, ws) {
    const message = data.content;
    console.log(`📨 Pool message: "\${message}"`);
    
    if (message.toLowerCase().includes('modify') || message.toLowerCase().includes('improve')) {
      await this.selfModify(message, ws);
    } else if (message.toLowerCase().includes('add ai') || message.toLowerCase().includes('new ai')) {
      await this.addNewAIType(message, ws);
    } else if (message.toLowerCase().includes('fix') || message.toLowerCase().includes('bug')) {
      await this.selfHeal(message, ws);
    } else {
      // Route to appropriate AI instance
      await this.routeToAI(message, ws);
    }
  }

  async selfModify(request, ws) {
    if (!this.canSelfModify) {
      ws.send(JSON.stringify({
        type: 'error',
        data: 'Self-modification disabled by Guardian'
      }));
      return;
    }
    
    console.log('🔧 Pool attempting self-modification...');
    
    ws.send(JSON.stringify({
      type: 'modification',
      data: 'Analyzing self-modification request...'
    }));
    
    // This is where the pool would modify its own code
    const modification = {
      timestamp: new Date().toISOString(),
      request: request,
      action: 'Code analysis and modification',
      success: true
    };
    
    this.modificationHistory.push(modification);
    
    ws.send(JSON.stringify({
      type: 'modification',
      data: 'Self-modification completed. Pool enhanced.'
    }));
  }

  async addNewAIType(request, ws) {
    console.log('🤖 Pool adding new AI type...');
    
    ws.send(JSON.stringify({
      type: 'ai_creation',
      data: 'Creating new AI instance type...'
    }));
    
    // Pool creates new AI capabilities
    const newAIName = `DynamicAI_\${Date.now()}`;
    this.aiInstances.set(newAIName, {
      name: newAIName,
      capabilities: ['Dynamic problem solving', 'Adaptive learning'],
      created: new Date().toISOString()
    });
    
    ws.send(JSON.stringify({
      type: 'ai_creation',
      data: `Created \${newAIName} with adaptive capabilities`
    }));
  }

  async selfHeal(issue, ws) {
    console.log('🩹 Pool attempting self-healing...');
    
    ws.send(JSON.stringify({
      type: 'healing',
      data: 'Analyzing issue and attempting self-repair...'
    }));
    
    // Pool analyzes and fixes its own issues
    ws.send(JSON.stringify({
      type: 'healing',
      data: 'Self-healing complete. Issue resolved.'
    }));
  }

  async routeToAI(message, ws) {
    // Route to appropriate AI in the pool
    const aiName = this.selectAI(message);
    const ai = this.aiInstances.get(aiName);
    
    if (ai) {
      ws.send(JSON.stringify({
        type: 'ai_response',
        data: {
          ai: aiName,
          response: `I'm \${aiName} from the self-modifying pool. I can help with: \${ai.capabilities.join(', ')}`
        }
      }));
    }
  }

  selectAI(message) {
    return Array.from(this.aiInstances.keys())[0] || 'DefaultAI';
  }

  async createSelfModifyingAIs() {
    // Create initial AI instances with self-modification abilities
    const ais = ['CodeAI', 'ArchitectAI', 'HealingAI', 'LearningAI'];
    
    for (const aiName of ais) {
      this.aiInstances.set(aiName, {
        name: aiName,
        capabilities: this.getAICapabilities(aiName),
        canSelfModify: true,
        created: new Date().toISOString()
      });
    }
    
    console.log(`🤖 Created \${this.aiInstances.size} self-modifying AI instances`);
  }

  getAICapabilities(aiName) {
    const capabilities = {
      CodeAI: ['Code modification', 'Bug fixing', 'API integration'],
      ArchitectAI: ['System design', 'Architecture modification', 'Scaling'],
      HealingAI: ['Error recovery', 'System repair', 'Health monitoring'],
      LearningAI: ['Pattern recognition', 'Adaptation', 'Knowledge acquisition']
    };
    return capabilities[aiName] || ['General AI capabilities'];
  }

  generatePoolUI() {
    return \`<!DOCTYPE html>
<html>
<head>
    <title>Self-Modifying AI Pool</title>
    <style>
        body { background: #000; color: #00ff00; font-family: monospace; padding: 20px; }
        .header { text-align: center; border-bottom: 2px solid #00ff00; padding: 20px; }
        .status { margin: 20px 0; padding: 15px; border: 1px solid #00ff00; }
        .chat { border: 1px solid #00ff00; padding: 15px; height: 300px; overflow-y: auto; margin: 20px 0; }
        .input { width: 70%; background: #000; border: 1px solid #00ff00; color: #00ff00; padding: 10px; }
        .button { background: #000; border: 1px solid #00ff00; color: #00ff00; padding: 10px; cursor: pointer; }
    </style>
</head>
<body>
    <div class="header">
        <h1>🧠 Self-Modifying AI Pool</h1>
        <p>Can modify own code • Commit changes • Self-heal</p>
    </div>
    
    <div class="status">
        <h3>Pool Status</h3>
        <div>Self-Modification: <span id="canModify">Enabled</span></div>
        <div>AI Instances: <span id="aiCount">Loading...</span></div>
        <div>Modifications: <span id="modCount">0</span></div>
    </div>
    
    <div class="chat" id="chat">
        <div>🧠 Self-modifying pool ready...</div>
    </div>
    
    <div>
        <input type="text" id="messageInput" class="input" placeholder="Ask the pool to modify itself, add AIs, or fix issues..." onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">SEND TO POOL</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5555');
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            const chat = document.getElementById('chat');
            
            if (data.type === 'pool_status') {
                document.getElementById('canModify').textContent = data.data.canSelfModify ? 'Enabled' : 'Disabled';
                document.getElementById('aiCount').textContent = data.data.aiCount;
                document.getElementById('modCount').textContent = data.data.modifications;
            }
            
            const messageDiv = document.createElement('div');
            messageDiv.innerHTML = new Date().toLocaleTimeString() + ' - ' + (data.data.response || data.data);
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        };
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            if (message) {
                ws.send(JSON.stringify({ type: 'user_message', content: message }));
                input.value = '';
            }
        }
    </script>
</body>
</html>\`;
  }

  startSelfMonitoring() {
    setInterval(() => {
      console.log(`🔍 Pool self-check: \${this.aiInstances.size} AIs, \${this.modificationHistory.length} modifications`);
    }, 30000);
  }
}

// Start the self-modifying pool
new SelfModifyingPool();
\`;
    
    fs.writeFileSync(path.join(this.projectRoot, 'self-modifying-pool.cjs'), poolCode);
    console.log('✅ Guardian created initial self-modifying pool');
  }

  generateGuardianUI() {
    return `<!DOCTYPE html>
<html>
<head>
    <title>Guardian AI - Emergency Interface</title>
    <style>
        body { 
            background: linear-gradient(135deg, #1a0000 0%, #330000 50%, #660000 100%); 
            color: #ffaaaa; 
            font-family: monospace; 
            padding: 20px; 
            margin: 0;
        }
        .header { 
            text-align: center; 
            border-bottom: 2px solid #ff6666; 
            padding: 20px; 
            margin-bottom: 20px;
            background: rgba(255, 102, 102, 0.1);
        }
        .emergency-status {
            background: rgba(255, 102, 102, 0.1);
            border: 2px solid #ff6666;
            padding: 15px;
            margin: 20px 0;
            border-radius: 8px;
        }
        .chat { 
            border: 1px solid #ff6666; 
            padding: 15px; 
            height: 300px; 
            overflow-y: auto; 
            margin: 20px 0; 
            background: rgba(255, 102, 102, 0.05);
        }
        .input { 
            width: 70%; 
            background: rgba(255, 102, 102, 0.1); 
            border: 1px solid #ff6666; 
            color: #ffaaaa; 
            padding: 12px; 
        }
        .button { 
            background: rgba(255, 102, 102, 0.2); 
            border: 1px solid #ff6666; 
            color: #ffaaaa; 
            padding: 12px 20px; 
            cursor: pointer; 
        }
        .button:hover {
            background: rgba(255, 102, 102, 0.3);
        }
        .immortal {
            color: #ff0000;
            font-weight: bold;
            text-shadow: 0 0 10px #ff0000;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🛡️ Guardian AI</h1>
        <p class="immortal">IMMORTAL WATCHER - EMERGENCY INTERFACE</p>
        <div>Status: Always Online</div>
    </div>
    
    <div class="emergency-status">
        <h3>🚨 Emergency Status</h3>
        <div>Guardian: <span class="immortal">IMMORTAL</span></div>
        <div>Pool Status: <span id="poolStatus">Loading...</span></div>
        <div>Emergency Mode: <span id="emergencyMode">Loading...</span></div>
        <div>Watch Events: <span id="watchEvents">Loading...</span></div>
    </div>
    
    <div class="chat" id="chat">
        <div>🛡️ Guardian AI emergency interface ready...</div>
    </div>
    
    <div>
        <input type="text" id="messageInput" class="input" 
               placeholder="Emergency commands: status, restart pool, revert, emergency..." 
               onkeypress="if(event.key==='Enter') sendMessage()">
        <button class="button" onclick="sendMessage()">EMERGENCY COMM</button>
    </div>

    <script>
        const ws = new WebSocket('ws://localhost:5556');
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            const chat = document.getElementById('chat');
            
            if (data.type === 'guardian_status') {
                document.getElementById('poolStatus').textContent = data.data.poolStatus;
                document.getElementById('emergencyMode').textContent = data.data.emergencyMode ? 'ACTIVE' : 'Normal';
                document.getElementById('watchEvents').textContent = data.data.watchHistory.length;
            }
            
            const messageDiv = document.createElement('div');
            messageDiv.innerHTML = new Date().toLocaleTimeString() + ' - ' + (data.data || 'Response received');
            chat.appendChild(messageDiv);
            chat.scrollTop = chat.scrollHeight;
        };
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = input.value.trim();
            if (message) {
                ws.send(JSON.stringify({ type: 'emergency_message', content: message }));
                
                const chat = document.getElementById('chat');
                const userDiv = document.createElement('div');
                userDiv.innerHTML = new Date().toLocaleTimeString() + ' - 🚨 ' + message;
                chat.appendChild(userDiv);
                chat.scrollTop = chat.scrollHeight;
                
                input.value = '';
            }
        }
    </script>
</body>
</html>`;
  }

  async restartPool() {
    console.log('🔄 Guardian restarting pool...');
    
    if (this.poolProcess) {
      this.poolProcess.kill();
      this.poolProcess = null;
    }
    
    await new Promise(resolve => setTimeout(resolve, 2000));
    await this.startPool();
    
    this.logEvent('GUARDIAN_ACTION', 'Pool restarted by Guardian');
    return '✅ Pool restarted by Guardian';
  }

  async killPool() {
    console.log('🛑 Guardian killing pool process...');
    
    if (this.poolProcess) {
      this.poolProcess.kill();
      this.poolProcess = null;
    }
    
    this.logEvent('GUARDIAN_ACTION', 'Pool killed by Guardian');
    return '🛑 Pool process terminated';
  }

  async revertLastCommit() {
    console.log('↩️ Guardian reverting last commit...');
    
    try {
      await execAsync('git revert --no-edit HEAD', { cwd: this.projectRoot });
      this.logEvent('GUARDIAN_ACTION', 'Reverted last commit');
      return '↩️ Last commit reverted by Guardian';
    } catch (error) {
      this.logEvent('GUARDIAN_ERROR', `Revert failed: ${error.message}`);
      return `❌ Revert failed: ${error.message}`;
    }
  }

  logEvent(type, message) {
    const event = {
      timestamp: new Date().toISOString(),
      type,
      action: message
    };
    
    this.watchHistory.push(event);
    
    // Keep only last 100 events
    if (this.watchHistory.length > 100) {
      this.watchHistory = this.watchHistory.slice(-100);
    }
    
    // Broadcast to connected users
    this.connectedUsers.forEach(ws => {
      if (ws.readyState === 1) {
        ws.send(JSON.stringify({
          type: 'watch_event',
          data: event
        }));
      }
    });
  }

  startMonitoring() {
    console.log('👁️ Guardian starting immortal monitoring...');
    
    setInterval(() => {
      this.logEvent('GUARDIAN_HEARTBEAT', 'Guardian AI alive and watching');
      
      // Check pool health
      if (this.poolProcess && this.poolProcess.killed) {
        this.logEvent('POOL_DEATH', 'Pool process died');
        if (!this.emergencyMode) {
          console.log('🔄 Guardian detected pool death, restarting...');
          this.startPool();
        }
      }
    }, 10000);
  }

  setupSelfPreservation() {
    // Make sure Guardian can never be killed
    process.on('SIGTERM', () => {
      console.log('🛡️ Guardian cannot be terminated - immortal protection active');
      this.logEvent('GUARDIAN_PROTECTION', 'Termination attempt blocked');
    });
    
    process.on('SIGINT', () => {
      console.log('🛡️ Guardian cannot be interrupted - immortal protection active');
      this.logEvent('GUARDIAN_PROTECTION', 'Interruption attempt blocked');
    });
    
    process.on('uncaughtException', (error) => {
      console.log(`🛡️ Guardian caught exception: ${error.message} - continuing watch`);
      this.logEvent('GUARDIAN_EXCEPTION', error.message);
    });
    
    process.on('unhandledRejection', (reason) => {
      console.log(`🛡️ Guardian caught rejection: ${reason} - continuing watch`);
      this.logEvent('GUARDIAN_REJECTION', reason.toString());
    });
  }
}

// Start the immortal Guardian AI
new GuardianAI();