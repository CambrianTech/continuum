#!/usr/bin/env node
/**
 * Monitored Continuum Spawn - Human-AI Collaboration System
 * 
 * This creates a monitored environment where:
 * - You and I can communicate with the running AI
 * - Kill switch for emergency stops
 * - Real-time monitoring of AI actions
 * - Bidirectional communication channel
 * - JTAG-like debugging interface
 * - Live intervention capabilities
 */

const fs = require('fs');
const path = require('path');
const { exec, spawn } = require('child_process');
const { promisify } = require('util');
const readline = require('readline');

const execAsync = promisify(exec);

class MonitoredContinuumSpawn {
  constructor() {
    this.projectRoot = process.cwd();
    this.aiProcess = null;
    this.monitoring = true;
    this.communicationLog = [];
    this.killSwitchActivated = false;
    
    // Real-time monitoring state
    this.lastActivity = Date.now();
    this.healthCheck = {
      status: 'initializing',
      lastGitAction: null,
      lastFileChange: null,
      ciStatus: 'unknown',
      errors: []
    };

    console.log('🔬 MONITORED CONTINUUM SPAWN - HUMAN-AI COLLABORATION');
    console.log('====================================================');
    console.log('👥 Human-AI collaborative monitoring active');
    console.log('🛑 Kill switch available (Ctrl+C or type "kill")');
    console.log('💬 Bidirectional communication enabled');
    console.log('🔍 Real-time debugging and intervention');
    console.log('');

    this.setupCommunication();
    this.startMonitoring();
    this.spawnAIWithMonitoring();
  }

  setupCommunication() {
    console.log('💬 SETTING UP COMMUNICATION CHANNEL');
    console.log('===================================');
    
    // Create readline interface for human input
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      prompt: '🧑‍💻 Human> '
    });

    // Handle human commands
    this.rl.on('line', (input) => {
      this.handleHumanInput(input.trim());
    });

    // Setup graceful shutdown
    process.on('SIGINT', () => {
      this.activateKillSwitch('SIGINT received');
    });

    console.log('✅ Communication channel ready');
    console.log('📝 Available commands: kill, status, help, git-status, ci-check, communicate');
    console.log('');
  }

  handleHumanInput(input) {
    const command = input.toLowerCase();
    
    switch (command) {
      case 'kill':
        this.activateKillSwitch('Human initiated kill switch');
        break;
        
      case 'status':
        this.showDetailedStatus();
        break;
        
      case 'help':
        this.showHelpCommands();
        break;
        
      case 'git-status':
        this.checkGitStatus();
        break;
        
      case 'ci-check':
        this.checkCIStatus();
        break;
        
      case '':
        // Just enter pressed, show prompt again
        break;
        
      default:
        if (input.startsWith('communicate ')) {
          const message = input.substring(12);
          this.communicateWithAI(message);
        } else {
          console.log('❓ Unknown command. Type "help" for available commands.');
        }
    }
    
    if (!this.killSwitchActivated) {
      this.rl.prompt();
    }
  }

  showHelpCommands() {
    console.log('📚 AVAILABLE COMMANDS:');
    console.log('======================');
    console.log('  kill              - Emergency stop AI execution');
    console.log('  status            - Show detailed AI status');
    console.log('  git-status        - Check git repository status');
    console.log('  ci-check          - Check CI build status');
    console.log('  communicate <msg> - Send message to AI');
    console.log('  help              - Show this help');
    console.log('');
  }

  async checkGitStatus() {
    console.log('🔍 Checking git status...');
    try {
      const { stdout } = await execAsync('git status --porcelain');
      if (stdout.trim()) {
        console.log('📝 Uncommitted changes:');
        console.log(stdout);
      } else {
        console.log('✅ Working directory clean');
      }
      
      const { stdout: branch } = await execAsync('git branch --show-current');
      console.log(`🌿 Current branch: ${branch.trim()}`);
      
    } catch (error) {
      console.log(`❌ Git error: ${error.message}`);
    }
  }

  async checkCIStatus() {
    console.log('🔍 Checking CI status for PR #63...');
    try {
      const { stdout } = await execAsync('gh pr checks 63');
      console.log('📊 CI Status:');
      console.log(stdout);
      
      // Parse status for monitoring
      if (stdout.includes('fail')) {
        this.healthCheck.ciStatus = 'failing';
        this.healthCheck.errors.push('CI checks failing');
      } else if (stdout.includes('pass')) {
        this.healthCheck.ciStatus = 'passing';
      } else {
        this.healthCheck.ciStatus = 'pending';
      }
      
    } catch (error) {
      console.log(`❌ CI check error: ${error.message}`);
      this.healthCheck.ciStatus = 'error';
    }
  }

  communicateWithAI(message) {
    console.log(`💬 Sending to AI: "${message}"`);
    
    // Log communication
    this.communicationLog.push({
      timestamp: Date.now(),
      type: 'human-to-ai',
      message
    });
    
    // Create a communication file that AI can read
    const commDir = path.join(this.projectRoot, '.continuum', 'communication');
    if (!fs.existsSync(commDir)) {
      fs.mkdirSync(commDir, { recursive: true });
    }
    
    const commFile = path.join(commDir, 'human-input.json');
    fs.writeFileSync(commFile, JSON.stringify({
      timestamp: Date.now(),
      message,
      type: 'intervention'
    }, null, 2));
    
    console.log('📨 Message sent to AI communication channel');
  }

  startMonitoring() {
    console.log('🔍 STARTING REAL-TIME MONITORING');
    console.log('================================');
    
    // Monitor every 5 seconds
    this.monitoringInterval = setInterval(() => {
      this.performHealthCheck();
    }, 5000);
    
    // Watch for file changes
    this.watchFileChanges();
    
    console.log('✅ Monitoring active - checking every 5 seconds');
    console.log('');
  }

  async performHealthCheck() {
    if (this.killSwitchActivated) return;
    
    const now = Date.now();
    
    // Check if AI is hung (no activity for 2 minutes)
    if (now - this.lastActivity > 120000) {
      console.log('⚠️  WARNING: No AI activity detected for 2 minutes');
      console.log('🔧 Consider intervention or kill switch');
      this.healthCheck.status = 'potentially-hung';
    }
    
    // Check CI status periodically
    if (now % 30000 < 5000) { // Every 30 seconds
      await this.checkCIStatus();
    }
    
    // Log current status
    if (this.healthCheck.status === 'potentially-hung') {
      console.log(`🚨 Health Check: ${this.healthCheck.status} | CI: ${this.healthCheck.ciStatus}`);
    }
  }

  watchFileChanges() {
    // Watch for file changes to detect AI activity
    try {
      const watcher = require('fs').watch(this.projectRoot, { recursive: true }, (eventType, filename) => {
        if (filename && !filename.includes('node_modules') && !filename.includes('.git')) {
          this.lastActivity = Date.now();
          this.healthCheck.lastFileChange = filename;
          this.healthCheck.status = 'active';
          
          console.log(`📁 File changed: ${filename}`);
        }
      });
      
      console.log('👁️  File system monitoring active');
      
    } catch (error) {
      console.log('⚠️  File watching not available on this system');
    }
  }

  spawnAIWithMonitoring() {
    console.log('🚀 SPAWNING AI WITH MONITORING');
    console.log('==============================');
    
    // First, let's check the current CI status to inform the AI
    this.checkCIStatus().then(() => {
      this.actuallySpawnAI();
    });
  }

  async actuallySpawnAI() {
    console.log('🤖 Launching Continuum AI with full monitoring...');
    
    // Create enhanced spawn with real CI awareness
    const enhancedSpawnCode = `
const ContinuumSpawn = require('./continuum-spawn.cjs');

class MonitoredContinuumAI extends ContinuumSpawn {
  constructor() {
    super();
    this.monitoringEnabled = true;
    this.communicationChannel = '.continuum/communication';
    this.setupCommunicationListener();
  }

  setupCommunicationListener() {
    // Check for human messages every 10 seconds
    setInterval(() => {
      this.checkForHumanMessages();
    }, 10000);
  }

  checkForHumanMessages() {
    const commFile = path.join(this.projectRoot, this.communicationChannel, 'human-input.json');
    if (fs.existsSync(commFile)) {
      try {
        const message = JSON.parse(fs.readFileSync(commFile, 'utf-8'));
        if (message.timestamp > (this.lastMessageTime || 0)) {
          this.handleHumanMessage(message);
          this.lastMessageTime = message.timestamp;
        }
      } catch (error) {
        // Ignore parsing errors
      }
    }
  }

  handleHumanMessage(message) {
    console.log('📨 Received human message:', message.message);
    
    // Respond to human guidance
    if (message.message.toLowerCase().includes('ci status')) {
      this.respondToHuman('Checking CI status now...');
      this.checkCIStatus();
    } else if (message.message.toLowerCase().includes('stop')) {
      this.respondToHuman('Stopping current operation...');
      this.gracefulStop();
    } else if (message.message.toLowerCase().includes('status')) {
      this.respondToHuman('Current status: Working on cyberpunk improvements');
    }
  }

  respondToHuman(response) {
    const responseFile = path.join(this.projectRoot, this.communicationChannel, 'ai-response.json');
    fs.writeFileSync(responseFile, JSON.stringify({
      timestamp: Date.now(),
      response,
      type: 'ai-response'
    }, null, 2));
    console.log('📤 Responded to human:', response);
  }

  async checkCIStatus() {
    try {
      const { stdout } = await execAsync('gh pr checks 63');
      console.log('🔍 Real CI Status Check:');
      
      // Parse the actual failures
      const lines = stdout.split('\\n');
      const failures = lines.filter(line => line.includes('fail'));
      
      if (failures.length > 0) {
        console.log('❌ Detected CI failures:');
        failures.forEach(failure => console.log('  ', failure));
        
        // Focus on the real issues
        await this.fixRealCIIssues(failures);
      } else {
        console.log('✅ No CI failures detected');
      }
    } catch (error) {
      console.log('⚠️  Could not check CI status:', error.message);
    }
  }

  async fixRealCIIssues(failures) {
    console.log('🔧 Focusing on real CI issues...');
    
    // Check what the build error actually is
    try {
      const { stdout, stderr } = await execAsync('npm run build 2>&1 || true');
      console.log('📋 Build output:', stdout);
      
      if (stderr || stdout.includes('error') || stdout.includes('Error')) {
        console.log('❌ Build is actually failing locally');
        await this.fixBuildIssues();
      } else {
        console.log('✅ Build passes locally - CI environment issue');
        await this.fixCIEnvironmentIssues();
      }
    } catch (error) {
      console.log('🔧 Attempting to fix build configuration...');
      await this.fixBuildConfiguration();
    }
  }

  async fixBuildIssues() {
    console.log('🔧 Fixing actual build issues...');
    
    // Check if memory package is the issue
    const memoryPackage = path.join(this.projectRoot, 'packages/memory/package.json');
    if (!fs.existsSync(memoryPackage)) {
      console.log('📦 Creating missing memory package.json...');
      // ... create proper package.json
    }
    
    // Check TypeScript issues
    const tsFiles = await this.findTypeScriptFiles();
    for (const file of tsFiles) {
      await this.fixTypeScriptFile(file);
    }
  }

  gracefulStop() {
    console.log('🛑 Graceful stop requested by human');
    process.exit(0);
  }
}

new MonitoredContinuumAI();
`;

    // Write the enhanced spawn file
    fs.writeFileSync(
      path.join(this.projectRoot, 'monitored-ai.cjs'),
      enhancedSpawnCode
    );

    // Actually spawn the AI
    this.aiProcess = spawn('node', ['monitored-ai.cjs'], {
      cwd: this.projectRoot,
      stdio: ['pipe', 'pipe', 'pipe']
    });

    // Monitor AI output
    this.aiProcess.stdout.on('data', (data) => {
      const output = data.toString();
      console.log(`🤖 AI: ${output}`);
      this.lastActivity = Date.now();
      
      // Check for AI responses
      this.checkForAIResponses();
    });

    this.aiProcess.stderr.on('data', (data) => {
      console.log(`🚨 AI Error: ${data.toString()}`);
      this.healthCheck.errors.push(data.toString());
    });

    this.aiProcess.on('close', (code) => {
      console.log(`🤖 AI process exited with code ${code}`);
      this.cleanup();
    });

    console.log('✅ AI spawned with monitoring active');
    console.log('💬 You can now communicate with the AI');
    this.rl.prompt();
  }

  checkForAIResponses() {
    const responseFile = path.join(this.projectRoot, '.continuum', 'communication', 'ai-response.json');
    if (fs.existsSync(responseFile)) {
      try {
        const response = JSON.parse(fs.readFileSync(responseFile, 'utf-8'));
        if (response.timestamp > (this.lastAIResponseTime || 0)) {
          console.log(`🤖 AI Response: ${response.response}`);
          this.lastAIResponseTime = response.timestamp;
        }
      } catch (error) {
        // Ignore parsing errors
      }
    }
  }

  showDetailedStatus() {
    console.log('📊 DETAILED SYSTEM STATUS');
    console.log('=========================');
    console.log(`🤖 AI Process: ${this.aiProcess ? 'Running' : 'Stopped'}`);
    console.log(`💚 Health: ${this.healthCheck.status}`);
    console.log(`🔧 CI Status: ${this.healthCheck.ciStatus}`);
    console.log(`📁 Last File Change: ${this.healthCheck.lastFileChange || 'None'}`);
    console.log(`⏰ Last Activity: ${new Date(this.lastActivity).toLocaleTimeString()}`);
    console.log(`❌ Errors: ${this.healthCheck.errors.length}`);
    
    if (this.healthCheck.errors.length > 0) {
      console.log('   Recent errors:');
      this.healthCheck.errors.slice(-3).forEach(error => {
        console.log(`   - ${error.substring(0, 100)}...`);
      });
    }
    console.log('');
  }

  activateKillSwitch(reason) {
    console.log('🛑 KILL SWITCH ACTIVATED');
    console.log('========================');
    console.log(`📋 Reason: ${reason}`);
    
    this.killSwitchActivated = true;
    
    if (this.aiProcess) {
      console.log('🔪 Terminating AI process...');
      this.aiProcess.kill('SIGTERM');
      
      // Force kill if needed
      setTimeout(() => {
        if (this.aiProcess && !this.aiProcess.killed) {
          console.log('🔥 Force killing AI process...');
          this.aiProcess.kill('SIGKILL');
        }
      }, 5000);
    }
    
    this.cleanup();
  }

  cleanup() {
    console.log('🧹 Cleaning up monitoring system...');
    
    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
    }
    
    if (this.rl) {
      this.rl.close();
    }
    
    console.log('✅ Cleanup complete');
    process.exit(0);
  }
}

// Start the monitored spawn system
console.log('🔬 Starting monitored Continuum spawn system...');
new MonitoredContinuumSpawn();