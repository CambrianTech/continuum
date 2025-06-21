/**
 * DocsCommand - Generate dynamic documentation from help system
 * Keeps README and docs in sync with actual command definitions
 */

const BaseCommand = require('../../core/BaseCommand.cjs');
const fs = require('fs');
const path = require('path');

class DocsCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'docs',
      description: 'Generate dynamic documentation from help system',
      icon: '📖',
      parameters: {
        format: {
          type: 'string',
          required: false,
          description: 'Output format: markdown, json, html',
          default: 'markdown'
        },
        output: {
          type: 'string',
          required: false,
          description: 'Output file path (relative to project root)',
          default: 'README.md'
        },
        include: {
          type: 'string',
          required: false,
          description: 'What to include: all, commands, help, agents',
          default: 'all'
        },
        sync: {
          type: 'boolean',
          required: false,
          description: 'Sync FILES.md with current file structure first',
          default: false
        }
      },
      examples: [
        'docs',
        'docs --sync',
        'docs --format markdown --output API.md',
        'docs --include commands --output COMMANDS.md',
        'docs --sync --output README.md',
        'docs --format json --output api.json'
      ]
    };
  }

  static async execute(params, continuum) {
    const options = this.parseParams(params);
    const format = options.format || 'markdown';
    const outputPath = options.output || 'README.md';
    const include = options.include || 'all';
    const sync = options.sync || false;
    
    try {
      console.log(`📖 Generating ${format} documentation...`);
      
      // If sync is requested, update FILES.md first
      if (sync) {
        console.log('📁 Syncing FILES.md with current file structure...');
        await this.syncFileStructure();
      }
      
      const content = await this.generateDocs(continuum, format, include);
      
      // Write to output file
      const projectRoot = process.cwd();
      const fullOutputPath = path.join(projectRoot, outputPath);
      
      fs.writeFileSync(fullOutputPath, content);
      
      console.log(`✅ Documentation written to: ${outputPath}`);
      
      const result = {
        format,
        outputPath,
        include,
        size: content.length,
        lines: content.split('\n').length
      };
      
      if (sync) {
        result.fileStructureSynced = true;
        console.log('📚 Key Documentation Files:');
        console.log('   🏛️ RESTORATION-STRATEGY.md - Complete restoration plan');
        console.log('   📖 FILES.md - Archaeological map with Agent Study Guide');  
        console.log('   🚀 README.md - System overview and quick start');
        console.log('   🎓 docs/ACADEMY_ARCHITECTURE.md - Academy system details');
      }
      
      return this.createSuccessResult(result, `Documentation generated: ${outputPath}`);
      
    } catch (error) {
      return this.createErrorResult('Documentation generation failed', error.message);
    }
  }
  
  static async syncFileStructure() {
    const { spawn } = require('child_process');
    const path = require('path');
    
    return new Promise((resolve, reject) => {
      const scriptPath = path.join(process.cwd(), 'scripts', 'generate-files-tree.sh');
      
      const child = spawn('bash', [scriptPath], {
        cwd: process.cwd(),
        stdio: 'inherit'
      });
      
      child.on('close', (code) => {
        if (code === 0) {
          console.log('✅ File structure synced successfully');
          resolve();
        } else {
          reject(new Error(`File structure sync failed with code ${code}`));
        }
      });
      
      child.on('error', (error) => {
        reject(new Error(`Failed to run file structure sync: ${error.message}`));
      });
    });
  }
  
  static async generateDocs(continuum, format, include) {
    const content = [];
    
    if (format === 'markdown') {
      content.push('# Continuum - AI Workforce Construction Platform\n');
      content.push('> Dynamic documentation generated from live help system\n');
      
      if (include === 'all' || include === 'help') {
        content.push('## Overview\n');
        content.push(await this.getHelpContent(continuum));
        content.push('\n');
      }
      
      if (include === 'all' || include === 'agents') {
        content.push('## AI Agent Guide\n');
        content.push(await this.getAgentsContent(continuum));
        content.push('\n');
      }
      
      if (include === 'all' || include === 'commands') {
        content.push('## Available Commands\n');
        content.push(await this.getCommandsContent(continuum));
        content.push('\n');
      }
      
      // Add dashboard status report
      if (include === 'all' || include === 'status') {
        content.push('## Project Status Dashboard\n');
        content.push(await this.getDashboardReport());
        content.push('\n');
      }
      
      content.push('## Architecture\n');
      content.push(this.getArchitectureContent());
      content.push('\n');
      
      content.push('---\n');
      content.push(`*Documentation auto-generated on ${new Date().toISOString()}*\n`);
      content.push('*To update: `python3 python-client/ai-portal.py --cmd docs`*\n');
      
    } else if (format === 'json') {
      const data = {
        platform: 'Continuum',
        description: 'AI Workforce Construction Platform',
        generated: new Date().toISOString(),
        help: await this.getHelpContentRaw(continuum),
        agents: await this.getAgentsContentRaw(continuum),
        commands: await this.getCommandsData(continuum),
        architecture: this.getArchitectureData()
      };
      return JSON.stringify(data, null, 2);
    }
    
    return content.join('');
  }
  
  static async getDashboardReport() {
    try {
      // Call the Python dashboard to get status report
      const { spawn } = require('child_process');
      const pythonPath = path.join(process.cwd(), 'python-client', 'ai-agent.py');
      
      return new Promise((resolve) => {
        const python = spawn('python3', [pythonPath, '--broken'], {
          cwd: process.cwd(),
          stdio: ['ignore', 'pipe', 'pipe']
        });
        
        let output = '';
        python.stdout.on('data', (data) => {
          output += data.toString();
        });
        
        python.on('close', (code) => {
          if (code === 0) {
            // Convert console output to markdown format
            const lines = output.split('\n');
            const markdownOutput = [];
            
            markdownOutput.push('> 📊 **Live Status Report** - Generated from dependency-aware dashboard\n');
            markdownOutput.push('```');
            markdownOutput.push(output.trim());
            markdownOutput.push('```\n');
            
            markdownOutput.push('### Quick Commands for Development\n');
            markdownOutput.push('```bash');
            markdownOutput.push('# View full dashboard');
            markdownOutput.push('python3 python-client/ai-portal.py --dashboard');
            markdownOutput.push('');
            markdownOutput.push('# View broken commands in fix order');
            markdownOutput.push('python3 python-client/ai-portal.py --broken');
            markdownOutput.push('');
            markdownOutput.push('# Test any command');
            markdownOutput.push('python3 python-client/ai-portal.py --cmd [command-name]');
            markdownOutput.push('```\n');
            
            resolve(markdownOutput.join('\n'));
          } else {
            resolve('> ⚠️ Dashboard status unavailable - run `python3 python-client/ai-portal.py --dashboard` manually\n');
          }
        });
        
        python.on('error', () => {
          resolve('> ⚠️ Dashboard status unavailable - run `python3 python-client/ai-portal.py --dashboard` manually\n');
        });
      });
    } catch (error) {
      return '> ⚠️ Dashboard status unavailable - run `python3 python-client/ai-portal.py --dashboard` manually\n';
    }
  }

  static async getHelpContent(continuum) {
    try {
      // Execute help command to get live content
      const helpResult = await continuum.commandProcessor.executeCommand('HELP', '{}');
      if (helpResult && helpResult.success) {
        // Extract the console output from help command
        return '```\nContinuum Help System Output\n(See help command for live version)\n```\n';
      }
    } catch (error) {
      console.log('Note: Could not get live help content, using static description');
    }
    
    return `
Continuum is a revolutionary AI workforce construction platform that provides:

- **Modular Command Architecture**: Self-documenting, discoverable commands
- **Thin Client Adapters**: Python, Browser, and other client interfaces
- **Promise-based API**: Clean async/await patterns across all clients
- **AI Portal**: Primary interface for AI agents and automation
- **Sentinel System**: Monitoring and logging for AI task management
- **Command Bus**: Central orchestration of all system functionality

### Quick Start

\`\`\`bash
# For AI Agents
python3 python-client/ai-portal.py --help
python3 python-client/ai-portal.py --cmd help

# For Humans  
continuum --help
continuum --agents
\`\`\`
`;
  }
  
  static async getAgentsContent(continuum) {
    return `
### AI Agent Interface

The AI Portal provides a clean, promise-based interface for AI agents:

\`\`\`bash
# Primary interface
python3 python-client/ai-portal.py --cmd [command] [--params '{}']

# Key commands for AI agents
python3 python-client/ai-portal.py --cmd workspace    # Get workspace paths
python3 python-client/ai-portal.py --cmd sentinel     # Start monitoring
python3 python-client/ai-portal.py --cmd restart      # Version bump + restart
python3 python-client/ai-portal.py --cmd help         # Live API docs
\`\`\`

### Architecture Principles for AI Agents

- **No hardcoded paths**: Use workspace command for all paths
- **No god objects**: Thin client adapter pattern only
- **Self-documenting**: All commands provide help via \`--cmd [command] --help\`
- **Promise-based**: Clean async/await, no callback hell
- **Modular**: Add functionality via Continuum commands, not client code
`;
  }
  
  static async getCommandsContent(continuum) {
    const commands = await this.getCommandsData(continuum);
    const content = [];
    
    content.push('### Core Commands\n');
    
    for (const [category, cmdList] of Object.entries(commands)) {
      content.push(`#### ${category}\n`);
      
      for (const cmd of cmdList) {
        content.push(`**${cmd.name}** - ${cmd.description}\n`);
        content.push(`- Icon: ${cmd.icon}\n`);
        
        if (cmd.parameters && Object.keys(cmd.parameters).length > 0) {
          content.push(`- Parameters:\n`);
          for (const [param, details] of Object.entries(cmd.parameters)) {
            const required = details.required ? '(required)' : '(optional)';
            const defaultVal = details.default ? ` default: ${details.default}` : '';
            content.push(`  - \`${param}\`: ${details.type} ${required} - ${details.description}${defaultVal}\n`);
          }
        }
        
        if (cmd.examples && cmd.examples.length > 0) {
          content.push(`- Examples:\n`);
          for (const example of cmd.examples) {
            content.push(`  - \`python3 python-client/ai-portal.py --cmd ${example}\`\n`);
          }
        }
        
        content.push('\n');
      }
    }
    
    return content.join('');
  }
  
  static async getCommandsData(continuum) {
    const commands = {};
    
    try {
      // Get command registry from continuum
      const registry = continuum.commandProcessor?.commandRegistry;
      if (registry && registry.commands) {
        for (const [name, command] of registry.commands) {
          if (command.getDefinition) {
            const def = command.getDefinition();
            const category = def.category || 'Core';
            
            if (!commands[category]) {
              commands[category] = [];
            }
            
            commands[category].push(def);
          }
        }
      }
    } catch (error) {
      console.log('Note: Could not get live command data');
      // Fallback to known commands
      commands['Core'] = [
        { name: 'help', description: 'Show help information', icon: '📚' },
        { name: 'workspace', description: 'Manage workspace directories', icon: '📁' },
        { name: 'sentinel', description: 'AI guardian for logging and monitoring', icon: '🛡️' },
        { name: 'restart', description: 'Restart server with version bump', icon: '🔄' }
      ];
    }
    
    return commands;
  }
  
  static getArchitectureContent() {
    return `
### System Architecture

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

### Design Principles

- **Adapter Pattern**: Thin clients forward to command bus
- **Command Bus**: All business logic in server commands
- **Self-Documenting**: Live help system keeps docs in sync
- **Promise-Based**: Clean async/await across all interfaces
- **Modular**: Add functionality via commands, not client code
- **No God Objects**: Clean separation of concerns

### Key Locations

- \`python-client/ai-portal.py\` - Primary AI agent interface
- \`python-client/continuum_client/\` - Promise-based Python API
- \`src/commands/core/\` - Modular command implementations
- \`.continuum/\` - Workspace directory (managed by workspace command)
- \`docs/AI_PORTAL_ARCHITECTURE.md\` - Detailed architecture docs
`;
  }
  
  static getArchitectureData() {
    return {
      pattern: 'Command Bus with Thin Client Adapters',
      principles: [
        'Adapter Pattern: Thin clients forward to command bus',
        'Command Bus: All business logic in server commands', 
        'Self-Documenting: Live help system keeps docs in sync',
        'Promise-Based: Clean async/await across all interfaces',
        'Modular: Add functionality via commands, not client code',
        'No God Objects: Clean separation of concerns'
      ],
      components: {
        server: 'Continuum Server (OS/Orchestrator)',
        commandBus: 'Central command routing and execution',
        clients: ['Python Client (AI Portal)', 'Browser Client (UI)'],
        commands: ['workspace', 'sentinel', 'restart', 'help', 'docs']
      }
    };
  }
  
  static async getHelpContentRaw(continuum) {
    // Return structured help data for JSON format
    return {
      description: 'Continuum AI Workforce Construction Platform',
      quickStart: [
        'python3 python-client/ai-portal.py --help',
        'python3 python-client/ai-portal.py --cmd help',
        'continuum --help',
        'continuum --agents'
      ]
    };
  }
  
  static async getAgentsContentRaw(continuum) {
    return {
      primaryInterface: 'python3 python-client/ai-portal.py',
      keyCommands: ['workspace', 'sentinel', 'restart', 'help'],
      principles: [
        'No hardcoded paths',
        'No god objects', 
        'Self-documenting',
        'Promise-based',
        'Modular'
      ]
    };
  }
}

module.exports = DocsCommand;