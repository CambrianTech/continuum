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
  
For more information, visit: https://github.com/CambrianTech/continuum
`);
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