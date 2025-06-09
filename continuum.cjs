#!/usr/bin/env node
/**
 * CONTINUUM CLI - Main Entry Point
 * 
 * npm install -g continuum
 * continuum
 * 
 * Launches web interface with real Claude pool
 */

const fs = require('fs');
const path = require('path');

// Parse command line arguments
function parseArgs() {
  const args = process.argv.slice(2);
  const flags = new Set();
  const options = {};
  
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    
    if (arg.startsWith('--')) {
      const flagName = arg.slice(2);
      
      // Check if next arg is a value (doesn't start with -)
      if (i + 1 < args.length && !args[i + 1].startsWith('-')) {
        options[flagName] = args[i + 1];
        i++; // Skip next arg since we consumed it
      } else {
        flags.add(flagName);
      }
    } else if (arg.startsWith('-')) {
      flags.add(arg.slice(1));
    }
  }
  
  return { flags, options, args };
}

// Get package version
function getVersion() {
  try {
    const packagePath = path.join(__dirname, 'package.json');
    const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
    return packageJson.version;
  } catch (error) {
    return 'unknown';
  }
}

// Show help
function showHelp() {
  console.log(`
🔄 Continuum Academy v${getVersion()} - Revolutionary AI Workforce Construction

USAGE:
  continuum                    Start the Academy web interface
  continuum --version         Show version information
  continuum --help            Show this help message
  continuum --port <number>   Specify custom port (default: 9000)
  continuum --restart         Force restart the server (kill existing instance)
  continuum --daemon          Run as daemon (detached background process)
  continuum --idle-timeout <minutes>  Auto-shutdown after idle time (default: 30)

FEATURES:
  🎓 Academy adversarial training (Testing Droid vs Protocol Sheriff)
  🔬 LoRA adapter system (190,735x storage reduction)
  🏗️ Hierarchical specialization stacking
  🤝 Cross-scope persona sharing (project/user/organization)
  📊 Real-time cost tracking and session management
  💬 Multi-provider AI integration (OpenAI, Anthropic, HuggingFace)

DEBUGGING:
  🔍 Agent Debug Validation:
     cd /Users/joel/Development/ideem/vHSM/externals/continuum/python-client && source ../.continuum/venv/agents/bin/activate && python continuum_client.py Claude
     
     Creates AgentClientConnection and triggers BrowserClientConnection validation:
     • ✅ Remote JavaScript execution capability
     • ✅ Version reading from browser UI (v0.2.1987)  
     • ✅ Error/warning generation in browser console
     • ✅ Screenshot capture with full dark UI theme (187KB screenshots)
     • ✅ WebSocket communication between Python agents and browser
     • ✅ File saving to .continuum/screenshots/ directory
     
     Screenshots automatically capture the complete dark cyberpunk UI including 
     sidebar, chat area, and all interface elements.
     • Tests cross-client communication between browser and Python agent
  📸 AI screenshot capture & visual debugging (Promise Post Office System)

WEB INTERFACE:
  Navigate to http://localhost:9000 after starting

AGENT AUTOMATION:
  🛰️ Deep Space Probe Portal - Browser automation via WebSocket telemetry
  📸 Promise Post Office System - AI-driven screenshot capture & web control
  
  Quick Start:
    js-send 'console.log("Hello from agent!")'  # Send JavaScript to browser
    heal "Connection refused"                    # Auto-heal system issues
    
  Agent Scripts Directory: ./agent-scripts/
    📁 tools/python/     - Core automation tools (js-send, heal, etc.)
    📁 examples/         - Example scripts by category (jokes, diagnostics, fixes)
    📁 docs/            - Architecture and usage documentation
    
  Python Client Directory: ./python-client/
    📁 examples/         - Screenshot capture & UI debugging demos
    📁 continuum_client/ - WebSocket client for AI agents
    📁 tests/           - Comprehensive test suite (19/19 passing)
    
  Debugging Demos: ./python-client/examples/
    🎨 fix_ui_styling_with_feedback.py - Complete UI development workflow ⭐ NEW
    🪟 natural_glass_submenu_demo.py   - Star Trek TNG glass submenu demo
    📸 simple_screenshot.py           - Basic screenshot capture
    🔍 find_and_capture.py            - Smart element finding
    
  Learn more: 
    ./agent-scripts/README.md       - Agent automation documentation
    ./python-client/README.md       - Python WebSocket client guide  
    ./docs/DEBUGGING_UTILITIES.md   - UI debugging & validation utilities

WEBSOCKET AGENT CONNECTION:
  🔌 Direct agent connection (like telnet/ssh for AI agents)
  
  Connection:
    ws://localhost:9000             # WebSocket endpoint (default port)
    
  On Connect:
    - Receives connection banner with available commands
    - Dynamic command list from loaded modules
    - Usage examples and agent information
    - Session management and timeout info
    
  Message Types:
    {"type": "task", "role": "system", "task": "[CMD:BROWSER_JS] <base64>"}
    {"type": "message", "content": "Hello", "room": "general"}
    {"type": "direct_message", "agent": "CodeAI", "content": "debug this"}
    
  Commands (loaded dynamically from modules):
    SCREENSHOT - Capture browser screenshots (PNG/JPEG/WebP)
    BROWSER_JS - Execute JavaScript with Promise-like returns
    Run 'continuum' to see full command list with descriptions
    
  Response Format:
    {"type": "response", "message": "<result>", "agent": "Claude"}
    {"type": "connection_banner", "data": {"commands": [...], "agents": [...]}}
    
  Examples:
    # Send joke to browser
    {"type": "task", "role": "system", "task": "[CMD:BROWSER_JS] Y29uc29sZS5sb2coJ0pva2UhJyk="}
    
    # Capture screenshot
    {"type": "task", "role": "system", "task": "[CMD:SCREENSHOT] {}"}
    
    # Chat with Claude
    {"type": "message", "content": "Tell me a joke", "room": "general"}
    
    # Python AI screenshot example
    python python-client/examples/simple_screenshot.py
    
    # Complete UI development workflow example  
    python python-client/examples/fix_ui_styling_with_feedback.py
    
  Connection acts like terminal login - full command interface available

DAEMON MODE:
  continuum --daemon                        # Run as background daemon
  continuum --daemon --idle-timeout 60     # Auto-shutdown after 60 min idle
  continuum --daemon --idle-timeout 0      # Never auto-shutdown
  
  Agents can wake daemon: heal "Connection refused"
  
EXAMPLES:
  continuum                   # Start on default port 9000
  continuum --port 8080       # Start on custom port
  continuum --restart         # Force restart existing server
  continuum --daemon          # Run as background daemon with 30min timeout
  
For more information, visit: https://github.com/CambrianTech/continuum
`);
}

// Force restart functionality
async function forceRestart(options) {
  const fs = require('fs');
  const path = require('path');
  const { spawn } = require('child_process');
  
  console.log('🔄 Force restarting Continuum server...');
  
  // Bump version first
  console.log('📈 Bumping version...');
  try {
    await new Promise((resolve, reject) => {
      const versionProcess = spawn('npm', ['version', 'patch', '--no-git-tag-version'], {
        stdio: 'pipe',
        cwd: __dirname
      });
      
      let output = '';
      versionProcess.stdout.on('data', (data) => {
        output += data.toString();
      });
      
      versionProcess.on('close', (code) => {
        if (code === 0) {
          const newVersion = output.trim().replace(/^v/, '');
          console.log(`✅ Version bumped to: ${newVersion}`);
          resolve(newVersion);
        } else {
          console.log('⚠️ Version bump failed, continuing with restart...');
          resolve(null);
        }
      });
      
      versionProcess.on('error', (error) => {
        console.log('⚠️ Version bump error, continuing with restart...');
        resolve(null);
      });
    });
  } catch (error) {
    console.log('⚠️ Version bump failed, continuing with restart...');
  }
  
  // Look for existing PID files
  const cwd = process.cwd();
  const pidPaths = [
    path.join(cwd, '.continuum', 'continuum.pid'),
    path.join(cwd, 'continuum.pid'),
    path.join(require('os').homedir(), '.continuum', 'continuum.pid')
  ];
  
  let killedAny = false;
  
  for (const pidPath of pidPaths) {
    if (fs.existsSync(pidPath)) {
      try {
        const pid = parseInt(fs.readFileSync(pidPath, 'utf8').trim());
        
        console.log(`🔍 Found PID ${pid} in ${pidPath}`);
        
        // Try to kill the process
        try {
          process.kill(pid, 'SIGTERM');
          console.log(`💀 Sent SIGTERM to process ${pid}`);
          
          // Wait a moment for graceful shutdown
          await new Promise(resolve => setTimeout(resolve, 2000));
          
          // Check if process is still running, force kill if needed
          try {
            process.kill(pid, 0); // Check if process exists
            console.log(`⚡ Process ${pid} still running, sending SIGKILL`);
            process.kill(pid, 'SIGKILL');
          } catch (error) {
            // Process already dead, which is what we want
          }
          
          killedAny = true;
        } catch (error) {
          if (error.code === 'ESRCH') {
            console.log(`💀 Process ${pid} was already dead`);
          } else {
            console.log(`⚠️  Could not kill process ${pid}: ${error.message}`);
          }
        }
        
        // Remove the PID file
        fs.unlinkSync(pidPath);
        console.log(`🗑️  Removed PID file: ${pidPath}`);
        
      } catch (error) {
        console.log(`⚠️  Could not read PID file ${pidPath}: ${error.message}`);
      }
    }
  }
  
  if (killedAny) {
    console.log('✅ Existing Continuum processes terminated');
    // Wait a bit more to ensure port is freed
    await new Promise(resolve => setTimeout(resolve, 1000));
  } else {
    console.log('🔍 No existing Continuum processes found');
  }
  
  // Now start fresh with restart flag
  console.log('🚀 Starting fresh Continuum instance...');
  const Continuum = require('./src/core/continuum-core.cjs');
  const continuum = new Continuum({ ...options, isRestart: true });
  
  return continuum.start();
}

// Start as daemon
function startDaemon(options, flags) {
  const { spawn } = require('child_process');
  const path = require('path');
  
  console.log('🚀 Starting Continuum daemon...');
  
  // Build arguments for daemon process
  const args = [];
  
  if (options.port) {
    args.push('--port', options.port);
  }
  
  if (options['idle-timeout']) {
    args.push('--idle-timeout', options['idle-timeout']);
  }
  
  // Don't pass --daemon flag to child process to avoid infinite recursion
  
  // Create daemon process
  const daemon = spawn(process.execPath, [__filename, ...args], {
    detached: true,
    stdio: ['ignore', 'ignore', 'ignore'], // Fully detached
    cwd: process.cwd()
  });
  
  // Unref so parent can exit
  daemon.unref();
  
  console.log(`📝 Daemon started with PID: ${daemon.pid}`);
  console.log(`🌐 Will be available at: http://localhost:${options.port || process.env.CONTINUUM_PORT || '9000'}`);
  console.log(`⏰ Auto-shutdown after ${options['idle-timeout'] || '30'} minutes of inactivity`);
  console.log('🔧 Agents can wake daemon with: heal "Connection refused"');
  console.log('✅ Daemon launched successfully');
  
  process.exit(0);
}

// Main CLI handler
function main() {
  const { flags, options } = parseArgs();
  
  // Handle version flag
  if (flags.has('version') || flags.has('v')) {
    console.log(`continuum v${getVersion()}`);
    process.exit(0);
  }
  
  // Handle help flag
  if (flags.has('help') || flags.has('h')) {
    showHelp();
    process.exit(0);
  }
  
  // Handle restart flag
  if (flags.has('restart')) {
    forceRestart(options).catch(error => {
      console.error('❌ Failed to restart Continuum:', error);
      process.exit(1);
    });
    return;
  }
  
  // Handle daemon mode
  if (flags.has('daemon')) {
    startDaemon(options, flags);
    return;
  }
  
  // Start the full Continuum system
  const Continuum = require('./src/core/continuum-core.cjs');
  
  // Add idle timeout support - default 30 minutes for foreground, 60 for background
  const idleTimeoutMinutes = parseInt(options['idle-timeout']) || 30;
  const finalOptions = { 
    ...options, 
    idleTimeout: idleTimeoutMinutes > 0 ? idleTimeoutMinutes * 60 * 1000 : 0 
  };
  
  // User-kind messaging
  console.log('🚀 Starting Continuum Academy...');
  console.log(`🌐 Web interface: http://localhost:${options.port || process.env.CONTINUUM_PORT || '9000'}`);
  console.log(`⏰ Auto-shutdown after ${idleTimeoutMinutes} minutes of inactivity`);
  console.log('🛰️ Agent portal: Run "js-send" commands while server is active');
  console.log('💡 Use "continuum --daemon" for background operation');
  console.log('📚 Need help? Run "continuum --help"');
  console.log('');
  
  const continuum = new Continuum(finalOptions);
  
  continuum.start().catch(error => {
    console.error('❌ Failed to start Continuum:', error);
    process.exit(1);
  });
}

// Run main function if this file is executed directly
if (require.main === module) {
  main();
}

module.exports = require('./src/core/continuum-core.cjs');