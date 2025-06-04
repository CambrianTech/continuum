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
  continuum --port <number>   Specify custom port (default: 5555)
  continuum --restart         Force restart the server (kill existing instance)

FEATURES:
  🎓 Academy adversarial training (Testing Droid vs Protocol Sheriff)
  🔬 LoRA adapter system (190,735x storage reduction)
  🏗️ Hierarchical specialization stacking
  🤝 Cross-scope persona sharing (project/user/organization)
  📊 Real-time cost tracking and session management
  💬 Multi-provider AI integration (OpenAI, Anthropic, HuggingFace)

WEB INTERFACE:
  Navigate to http://localhost:5555 after starting
  
EXAMPLES:
  continuum                   # Start on default port 5555
  continuum --port 8080       # Start on custom port
  continuum --restart         # Force restart existing server
  
For more information, visit: https://github.com/CambrianTech/continuum
`);
}

// Force restart functionality
async function forceRestart(options) {
  const fs = require('fs');
  const path = require('path');
  
  console.log('🔄 Force restarting Continuum server...');
  
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
  
  // Now start fresh
  console.log('🚀 Starting fresh Continuum instance...');
  const Continuum = require('./src/core/continuum-core.cjs');
  const continuum = new Continuum(options);
  
  return continuum.start();
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
  
  // Start the full Continuum system
  const Continuum = require('./src/core/continuum-core.cjs');
  const continuum = new Continuum(options);
  
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