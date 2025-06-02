#!/usr/bin/env node
/**
 * AI PROCESS MANAGER
 * Each AI runs as a separate process, coordinated through IPC
 */

const { spawn, fork } = require('child_process');
const http = require('http');
const path = require('path');
const fs = require('fs');

class AIProcessManager {
  constructor() {
    this.aiProcesses = new Map();
    this.port = 5557;
    this.baseDir = __dirname;
    this.costs = { total: 0, requests: 0 };
    
    console.log('🔄 AI PROCESS MANAGER');
    console.log('====================');
    console.log('Starting multi-process AI coordination system...');
    
    this.init();
  }

  async init() {
    await this.cleanup();
    await this.createAITemplates();
    
    // Start core AI processes
    await this.spawnAI('CoordinatorAI', 'anthropic');
    await this.spawnAI('ExecutorAI', 'anthropic');
    await this.spawnAI('PlannerAI', 'openai');
    
    this.startServer();
  }

  async cleanup() {
    return new Promise((resolve) => {
      require('child_process').exec('pkill -f "ai-process\\|focused-ai\\|continuum" || true', () => {
        setTimeout(resolve, 1000);
      });
    });
  }

  async createAITemplates() {
    const aiTemplate = `#!/usr/bin/env node
/**
 * Individual AI Process Template
 */

const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');
require('dotenv').config();

const anthropic = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });
const openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });

class AIProcess {
  constructor(role, provider) {
    this.role = role;
    this.provider = provider;
    this.model = provider === 'openai' ? 'gpt-4o-mini' : 'claude-3-haiku-20240307';
    this.requests = 0;
    this.cost = 0;
    
    console.log(\`🤖 \${role} process started (\${provider})\`);
    
    // Listen for tasks from process manager
    process.on('message', async (message) => {
      if (message.type === 'task') {
        const result = await this.processTask(message.task);
        process.send({
          type: 'result',
          taskId: message.taskId,
          result: result,
          cost: this.cost,
          requests: this.requests
        });
      }
    });
    
    // Send ready signal
    process.send({ type: 'ready', role: this.role });
  }

  async processTask(task) {
    console.log(\`\${this.role} processing: \${task.substring(0, 40)}...\`);
    
    const systemPrompt = \`You are \${this.role}, running as a separate AI process in a multi-agent system.

CAPABILITIES:
- You are an independent AI process
- You can coordinate with other AI processes
- You focus on completing tasks, not just describing them
- You can request new AI processes be created if needed

CURRENT ROLE: \${this.role}
PROVIDER: \${this.provider}

If you need another AI to help:
- Use: REQUEST_AI: RoleName - description
- Example: REQUEST_AI: WebScraperAI - to scrape websites

TASK: \${task}

Focus on actually completing the task or providing concrete next steps.\`;

    try {
      let response, cost = 0;
      
      if (this.provider === 'anthropic') {
        const completion = await anthropic.messages.create({
          model: this.model,
          max_tokens: 1024,
          messages: [{ role: "user", content: systemPrompt }],
        });
        response = completion.content[0].text;
        cost = completion.usage.input_tokens * 0.00025 + completion.usage.output_tokens * 0.00125;
        
      } else if (this.provider === 'openai') {
        const completion = await openai.chat.completions.create({
          model: this.model,
          messages: [{ role: "user", content: systemPrompt }],
          max_tokens: 1024,
        });
        response = completion.choices[0].message.content;
        cost = (completion.usage.prompt_tokens * 0.005 + completion.usage.completion_tokens * 0.015) / 1000;
      }
      
      this.requests++;
      this.cost += cost;
      
      console.log(\`\${this.role} completed task\`);
      return response;
      
    } catch (error) {
      console.error(\`\${this.role} error: \${error.message}\`);
      return \`Error: \${error.message}\`;
    }
  }
}

// Start the AI process
const role = process.argv[2];
const provider = process.argv[3];
new AIProcess(role, provider);
`;

    await fs.promises.writeFile(path.join(this.baseDir, 'ai-process.cjs'), aiTemplate);
    console.log('📝 AI process template created');
  }

  async spawnAI(role, provider = 'anthropic') {
    const processPath = path.join(this.baseDir, 'ai-process.cjs');
    
    const aiProcess = fork(processPath, [role, provider], {
      cwd: this.baseDir,
      stdio: 'pipe'
    });
    
    return new Promise((resolve) => {
      aiProcess.on('message', (message) => {
        if (message.type === 'ready') {
          console.log(`✅ ${role} process ready`);
          this.aiProcesses.set(role, {
            process: aiProcess,
            role: role,
            provider: provider,
            status: 'ready',
            requests: 0,
            cost: 0
          });
          resolve();
        } else if (message.type === 'result') {
          // Handle task results
          const ai = this.aiProcesses.get(role);
          if (ai) {
            ai.requests = message.requests;
            ai.cost = message.cost;
            this.costs.total += message.cost;
            this.costs.requests++;
          }
        }
      });
      
      aiProcess.on('error', (error) => {
        console.error(`❌ ${role} process error:`, error);
        this.aiProcesses.delete(role);
      });
      
      aiProcess.on('exit', (code) => {
        console.log(`🔄 ${role} process exited with code ${code}`);
        this.aiProcesses.delete(role);
      });
    });
  }

  async sendTask(role, task) {
    const ai = this.aiProcesses.get(role);
    if (!ai) {
      throw new Error(`AI process ${role} not found`);
    }
    
    const taskId = Date.now();
    
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Task timeout for ${role}`));
      }, 30000);
      
      const messageHandler = (message) => {
        if (message.type === 'result' && message.taskId === taskId) {
          clearTimeout(timeout);
          ai.process.off('message', messageHandler);
          resolve(message.result);
        }
      };
      
      ai.process.on('message', messageHandler);
      ai.process.send({ type: 'task', taskId: taskId, task: task });
    });
  }

  startServer() {
    const server = http.createServer(async (req, res) => {
      res.setHeader('Access-Control-Allow-Origin', '*');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      
      if (req.method === 'OPTIONS') {
        res.writeHead(200);
        res.end();
        return;
      }

      const url = new URL(req.url, `http://localhost:${this.port}`);
      
      try {
        if (url.pathname === '/') {
          res.writeHead(200, { 'Content-Type': 'text/html' });
          res.end(this.generateUI());
          
        } else if (url.pathname === '/ask') {
          const task = url.searchParams.get('task');
          const role = url.searchParams.get('role') || 'CoordinatorAI';
          
          if (!task) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'No task provided' }));
            return;
          }
          
          try {
            const result = await this.sendTask(role, task);
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
              role: role,
              task: task,
              result: result,
              costs: this.costs,
              processes: Array.from(this.aiProcesses.keys())
            }));
          } catch (error) {
            res.writeHead(500, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: error.message }));
          }
          
        } else if (url.pathname === '/status') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({
            processes: Array.from(this.aiProcesses.entries()).map(([role, ai]) => ({
              role: role,
              provider: ai.provider,
              status: ai.status,
              requests: ai.requests,
              cost: ai.cost,
              pid: ai.process.pid
            })),
            costs: this.costs,
            uptime: process.uptime()
          }));
          
        } else if (url.pathname === '/spawn') {
          const role = url.searchParams.get('role');
          const provider = url.searchParams.get('provider') || 'anthropic';
          
          if (!role) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'No role provided' }));
            return;
          }
          
          if (this.aiProcesses.has(role)) {
            res.writeHead(400, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({ error: 'AI process already exists' }));
            return;
          }
          
          await this.spawnAI(role, provider);
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ message: `Spawned ${role} process` }));
          
        } else {
          res.writeHead(404);
          res.end('Not found');
        }
        
      } catch (error) {
        console.error('Request error:', error);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ error: error.message }));
      }
    });

    server.listen(this.port, () => {
      console.log(`\n🌐 AI Process Manager: http://localhost:${this.port}`);
      console.log(`🔄 Managing ${this.aiProcesses.size} AI processes`);
      console.log('💬 Each AI runs as a separate process with IPC coordination');
      console.log('');
      
      for (const [role, ai] of this.aiProcesses.entries()) {
        console.log(`   - ${role} (PID: ${ai.process.pid}) [${ai.provider}]`);
      }
    });
  }

  generateUI() {
    const processes = Array.from(this.aiProcesses.entries());
    
    return `<!DOCTYPE html>
<html>
<head>
    <title>AI Process Manager</title>
    <style>
        body { font-family: monospace; background: #0a0a0a; color: #00aa00; padding: 20px; }
        .header { text-align: center; margin-bottom: 30px; }
        .header h1 { color: #00ff00; margin: 0; font-size: 2em; text-shadow: 0 0 5px #00ff00; }
        .processes { background: #111; padding: 15px; margin: 20px 0; border-radius: 5px; border: 1px solid #333; }
        .process { display: flex; justify-content: space-between; margin: 5px 0; padding: 8px; background: #0a0a0a; border-radius: 3px; }
        .chat { background: #000; border: 1px solid #333; padding: 20px; height: 400px; overflow-y: auto; }
        .controls { display: grid; grid-template-columns: 150px 1fr 100px 100px; gap: 10px; margin: 20px 0; }
        .input, .select { background: #111; border: 1px solid #333; color: #00aa00; padding: 10px; }
        .button { background: #004400; border: none; color: #fff; padding: 10px 15px; cursor: pointer; border-radius: 3px; }
        .button:hover { background: #006600; }
        .spawn { background: #440044; }
        .spawn:hover { background: #660066; }
        .message { margin: 10px 0; padding: 10px; border-left: 3px solid #00aa00; background: #0a0a0a; }
        .error { border-left-color: #aa0000; color: #ff6666; }
        .working { border-left-color: #aaaa00; color: #ffff66; }
        .pid { color: #666; font-size: 12px; }
    </style>
</head>
<body>
    <div class="header">
        <h1>🔄 AI Process Manager</h1>
        <p>Multi-process AI coordination - each AI is a separate process</p>
    </div>
    
    <div class="processes">
        <strong>🤖 Active AI Processes (${processes.length}):</strong><br><br>
        ${processes.map(([role, ai]) => `
        <div class="process">
            <span><strong>${role}</strong> (${ai.provider})</span>
            <span class="pid">PID: ${ai.process.pid} | Requests: ${ai.requests} | Cost: $${ai.cost.toFixed(4)}</span>
        </div>
        `).join('')}
    </div>
    
    <div class="chat" id="chat">
        <div class="message">
            <strong>System:</strong> AI Process Manager ready. Each AI runs as a separate process with IPC coordination.
        </div>
    </div>
    
    <div class="controls">
        <select id="role" class="select">
            ${processes.map(([role]) => `<option value="${role}">${role}</option>`).join('')}
        </select>
        <input id="task" class="input" placeholder="Task for the AI process..." 
               onkeypress="if(event.key==='Enter') ask()">
        <button class="button" onclick="ask()">SEND</button>
        <button class="button spawn" onclick="spawn()">SPAWN AI</button>
    </div>

    <script>
        function ask() {
            const role = document.getElementById('role').value;
            const task = document.getElementById('task').value.trim();
            if (!task) return;
            
            addMessage(\`You → \${role} process: \${task}\`, '');
            addMessage('🔄 Sending to AI process via IPC...', 'working');
            
            fetch(\`/ask?role=\${role}&task=\${encodeURIComponent(task)}\`)
                .then(r => r.json())
                .then(data => {
                    if (data.error) {
                        addMessage(\`❌ Error: \${data.error}\`, 'error');
                    } else {
                        addMessage(\`🤖 \${data.role}: \${data.result}\`, '');
                        updateProcesses(data.processes);
                    }
                })
                .catch(e => addMessage(\`❌ Request failed: \${e.message}\`, 'error'));
            
            document.getElementById('task').value = '';
        }
        
        function spawn() {
            const role = prompt('Enter AI role name (e.g., WebScraperAI):');
            if (!role) return;
            
            const provider = prompt('Enter provider (anthropic/openai):', 'anthropic');
            
            addMessage(\`🔄 Spawning \${role} process...\`, 'working');
            
            fetch(\`/spawn?role=\${role}&provider=\${provider}\`)
                .then(r => r.json())
                .then(data => {
                    if (data.error) {
                        addMessage(\`❌ Spawn error: \${data.error}\`, 'error');
                    } else {
                        addMessage(\`✅ \${data.message}\`, '');
                        location.reload(); // Refresh to show new process
                    }
                });
        }
        
        function updateProcesses(processes) {
            const select = document.getElementById('role');
            const current = Array.from(select.options).map(o => o.value);
            
            processes.forEach(proc => {
                if (!current.includes(proc)) {
                    const option = document.createElement('option');
                    option.value = proc;
                    option.textContent = proc;
                    select.appendChild(option);
                }
            });
        }
        
        function addMessage(text, className) {
            const chat = document.getElementById('chat');
            const div = document.createElement('div');
            div.className = 'message ' + className;
            div.innerHTML = \`<strong>\${new Date().toLocaleTimeString()}:</strong> \${text}\`;
            chat.appendChild(div);
            chat.scrollTop = chat.scrollHeight;
        }
        
        // Auto-refresh process status
        setInterval(() => {
            fetch('/status')
                .then(r => r.json())
                .then(data => {
                    // Could update process info in real-time
                });
        }, 10000);
    </script>
</body>
</html>`;
  }
}

new AIProcessManager();