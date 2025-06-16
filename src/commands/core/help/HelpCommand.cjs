/**
 * HelpCommand - Show help information for users and admins
 */

const InfoCommand = require('../info/InfoCommand.cjs');

class HelpCommand extends InfoCommand {
  static getDefinition() {
    return {
      name: 'help',
      description: 'Show help information for users and admins',
      icon: '📚',
      parameters: {
        section: {
          type: 'string',
          required: false,
          description: 'Help section: overview, commands, debugging, setup',
          default: 'overview'
        }
      },
      examples: [
        'help',
        'help --section commands',
        'help --section debugging'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    this.displayHeader('🔄 Continuum Academy', 'Revolutionary AI Workforce Construction');
    
    console.log(`

USAGE:
  continuum                    Start the Academy web interface
  continuum --version         Show version information
  continuum --help            Show this help message (users/admins)
  continuum --agents          Show agent-specific development help
  continuum --test            Run built-in system tests (isolated, fresh logs)
  continuum --test --screenshot  Test screenshot system only  
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

🚨 AGENT DEVELOPMENT PROCESS (TRUST THE PROCESS):
  ⚠️  CRITICAL: Follow this methodology to ensure system stability ⚠️
  
  📖 COMPLETE PROCESS GUIDE:
  cat process.md                               # Full baby steps methodology
  
  🎯 SIMPLE COMMAND FOR FRESH AGENTS:
  python python-client/trust_the_process.py    # Single function call does it all!
  
  📋 Baby Steps Development Cycle (Automated):
  1️⃣  Clear old data: Delete .continuum/screenshots/ (avoid cheating/confusion)
  2️⃣  Make small change: Max 50 lines, single file only
  3️⃣  Bump version: Auto-increment build number for tracking
  4️⃣  Test immediately: Screenshot + console check + unit tests ← AUTOMATED
  5️⃣  Fix ANY errors: Zero tolerance for breaking the system
  6️⃣  Commit when stable: Only when everything works perfectly

🧪 COMPREHENSIVE TESTING SYSTEM:
  continuum --test                             # Run complete test suite from anywhere
  npm test -- __tests__/comprehensive/        # Single comprehensive test location
  # Tests all 58 patterns (32 Python + 26 JS) in one organized location
  # Covers: modular commands, screenshots, console reading, validation
  # Everything off continuum universal command API - elegant architecture

WEB INTERFACE:
  Navigate to http://localhost:9000 after starting

For more information, visit: https://github.com/CambrianTech/continuum
`);

    return this.createSuccessResult({ version: this.getVersion() }, 'Help displayed');
  }
}

module.exports = HelpCommand;