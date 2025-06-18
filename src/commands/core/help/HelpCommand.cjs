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
        },
        sync: {
          type: 'boolean',
          required: false,
          description: 'Sync documentation (generate README.md from live help)',
          default: false
        },
        output: {
          type: 'string',
          required: false,
          description: 'Output file for sync (default: README.md)',
          default: 'README.md'
        }
      },
      examples: [
        'help',
        'help --section commands',
        'help --sync',
        'help --sync --output API.md'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    // Handle sync documentation first
    if (options.sync) {
      return await this.syncDocumentation(options, continuum);
    }
    
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

🤖 AI AGENT QUICK START:
  # Use the AI Portal (thin client adapter for command bus)
  python3 python-client/ai-portal.py --help              # Show all available commands
  python3 python-client/ai-portal.py --cmd help          # Live API documentation
  python3 python-client/ai-portal.py --cmd workspace     # Get workspace paths
  python3 python-client/ai-portal.py --cmd sentinel      # Start monitoring/logging
  
  # Commands are self-documenting via help system
  python3 python-client/ai-portal.py --cmd workspace --help
  python3 python-client/ai-portal.py --cmd sentinel --help
  
  # Everything is promise-based and modular - no hardcoded paths or god objects

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

📁 KEY LOCATIONS (where things are):
  python-client/ai-portal.py              # AI agent entry point (thin client adapter)
  python-client/continuum_client/         # Python API (promise-based, forwards to command bus)
  src/commands/core/                       # Modular commands (workspace, sentinel, restart, etc)
  src/integrations/WebSocketServer.cjs    # Command bus message routing
  .continuum/                              # Workspace directory (managed by workspace command)
  .continuum/ai-portal/                    # AI portal workspace and logs
  .continuum/sentinel/                     # Sentinel monitoring and task logs
  docs/AI_PORTAL_ARCHITECTURE.md          # Architecture documentation

🏗️ ARCHITECTURE PRINCIPLES:
  • Continuum = OS/Orchestrator with modular command bus
  • Clients = Thin adapters (Python, Browser, etc) that forward to command bus
  • Commands = Self-documenting, discoverable, modular functionality
  • No god objects, no hardcoded paths, promise-based API

WEB INTERFACE:
  Navigate to http://localhost:9000 after starting

For more information, visit: https://github.com/CambrianTech/continuum
`);

    return this.createSuccessResult({ version: this.getVersion() }, 'Help displayed');
  }
  
  static async syncDocumentation(options, continuum) {
    const fs = require('fs');
    const path = require('path');
    
    try {
      console.log('📖 Syncing documentation from live help system...');
      
      const content = await this.generateMarkdownDocs(continuum);
      
      // Write to output file
      const projectRoot = process.cwd();
      const outputPath = options.output || 'README.md';
      const fullOutputPath = path.join(projectRoot, outputPath);
      
      fs.writeFileSync(fullOutputPath, content);
      
      console.log(`✅ Documentation synced to: ${outputPath}`);
      console.log(`📄 ${content.split('\n').length} lines generated from live help system`);
      
      return this.createSuccessResult({
        outputPath,
        size: content.length,
        lines: content.split('\n').length,
        timestamp: new Date().toISOString()
      }, `Documentation synced to ${outputPath}`);
      
    } catch (error) {
      console.error('❌ Documentation sync failed:', error.message);
      return this.createErrorResult('Documentation sync failed', error.message);
    }
  }
  
  static async generateMarkdownDocs(continuum) {
    const content = [];
    
    // Header
    content.push('# Continuum - AI Workforce Construction Platform\n');
    content.push('> 📖 Documentation auto-generated from live help system\n');
    content.push('> 🔄 To update: `python3 python-client/ai-portal.py --cmd help --sync`\n\n');
    
    // Overview from help content
    content.push('## Overview\n');
    content.push(this.getOverviewContent());
    
    // AI Agent Guide 
    content.push('## AI Agent Quick Start\n');
    content.push(this.getAIAgentContent());
    
    // Available Commands
    content.push('## Available Commands\n');
    content.push(await this.getCommandsList(continuum));
    
    // Architecture
    content.push('## Architecture\n');
    content.push(this.getArchitectureContent());
    
    // Key Locations
    content.push('## Key Locations\n');
    content.push(this.getKeyLocationsContent());
    
    // Footer
    content.push('---\n');
    content.push(`*Documentation auto-generated on ${new Date().toISOString()}*  \n`);
    content.push('*Source: Live help system via `help --sync` command*  \n');
    content.push('*Architecture: Command bus with thin client adapters*\n');
    
    return content.join('');
  }
  
  static getOverviewContent() {
    return `
Continuum is a revolutionary AI workforce construction platform with clean architecture principles:

- **🏗️ Command Bus Architecture**: Central orchestration with modular commands
- **🤖 AI Portal**: Primary interface for AI agents (\`python-client/ai-portal.py\`)
- **📡 Promise-based API**: Clean async/await patterns across all clients
- **🛡️ Sentinel System**: Monitoring and logging for AI task management  
- **📁 Workspace Management**: No hardcoded paths, configurable workspaces
- **📚 Self-documenting**: Live help system keeps docs in sync

### Quick Start

\`\`\`bash
# For AI Agents (primary interface)
python3 python-client/ai-portal.py --help
python3 python-client/ai-portal.py --cmd help

# For Humans
continuum --help
continuum --agents
\`\`\`

`;
  }
  
  static getAIAgentContent() {
    return `
The AI Portal provides a clean, thin client adapter for the Continuum command bus:

\`\`\`bash
# Primary AI interface
python3 python-client/ai-portal.py --cmd [command] [--params '{}']

# Essential commands for AI agents
python3 python-client/ai-portal.py --cmd workspace     # Get workspace paths
python3 python-client/ai-portal.py --cmd sentinel      # Start monitoring/logging  
python3 python-client/ai-portal.py --cmd restart       # Version bump + server restart
python3 python-client/ai-portal.py --cmd help          # Live API documentation

# All commands are self-documenting
python3 python-client/ai-portal.py --cmd [command] --help
\`\`\`

### Architecture Principles for AI Agents

- ✅ **No hardcoded paths** - Use workspace command for all directory management
- ✅ **No god objects** - Thin client adapter pattern, all logic in server commands  
- ✅ **Self-documenting** - Live help system provides current API documentation
- ✅ **Promise-based** - Clean async/await, no callback complexity
- ✅ **Modular** - Add functionality via Continuum commands, not client code

`;
  }
  
  static async getCommandsList(continuum) {
    const content = [];
    
    try {
      // Get live command registry
      const registry = continuum.commandProcessor?.commandRegistry;
      if (registry && registry.commands) {
        const commands = Array.from(registry.commands.entries())
          .map(([name, command]) => {
            if (command.getDefinition) {
              const def = command.getDefinition();
              return `- **${def.name}** ${def.icon} - ${def.description}`;
            }
            return `- **${name}** - Command available`;
          })
          .sort()
          .join('\n');
        
        content.push('### Core Commands\n');
        content.push(commands);
        content.push('\n\n');
        content.push('💡 **Get detailed help**: `python3 python-client/ai-portal.py --cmd [command] --help`\n\n');
      }
    } catch (error) {
      content.push('### Core Commands\n');
      content.push('- **help** 📚 - Show help information and sync documentation\n');
      content.push('- **workspace** 📁 - Manage workspace directories and paths\n');
      content.push('- **sentinel** 🛡️ - AI guardian for logging and task management\n');
      content.push('- **restart** 🔄 - Restart server with version bump\n');
      content.push('- **info** ℹ️ - System information and status\n\n');
      content.push('💡 **Get live commands**: `python3 python-client/ai-portal.py --cmd help`\n\n');
    }
    
    return content.join('');
  }
  
  static getArchitectureContent() {
    return `
\`\`\`
┌─────────────────────────────────────────┐
│           Continuum Server              │
│         (OS/Orchestrator)               │
│  ┌─────────────────────────────────────┐ │
│  │         Command Bus                 │ │
│  │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐  │ │
│  │  │work │ │restart│sentinel│help │  │ │ 
│  │  │space│ │     │ │     │ │     │  │ │
│  │  └─────┘ └─────┘ └─────┘ └─────┘  │ │
│  └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
         ↑                    ↑
   ┌─────────┐          ┌─────────┐
   │ Python  │          │Browser  │
   │ Client  │          │ Client  │
   │(AI Portal)│        │   (UI)  │
   └─────────┘          └─────────┘
\`\`\`

### Design Patterns

- **Adapter Pattern**: Thin clients forward commands to server bus
- **Command Bus**: All business logic in modular server commands
- **Promise-Based**: Async/await patterns across all interfaces
- **Self-Documenting**: Help system generates live documentation
- **No God Objects**: Clean separation of concerns throughout

`;
  }
  
  static getKeyLocationsContent() {
    return `
| Location | Purpose |
|----------|---------|
| \`python-client/ai-portal.py\` | 🚀 Primary AI agent interface (thin client adapter) |
| \`python-client/continuum_client/\` | Promise-based Python API library |
| \`src/commands/core/\` | Modular command implementations |
| \`src/integrations/WebSocketServer.cjs\` | Command bus message routing |
| \`.continuum/\` | Workspace directory (managed by workspace command) |
| \`.continuum/ai-portal/\` | AI portal workspace and logs |
| \`.continuum/sentinel/\` | Sentinel monitoring and task logs |
| \`docs/AI_PORTAL_ARCHITECTURE.md\` | Detailed architecture documentation |

`;
  }
}

module.exports = HelpCommand;