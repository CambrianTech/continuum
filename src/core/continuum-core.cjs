#!/usr/bin/env node
/**
 * CONTINUUM CORE - Streamlined Coordination Layer
 * ~400 lines focused on AI coordination and task routing
 */

const HttpServer = require('../integrations/HttpServer.cjs');
const WebSocketServer = require('../integrations/WebSocketServer.cjs');
const SimpleMenuBar = require('../integrations/SimpleMenuBar.cjs');
const CostTracker = require('./CostTracker.cjs');
const { ModelRegistry } = require('./AIModel.cjs');
const CommandProcessor = require('./CommandProcessor.cjs');
const UIGenerator = require('../ui/UIGenerator.cjs');
const ProtocolSheriff = require('./ProtocolSheriff.cjs');
const ModelCaliber = require('./ModelCaliber.cjs');
const VersionManager = require('./VersionManager.cjs');
const { BrowserAdapter } = require('../adapters/BrowserAdapter.cjs');
const ScreenshotService = require('../services/ScreenshotService.cjs');
const { Anthropic } = require('@anthropic-ai/sdk');
const { OpenAI } = require('openai');
const fs = require('fs');
const path = require('path');
const os = require('os');

// Load environment variables from .continuum hierarchy
// Structure: shared team configs -> user-specific configs (user wins)
const username = require('os').userInfo().username;

function findRepoRoot() {
  let dir = process.cwd();
  while (dir !== path.dirname(dir)) {
    if (fs.existsSync(path.join(dir, '.git'))) {
      return dir;
    }
    dir = path.dirname(dir);
  }
  return null;
}

const repoRoot = findRepoRoot();
const homeContinuumDir = path.join(os.homedir(), '.continuum');
const repoContinuumDir = repoRoot ? path.join(repoRoot, '.continuum') : null;
const localContinuumDir = path.join(process.cwd(), '.continuum');

// Load configs in priority order: home -> repo/shared -> repo/user -> local/shared -> local/user
const configPaths = [
  path.join(homeContinuumDir, 'config.env'),                                    // Global user config
  repoContinuumDir ? path.join(repoContinuumDir, 'config.env') : null,         // Repo shared config
  repoContinuumDir ? path.join(repoContinuumDir, 'shared', 'config.env') : null, // Repo shared config
  repoContinuumDir ? path.join(repoContinuumDir, 'users', username, 'config.env') : null, // Repo user config
  path.join(localContinuumDir, 'config.env'),                                  // Local shared config
  path.join(localContinuumDir, 'shared', 'config.env'),                        // Local shared config
  path.join(localContinuumDir, 'users', username, 'config.env')                // Local user config
].filter(Boolean);

configPaths.forEach(configFile => {
  if (fs.existsSync(configFile)) {
    require('dotenv').config({ path: configFile, override: true });
  }
});

class ContinuumCore {
  constructor(options = {}) {
    this.sessions = new Map();
    this.port = options.port || process.env.CONTINUUM_PORT || 9000;
    this.username = username;
    this.isRestart = options.isRestart || false;
    
    // Set up .continuum hierarchy
    this.homeContinuumDir = homeContinuumDir;
    this.repoContinuumDir = repoContinuumDir;
    this.localContinuumDir = localContinuumDir;
    
    // User-specific storage (for private data like chat logs, costs)
    this.userDataDir = path.join(this.homeContinuumDir, 'users', this.username);
    
    this.costTracker = new CostTracker(path.join(this.userDataDir, 'costs.json'));
    this.modelRegistry = new ModelRegistry();
    this.modelCaliber = new ModelCaliber();
    this.commandProcessor = new CommandProcessor();
    this.commandProcessor.continuum = this;
    this.uiGenerator = new UIGenerator(this);
    this.protocolSheriff = new ProtocolSheriff(this.modelRegistry, this.modelCaliber);
    this.versionManager = new VersionManager(this);
    this.browserAdapter = new BrowserAdapter();
    this.screenshotService = new ScreenshotService(this);
    
    // WebSocket management
    this.conversationThreads = new Map();
    this.activeConnections = new Map();
    this.registeredProjects = new Map(); // Track all registered projects
    
    // AI clients (only initialize if API keys are available)
    if (process.env.ANTHROPIC_API_KEY) {
      try {
        this.anthropic = new Anthropic({ apiKey: process.env.ANTHROPIC_API_KEY });
        console.log('✅ Anthropic API client initialized');
      } catch (error) {
        console.log('⚠️ Anthropic API client initialization failed:', error.message);
      }
    } else {
      console.log('ℹ️ ANTHROPIC_API_KEY not found - AI features will be limited');
    }
    
    if (process.env.OPENAI_API_KEY) {
      try {
        this.openai = new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
        console.log('✅ OpenAI API client initialized');
      } catch (error) {
        console.log('⚠️ OpenAI API client initialization failed:', error.message);
      }
    } else {
      console.log('ℹ️ OPENAI_API_KEY not found - AI features will be limited');
    }
    
    this.ensureContinuumDir();
    if (options.skipMerge !== true) {
      this.mergeContinuumDirectories();
    }
    this.loadConversationHistory();
    this.setupGracefulShutdown();
  }

  ensureContinuumDir() {
    // Ensure user data directory exists
    if (!fs.existsSync(this.userDataDir)) {
      fs.mkdirSync(this.userDataDir, { recursive: true });
      console.log(`📁 Created user data directory: ${this.userDataDir}`);
    }
    
    // Create .continuum structure from template if needed
    if (this.localContinuumDir && !fs.existsSync(this.localContinuumDir)) {
      this.createContinuumFromTemplate(this.localContinuumDir);
    }
  }

  createContinuumFromTemplate(targetDir) {
    console.log(`📋 Creating .continuum structure...`);
    
    // Create directory structure
    const sharedDir = path.join(targetDir, 'shared');
    const userDir = path.join(targetDir, 'users', this.username);
    
    fs.mkdirSync(sharedDir, { recursive: true });
    fs.mkdirSync(userDir, { recursive: true });
    
    // Create files from templates (not copy - generate from canonical source)
    this.createFileFromTemplate(targetDir, '.gitignore');
    this.createFileFromTemplate(targetDir, 'config.env');
    this.createFileFromTemplate(targetDir, 'README.md');
    this.createFileFromTemplate(sharedDir, 'models.json');
    
    console.log(`✅ .continuum structure created`);
    console.log(`📝 Add your API keys to users/${this.username}/config.env`);
  }

  createFileFromTemplate(targetDir, filename) {
    const templateDir = path.join(__dirname, '../../templates/continuum-structure');
    const templateFile = path.join(templateDir, filename);
    const targetFile = path.join(targetDir, filename);
    
    if (fs.existsSync(templateFile) && !fs.existsSync(targetFile)) {
      const content = fs.readFileSync(templateFile, 'utf-8');
      fs.writeFileSync(targetFile, content);
    }
  }

  mergeContinuumDirectories() {
    console.log('🔄 Merging all .continuum directories...');
    
    const allDirs = [
      this.localContinuumDir,
      this.repoContinuumDir,
      this.homeContinuumDir
    ].filter(Boolean);
    
    const mergedFiles = new Map();
    
    // Collect all files from all .continuum directories
    allDirs.forEach(dir => {
      if (fs.existsSync(dir)) {
        console.log(`📁 Scanning: ${dir}`);
        const files = fs.readdirSync(dir);
        
        files.forEach(file => {
          const filePath = path.join(dir, file);
          const stats = fs.statSync(filePath);
          
          if (stats.isFile()) {
            // Intelligent merging based on file type and content
            if (file.endsWith('.jsonl')) {
              // JSONL files: merge chronologically, dedupe by content
              if (!mergedFiles.has(file)) {
                mergedFiles.set(file, new Set());
              }
              try {
                const content = fs.readFileSync(filePath, 'utf-8');
                const lines = content.trim().split('\n').filter(line => line.trim());
                lines.forEach(line => mergedFiles.get(file).add(line));
              } catch (error) {
                console.log(`⚠️  Could not read ${filePath}`);
              }
            }
            else if (file.endsWith('.json')) {
              // JSON files: intelligent merge based on content structure
              if (!mergedFiles.has(file)) {
                mergedFiles.set(file, {});
              }
              try {
                const content = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
                const existing = mergedFiles.get(file);
                
                // Smart merge for specific file types
                if (file === 'costs.json') {
                  // Costs: sum totals, merge by model
                  existing.total = (existing.total || 0) + (content.total || 0);
                  existing.requests = (existing.requests || 0) + (content.requests || 0);
                  existing.byModel = { ...existing.byModel, ...content.byModel };
                } else if (file === 'projects.json') {
                  // Projects: merge project entries, update timestamps
                  Object.assign(existing, content);
                } else {
                  // Default: merge objects
                  Object.assign(existing, content);
                }
              } catch (error) {
                console.log(`⚠️  Could not parse JSON ${filePath}`);
              }
            }
            else if (file === 'config.env') {
              // ENV files: merge environment variables, later directories override
              if (!mergedFiles.has(file)) {
                mergedFiles.set(file, new Map());
              }
              try {
                const content = fs.readFileSync(filePath, 'utf-8');
                const envMap = mergedFiles.get(file);
                
                content.split('\n').forEach(line => {
                  line = line.trim();
                  if (line && !line.startsWith('#')) {
                    const [key, ...valueParts] = line.split('=');
                    if (key && valueParts.length > 0) {
                      envMap.set(key.trim(), valueParts.join('=').trim());
                    }
                  }
                });
              } catch (error) {
                console.log(`⚠️  Could not read ${filePath}`);
              }
            }
            else {
              // Other files: use highest priority (later directories win)
              try {
                const content = fs.readFileSync(filePath, 'utf-8');
                mergedFiles.set(file, content);
              } catch (error) {
                console.log(`⚠️  Could not read ${filePath}`);
              }
            }
          }
        });
      }
    });
    
    // Write intelligently merged files to home .continuum directory
    mergedFiles.forEach((content, filename) => {
      const outputPath = path.join(this.userDataDir, filename);
      
      try {
        if (filename.endsWith('.jsonl') && content instanceof Set) {
          // Convert Set to Array, sort by timestamp if possible, write unique lines
          const lines = Array.from(content);
          const sortedLines = lines.sort((a, b) => {
            try {
              const aTime = JSON.parse(a).timestamp;
              const bTime = JSON.parse(b).timestamp;
              return new Date(aTime) - new Date(bTime);
            } catch {
              return 0; // Keep original order if can't parse timestamps
            }
          });
          fs.writeFileSync(outputPath, sortedLines.join('\n') + '\n');
          console.log(`✅ Merged ${sortedLines.length} unique entries into ${filename}`);
        }
        else if (filename.endsWith('.json') && typeof content === 'object') {
          fs.writeFileSync(outputPath, JSON.stringify(content, null, 2));
          console.log(`✅ Intelligently merged ${filename}`);
        }
        else if (filename === 'config.env' && content instanceof Map) {
          // Convert Map back to .env format
          const envLines = Array.from(content.entries()).map(([key, value]) => `${key}=${value}`);
          fs.writeFileSync(outputPath, envLines.join('\n') + '\n');
          console.log(`✅ Merged ${content.size} environment variables into ${filename}`);
        }
        else {
          fs.writeFileSync(outputPath, content);
          console.log(`✅ Copied ${filename}`);
        }
      } catch (error) {
        console.log(`❌ Failed to write merged ${filename}: ${error.message}`);
      }
    });
    
    console.log(`🎯 Merged ${mergedFiles.size} files from ${allDirs.length} .continuum directories`);
  }

  registerProject() {
    const projectRegistryFile = path.join(this.globalContinuumDir, 'projects.json');
    const currentProjectPath = process.cwd();
    const currentProjectName = path.basename(currentProjectPath);
    
    let projects = {};
    
    // Load existing project registry
    if (fs.existsSync(projectRegistryFile)) {
      try {
        projects = JSON.parse(fs.readFileSync(projectRegistryFile, 'utf-8'));
      } catch (error) {
        console.log('⚠️  Could not load project registry, creating new one');
      }
    }
    
    // Register current project
    projects[currentProjectName] = {
      path: currentProjectPath,
      lastAccessed: new Date().toISOString(),
      continuumDir: this.projectContinuumDir
    };
    
    // Save updated registry
    try {
      fs.writeFileSync(projectRegistryFile, JSON.stringify(projects, null, 2));
      console.log(`📝 Registered project: ${currentProjectName}`);
    } catch (error) {
      console.log('⚠️  Could not save project registry');
    }
    
    this.projectRegistry = projects;
  }

  loadConversationHistory() {
    this.conversationHistory = [];
    this.currentProject = path.basename(process.cwd());
    
    // Load from user-specific directory (private data)
    const historyFile = path.join(this.userDataDir, 'conversation-history.jsonl');
    
    if (fs.existsSync(historyFile)) {
      try {
        const content = fs.readFileSync(historyFile, 'utf-8');
        const lines = content.trim().split('\n').filter(line => line.trim());
        this.conversationHistory = lines.slice(-30).map(line => JSON.parse(line));
        console.log(`📚 Loaded ${this.conversationHistory.length} conversation messages for ${this.username}`);
      } catch (error) {
        console.log('⚠️  Could not load conversation history');
      }
    }
  }

  setupGracefulShutdown() {
    const cleanup = () => {
      console.log('\\n🧹 Shutting down Continuum...');
      this.costTracker.saveData();
      if (this.server) {
        this.server.close();
      }
      process.exit(0);
    };

    process.on('SIGTERM', cleanup);
    process.on('SIGINT', cleanup);
    process.on('uncaughtException', (error) => {
      console.error('❌ Uncaught exception:', error);
      cleanup();
    });
  }

  async start() {
    console.log(`📝 PID: ${process.pid} (saved to ${path.join(this.localContinuumDir, 'continuum.pid')})`);
    
    // Save PID for process management
    fs.writeFileSync(path.join(this.localContinuumDir, 'continuum.pid'), process.pid.toString());

    // Check if another Continuum web console is already running
    const isPortInUse = await this.checkPortInUse(this.port);
    
    if (isPortInUse) {
      // Register this project with the existing web console
      console.log(`🔗 Connecting to existing Continuum web console at http://localhost:${this.port}`);
      console.log(`📁 Project: ${process.cwd()}`);
      console.log(`💾 Using local scratchpad: ${this.localContinuumDir}`);
      
      await this.registerWithExistingConsole();
      
      // Start minimal background service (no web server)
      console.log(`✅ Registered with unified web console`);
      console.log(`🌐 Opening: http://localhost:${this.port}`);
      
      // Open/refresh the web interface
      await this.openWebInterface();
      
      return;
    }

    // Start the unified web console (first instance)
    console.log(`🚀 Starting unified Continuum web console`);
    
    const httpServer = new HttpServer(this);
    this.server = httpServer.createServer();
    this.webSocketServer = new WebSocketServer(this, this.server);

    // Use a Promise to handle async server startup with proper error handling
    return new Promise((resolve, reject) => {
      this.server.on('error', (error) => {
        console.error('❌ Server error:', error);
        reject(error);
      });

      this.server.listen(this.port, async () => {
        console.log(`🌐 Unified Continuum Console: http://localhost:${this.port}`);
        console.log(`💬 Talk to real Claude instances`);
        console.log(`📊 Track costs and sessions across all projects`);
        console.log(`📁 Master project: ${process.cwd()}`);
        
        // Start version monitoring
        this.versionManager.startVersionChecking();
        
        // Initialize simple menu bar integration - click to open web portal
        this.systemTray = new SimpleMenuBar(this);
        
        // Open the web interface automatically
        await this.openWebInterface();
        
        resolve();
      });
    });
  }

  async checkPortInUse(port) {
    return new Promise((resolve) => {
      const server = require('net').createServer();
      
      server.listen(port, () => {
        server.once('close', () => resolve(false));
        server.close();
      });
      
      server.on('error', () => resolve(true));
    });
  }

  async registerWithExistingConsole() {
    try {
      const fetch = (await import('node-fetch')).default;
      
      const projectInfo = {
        workingDirectory: process.cwd(),
        projectName: path.basename(process.cwd()),
        scratchpadPath: this.localContinuumDir,
        pid: process.pid,
        registeredAt: new Date().toISOString()
      };

      const response = await fetch(`http://localhost:${this.port}/api/projects/register`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(projectInfo)
      });

      if (response.ok) {
        const result = await response.json();
        console.log(`✅ Project registered: ${result.projectName}`);
      } else {
        console.log(`⚠️ Failed to register with existing console, but proceeding...`);
      }
    } catch (error) {
      console.log(`⚠️ Could not connect to existing console: ${error.message}`);
    }
  }

  registerProject(projectInfo) {
    const projectId = `${projectInfo.projectName}_${projectInfo.pid}`;
    this.registeredProjects.set(projectId, {
      ...projectInfo,
      id: projectId,
      lastActive: new Date().toISOString()
    });
    
    console.log(`📁 Registered project: ${projectInfo.projectName} (${projectInfo.workingDirectory})`);
    return { projectId, projectName: projectInfo.projectName };
  }

  getRegisteredProjects() {
    return Array.from(this.registeredProjects.values());
  }

  async openWebInterface() {
    const url = `http://localhost:${this.port}`;
    
    // For restarts, use browser-level tab management (WebSocket tabs are stale)
    if (this.isRestart) {
      console.log('🔄 Restart detected, managing browser tabs...');
      
      // Try to focus existing browser tabs first (don't close them)
      const focused = await this.browserAdapter.focusExistingTab(url);
      if (focused) {
        console.log('🎯 Focused existing browser tab');
        return;
      }
      
      // If no existing tabs found, open new one
      console.log('📱 No existing tabs found, opening new tab...');
      await this.openNewTab(url);
      return;
    }
    
    try {
      // For normal startup, try WebSocket tab management first
      if (this.webSocketServer && await this.focusExistingTabs()) {
        console.log('🎯 Focused existing browser tab via WebSocket');
        return;
      }
      
      // Fall back to OS-level tab management
      await this.refreshExistingTab(url);
    } catch (error) {
      // Only open new tab if no existing tabs found
      console.log('📱 Opening new browser tab...');
      await this.openNewTab(url);
    }
  }

  async openNewTab(url) {
    // Simple system default browser opening - always use this first
    const { spawn } = require('child_process');
    
    try {
      let command, args;
      
      switch (process.platform) {
        case 'darwin':
          command = 'open';
          args = [url];
          break;
        case 'win32':
          command = 'start';
          args = ['', url];
          break;
        default:
          command = 'xdg-open';
          args = [url];
      }
      
      spawn(command, args, { detached: true, stdio: 'ignore' });
      console.log(`🌐 Opened with system default browser`);
      return true;
      
    } catch (error) {
      console.log(`⚠️ System default failed, trying BrowserAdapter...`);
      // Fallback to BrowserAdapter if system default fails
      try {
        return await this.browserAdapter.openNewTab(url);
      } catch (adapterError) {
        console.log(`⚠️ Could not open browser. Please visit: ${url}`);
        return false;
      }
    }
  }

  async refreshExistingTab(url) {
    // Try BrowserAdapter first for advanced tab management
    try {
      const focused = await this.browserAdapter.focusExistingTab(url);
      if (focused) {
        return true;
      }
    } catch (error) {
      // Continue to fallback
    }
    
    // Fallback to opening new tab
    throw new Error('No existing tabs found');
  }

  async focusExistingTabs() {
    if (!this.webSocketServer?.tabManager) {
      return false;
    }
    
    const tabStatus = this.webSocketServer.getTabStatus();
    if (tabStatus.activeTabs > 0) {
      // Focus the first active tab
      const firstActiveTab = tabStatus.tabs.find(tab => tab.isActive);
      if (firstActiveTab) {
        return await this.webSocketServer.tabManager.focusTab(firstActiveTab.tabId);
      }
    }
    
    return false;
  }

  // Browser management now handled by BrowserAdapter

  generateUI() {
    return this.uiGenerator.generateHTML();
  }

  async intelligentRoute(task) {
    console.log(`🧠 INTELLIGENCE_ROUTE: Enhanced intelligent routing: ${task.substring(0, 100)}...`);
    console.log(`🧠 INTELLIGENCE_ROUTE: Full task string: "${task}"`);
    console.log(`🧠 INTELLIGENCE_ROUTE: Task includes [CMD:? ${task.includes('[CMD:')}`);
    
    // UNIVERSAL BUS COMMAND DETECTION: Check for direct commands first
    if (task.includes('[CMD:')) {
      console.log(`🚀 BUS_COMMAND_DETECTED: Processing direct command instead of AI routing`);
      console.log(`🚀 BUS_COMMAND_DETECTED: Task contains [CMD: - proceeding with command parsing`);
      
      // Parse command using CommandProcessor protocol parsing
      const parsed = this.commandProcessor.parseAIProtocol(task);
      console.log(`🚀 BUS_COMMAND_DETECTED: Parsed result:`, JSON.stringify(parsed, null, 2));
      
      if (parsed.commands.length > 0) {
        console.log(`🎯 BUS_COMMAND_EXECUTE: Executing ${parsed.commands.length} bus command(s) directly`);
        console.log(`🎯 BUS_COMMAND_EXECUTE: Commands:`, parsed.commands);
        
        const results = [];
        for (const cmd of parsed.commands) {
          console.log(`🎯 BUS_COMMAND_EXECUTE: Processing command: ${cmd.command} with params: ${cmd.params}`);
          try {
            const commandResult = await this.commandProcessor.executeCommand(cmd.command, cmd.params);
            console.log(`✅ BUS_COMMAND_SUCCESS: Command ${cmd.command} result:`, commandResult);
            results.push({
              command: cmd.command,
              params: cmd.params,
              result: commandResult
            });
          } catch (error) {
            console.error(`❌ BUS_COMMAND_ERROR: Command ${cmd.command} failed:`, error.message);
            console.error(`❌ BUS_COMMAND_ERROR: Stack:`, error.stack);
            results.push({
              command: cmd.command,
              params: cmd.params,
              error: error.message
            });
          }
        }
        
        console.log(`🎯 BUS_COMMAND_COMPLETE: Returning results:`, results);
        
        // Return bus command results in standard format
        return {
          result: results.length === 1 ? results[0] : results,
          role: 'BusCommand',
          type: 'bus_command_execution'
        };
      } else {
        console.log(`❌ BUS_COMMAND_PARSE_FAILED: No commands found in parsed result`);
      }
    } else {
      console.log(`🧠 INTELLIGENCE_ROUTE: No [CMD: detected, proceeding with AI routing`);
    }
    
    // Check if we have any AI models available for non-command tasks
    if (!this.modelRegistry || this.modelRegistry.getAvailableModels().length === 0) {
      throw new Error('No AI models available. Please configure ANTHROPIC_API_KEY or OPENAI_API_KEY.');
    }
    
    // Determine best agent for the task
    const role = this.selectRole(task);
    console.log(`🎯 Strategic/complex task - routing to ${role}...`);
    
    const result = await this.sendTask(role, task);
    return {
      result: result,
      role: role,
      coordination: false
    };
  }

  selectRole(task) {
    const taskLower = task.toLowerCase();
    
    // Simple routing logic - can be enhanced with ML
    if (taskLower.includes('code') || taskLower.includes('debug') || taskLower.includes('function')) {
      return 'CodeAI';
    } else if (taskLower.includes('plan') || taskLower.includes('strategy') || taskLower.includes('coordinate')) {
      return 'PlannerAI';
    } else {
      return 'PlannerAI'; // Default to PlannerAI for most tasks
    }
  }

  getInitialAgent(task) {
    return this.selectRole(task);
  }

  async sendTask(role, task) {
    console.log(`📤 SEND_TASK: Routing to ${role} - "${task.substring(0, 80)}${task.length > 80 ? '...' : ''}"`);
    
    if (!this.sessions.has(role)) {
      console.log(`🆕 Creating new ${role} session...`);
      await this.createInstance(role);
    }

    const response = await this.callAI(role, task);
    
    // Track costs per session
    const session = this.sessions.get(role);
    session.requests++;
    session.cost += response.cost;
    
    console.log(`✅ ${role} completed task - response length: ${response.result.length} chars`);
    return response.result;
  }

  async createInstance(role) {
    console.log(`🚀 Creating ${role} instance...`);
    
    this.sessions.set(role, {
      role: role,
      created: new Date(),
      requests: 0,
      cost: 0,
      conversation: []
    });
    
    console.log(`✅ ${role} ready`);
  }

  async callAI(role, prompt) {
    try {
      console.log(`🔄 Calling ${role} with task...`);
      console.log(`🔄 ${role} processing: ${prompt.substring(0, 50)}...`);
      
      const model = this.modelRegistry.findBestModel(prompt, role);
      let response, usage;
      
      if (model.provider === 'anthropic') {
        const completion = await this.anthropic.messages.create({
          model: model.name,
          max_tokens: model.maxTokens,
          messages: [{ role: "user", content: this.buildSystemPrompt(role, true) + "\\n\\n" + prompt }],
        });
        
        response = completion.content[0].text;
        usage = completion.usage;
      } else if (model.provider === 'openai') {
        const completion = await this.openai.chat.completions.create({
          model: model.name,
          messages: [{ role: "user", content: this.buildSystemPrompt(role, true) + "\\n\\n" + prompt }],
          max_tokens: model.maxTokens,
          temperature: 0.7,
        });
        
        response = completion.choices[0].message.content;
        usage = completion.usage;
      }
      
      // Calculate cost using model
      const cost = model.calculateCost(usage.input_tokens || usage.prompt_tokens, 
                                     usage.output_tokens || usage.completion_tokens);
      
      // Process any commands in the response
      const toolResults = await this.commandProcessor.processToolCommands(response);
      if (toolResults.length > 0) {
        console.log(`🔧 Executed ${toolResults.length} tools, sending callback to ${role}...`);
        
        // Send results back to AI for final response
        const callbackPrompt = `Tool execution results:\\n${toolResults.map(r => 
          `${r.tool}: ${r.result.substring(0, 500)}`
        ).join('\\n\\n')}\\n\\nPlease provide a final response based on these results.`;
        
        const callbackCompletion = await this.anthropic.messages.create({
          model: "claude-3-5-sonnet-20241022",
          max_tokens: 1000,
          messages: [{ role: "user", content: callbackPrompt }],
        });
        
        response = callbackCompletion.content[0].text;
        const callbackCost = model.calculateCost(callbackCompletion.usage.input_tokens, 
                                               callbackCompletion.usage.output_tokens);
        
        // Track total cost including callback
        this.costTracker.trackCost(model.name, 
          (usage.input_tokens || usage.prompt_tokens) + callbackCompletion.usage.input_tokens,
          (usage.output_tokens || usage.completion_tokens) + callbackCompletion.usage.output_tokens,
          cost + callbackCost);
      } else {
        // Track regular cost
        this.costTracker.trackCost(model.name, 
          usage.input_tokens || usage.prompt_tokens,
          usage.output_tokens || usage.completion_tokens,
          cost);
      }
      
      // Add to conversation history
      this.addToHistory('User', prompt);
      this.addToHistory(role, response);
      
      console.log(`✅ ${role} responded: ${response.substring(0, 50)}...`);
      return { result: response, cost: cost };
    } catch (error) {
      console.error(`❌ ${role} call failed: ${error.message}`);
      throw new Error(`${role} call failed: ${error.message}`);
    }
  }

  buildSystemPrompt(role, includeHistory = false) {
    let basePrompt = `You are ${role}, an AI agent in the Continuum multi-agent coordination system.`;
    
    // Add current project context
    basePrompt += `\n\nCurrent project: ${this.currentProject} (${process.cwd()})`;
    
    // Add conversation history for context
    if (includeHistory && this.conversationHistory.length > 0) {
      const recentHistory = this.conversationHistory.slice(-5).map(entry => {
        const projectInfo = entry.project ? ` [${entry.project}]` : '';
        return `${entry.role}${projectInfo}: ${entry.message.substring(0, 100)}`;
      }).join('\n');
      basePrompt += `\n\nRecent conversation history:\n${recentHistory}\n\n`;
    }
    
    if (role === 'CodeAI') {
      return basePrompt + `You specialize in code analysis, debugging, and implementation. Focus on technical accuracy and best practices.`;
    } else if (role === 'PlannerAI') {
      return basePrompt + `You coordinate tasks, create strategies, and can execute commands using the AI protocol.
      
Available commands:
[CMD:EXEC] shell_command - Execute shell commands
[CMD:FILE_READ] file_path - Read file contents  
[CMD:FILE_WRITE] file_path content - Write content to file
[CMD:WEBFETCH] url - Fetch web content

Use [STATUS] for progress updates and [CHAT] for final responses.`;
    } else {
      return basePrompt + `You provide general assistance and coordinate with other AI agents as needed. You maintain conversational continuity and remember previous interactions.`;
    }
  }

  addToHistory(role, message) {
    const entry = {
      timestamp: new Date().toISOString(),
      role: role,
      message: message.substring(0, 500),
      project: this.currentProject,
      projectPath: process.cwd()
    };
    
    this.conversationHistory.push(entry);
    if (this.conversationHistory.length > 50) {
      this.conversationHistory = this.conversationHistory.slice(-40);
    }
    
    // Save to user-specific history file (private to user)
    try {
      const historyFile = path.join(this.userDataDir, 'conversation-history.jsonl');
      fs.appendFileSync(historyFile, JSON.stringify(entry) + '\n');
    } catch (error) {
      console.error('Failed to save conversation history:', error.message);
    }
  }

  // Getter for costs (used by UI and WebSocket)
  get costs() {
    return {
      total: this.costTracker.getTotal(),
      requests: this.costTracker.getRequests()
    };
  }
}

// Export for use as module
module.exports = ContinuumCore;

// Run directly if called as script
if (require.main === module) {
  const continuum = new ContinuumCore();
  continuum.start().catch(error => {
    console.error('❌ Failed to start Continuum:', error);
    process.exit(1);
  });
}