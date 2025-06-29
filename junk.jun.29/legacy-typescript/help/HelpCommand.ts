/**
 * HelpCommand - TypeScript Implementation
 * Show help information for users and admins with documentation sync
 */

import { InfoCommand } from '../info/InfoCommand';
import { CommandDefinition, CommandContext, CommandResult } from '../BaseCommand';
import * as fs from 'fs';
import * as path from 'path';

interface HelpParams {
  section?: string;
  sync?: boolean;
  output?: string;
  status_table?: boolean;
  verbose?: boolean;
}

interface ReadmeDefinition {
  name?: string;
  description?: string;
  icon?: string;
  category?: string;
  status?: string;
  parameters: Record<string, any>;
  todos?: string[];
}

interface CommandStatusInfo {
  status: string;
  name: string;
  icon: string;
  todos: number | string;
  tests: string;
  lastUpdated: string;
  notes: string;
  priority: number;
}

interface TestResults {
  [commandName: string]: {
    passed: number;
    failed: number;
    total: number;
  };
}

interface ReadmeData {
  path: string;
  content: string;
  commandDir: string;
}

/**
 * HelpCommand - Show help information and sync documentation
 * Extends InfoCommand for system information display
 */
export class HelpCommand extends InfoCommand {
  static getDefinition(): CommandDefinition {
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      return this.parseReadmeDefinition(readme);
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'help',
        category: 'Core',
        icon: '📚',
        description: 'Show help information and sync documentation',
        params: '{"section?": "string", "sync?": "boolean", "output?": "string", "status_table?": "boolean", "verbose?": "boolean"}',
        examples: [
          'help',
          'help --params \'{"verbose": true}\'',
          'help --params \'{"sync": true, "output": "README.md"}\''
        ],
        usage: 'help [--params \'{"section": "<section>", "sync": true, "verbose": true}\']'
      };
    }
  }

  static async execute(params: any, context?: CommandContext): Promise<CommandResult> {
    this.logExecution('Help', params, context);

    const options = this.parseParams<HelpParams>(params);
    
    // Handle sync documentation first
    if (options.sync) {
      return await this.syncDocumentation(options, context);
    }
    
    // Handle verbose mode - show complete dashboard
    if (options.verbose) {
      return await this.showVerboseDashboard(context);
    }
    
    this.displayHeader('🔄 Continuum Academy', 'Revolutionary AI Workforce Construction');
    
    // Show one-liner project health status
    const healthStatus = await this.getProjectHealthOneLiner();
    console.log(`📊 ${healthStatus}\n`);
    
    this.displayHelpContent();

    return this.createSuccessResult(
      { version: this.getVersion() }, 
      'Help displayed'
    );
  }

  /**
   * Display main help content
   */
  private static displayHelpContent(): void {
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
  }

  /**
   * Sync documentation from live help system
   */
  private static async syncDocumentation(options: HelpParams, context?: CommandContext): Promise<CommandResult> {
    try {
      console.log('📖 Syncing documentation from live help system...');
      
      // For sync, default to including status table unless explicitly disabled
      const includeStatusTable = options.status_table !== false;
      const content = await this.generateMarkdownDocs(context, includeStatusTable);
      
      // Write to output file
      const projectRoot = process.cwd();
      const outputPath = options.output || 'README.md';
      const fullOutputPath = path.join(projectRoot, outputPath);
      
      fs.writeFileSync(fullOutputPath, content);
      
      console.log(`✅ Documentation synced to: ${outputPath}`);
      console.log(`📄 ${content.split('\n').length} lines generated from live help system`);
      if (includeStatusTable) {
        console.log('📊 Status dashboard included for project management');
      }
      
      return this.createSuccessResult({
        outputPath,
        size: content.length,
        lines: content.split('\n').length,
        statusTable: includeStatusTable,
        timestamp: new Date().toISOString()
      }, `Documentation synced to ${outputPath}`);
      
    } catch (error) {
      console.error('❌ Documentation sync failed:', error instanceof Error ? error.message : String(error));
      return this.createErrorResult(
        'Documentation sync failed', 
        error instanceof Error ? error.message : String(error)
      );
    }
  }

  /**
   * Generate markdown documentation from live system
   */
  private static async generateMarkdownDocs(context?: CommandContext, includeStatusTable = true): Promise<string> {
    const content: string[] = [];
    
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
    content.push(await this.getCommandsList(context));
    
    // Command Status Table (if requested)
    if (includeStatusTable) {
      content.push('## Command Status Dashboard\n');
      content.push('> 📊 Built-in project management - tracks command health and TODOs\n\n');
      content.push(await this.generateCommandStatusTable());
    }
    
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

  /**
   * Generate command status table for project management
   */
  private static async generateCommandStatusTable(): Promise<string> {
    const content: string[] = [];
    
    // Load test results if available
    const testResults = this.loadTestResults();
    
    content.push('| Status | Command | Icon | TODOs | Tests | Last Updated | Notes |\n');
    content.push('|--------|---------|------|-------|-------|--------------|-------|\n');
    
    const commandDirs = fs.readdirSync('./src/commands/core');
    const commands: CommandStatusInfo[] = [];
    
    for (const dir of commandDirs) {
      const dirPath = path.join('./src/commands/core', dir);
      if (fs.statSync(dirPath).isDirectory()) {
        const readmePath = path.join(dirPath, 'README.md');
        const hasReadme = fs.existsSync(readmePath);
        
        if (hasReadme) {
          const readme = fs.readFileSync(readmePath, 'utf8');
          const statusMatch = readme.match(/\*\*Status\*\*:\s*([^\n]+)/);
          const iconMatch = readme.match(/\*\*Icon\*\*:\s*([^\n]+)/);
          const status = statusMatch ? statusMatch[1].trim() : '⚪ No status';
          const icon = iconMatch ? iconMatch[1].trim() : '📄';
          const todoCount = (readme.match(/TODO:/g) || []).length;
          
          const statusIcon = status.includes('🟢') ? '🟢' : 
                            status.includes('🟡') ? '🟡' : 
                            status.includes('🔴') ? '🔴' : '⚪';
          
          const dateMatch = status.match(/(\d{4}-\d{2}-\d{2})/);
          const lastUpdated = dateMatch ? dateMatch[1] : 'Unknown';
          
          const notes = status.includes('BROKEN') ? 'CRITICAL ISSUES' :
                       status.includes('TESTING') ? 'In migration' :
                       status.includes('STABLE') ? 'Production ready' : 'Needs review';
          
          const testStatus = testResults[dir] || { passed: 0, failed: 0, total: 0 };
          const testInfo = testStatus.total > 0 ? 
            `${testStatus.passed}/${testStatus.total}` : 'No tests';
          
          commands.push({
            status: statusIcon,
            name: dir,
            icon: icon,
            todos: todoCount,
            tests: testInfo,
            lastUpdated: lastUpdated,
            notes: notes,
            priority: statusIcon === '🔴' ? 0 : statusIcon === '🟠' ? 1 : statusIcon === '🟡' ? 2 : statusIcon === '🟢' ? 3 : 4
          });
        } else {
          const testStatus = testResults[dir] || { passed: 0, failed: 0, total: 0 };
          const testInfo = testStatus.total > 0 ? 
            `${testStatus.passed}/${testStatus.total}` : 'No tests';
            
          commands.push({
            status: '🟠',
            name: dir,
            icon: '📄',
            todos: '?',
            tests: testInfo,
            lastUpdated: 'Never',
            notes: 'No documentation',
            priority: 1
          });
        }
      }
    }
    
    // Sort by priority (broken first, then untested, etc.)
    commands.sort((a, b) => a.priority - b.priority);
    
    commands.forEach(cmd => {
      content.push(`| ${cmd.status} | ${cmd.name} | ${cmd.icon} | ${cmd.todos} | ${cmd.tests} | ${cmd.lastUpdated} | ${cmd.notes} |\n`);
    });
    
    content.push('\n### Project Health Summary\n\n');
    const stats = commands.reduce((acc, cmd) => {
      acc[cmd.status] = (acc[cmd.status] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);
    
    content.push('| Status | Count | Description |\n');
    content.push('|--------|-------|-------------|\n');
    content.push(`| 🔴 | ${stats['🔴'] || 0} | Broken - Do not use |\n`);
    content.push(`| 🟠 | ${stats['🟠'] || 0} | Untested - Needs documentation |\n`);
    content.push(`| 🟡 | ${stats['🟡'] || 0} | Testing - In progress |\n`);
    content.push(`| 🟢 | ${stats['🟢'] || 0} | Stable - Production ready |\n`);
    content.push(`| ⚪ | ${stats['⚪'] || 0} | Unknown - Needs assessment |\n`);
    
    const total = commands.length;
    const health = Math.round(((stats['🟢'] || 0) / total) * 100);
    content.push(`\n**Project Health: ${health}% stable (${stats['🟢'] || 0}/${total} commands)**\n\n`);
    
    return content.join('');
  }

  /**
   * Load test results from disk
   */
  private static loadTestResults(): TestResults {
    try {
      const testResultsDir = path.join(process.cwd(), '.continuum', 'test-results');
      if (!fs.existsSync(testResultsDir)) {
        return {};
      }
      
      const results: TestResults = {};
      const files = fs.readdirSync(testResultsDir);
      
      for (const file of files) {
        if (file.endsWith('.json')) {
          try {
            const commandName = file.replace('.json', '');
            const filePath = path.join(testResultsDir, file);
            const data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
            results[commandName] = data;
          } catch (error) {
            console.warn(`Could not parse test results for ${file}: ${error instanceof Error ? error.message : String(error)}`);
          }
        }
      }
      
      return results;
    } catch (error) {
      return {};
    }
  }

  /**
   * Get project health one-liner summary
   */
  private static async getProjectHealthOneLiner(): Promise<string> {
    const commandDirs = fs.readdirSync('./src/commands/core');
    const stats = { '🔴': 0, '🟠': 0, '🟡': 0, '🟢': 0, '⚪': 0 };
    let total = 0;
    
    for (const dir of commandDirs) {
      const dirPath = path.join('./src/commands/core', dir);
      if (fs.statSync(dirPath).isDirectory()) {
        total++;
        const readmePath = path.join(dirPath, 'README.md');
        
        if (fs.existsSync(readmePath)) {
          const readme = fs.readFileSync(readmePath, 'utf8');
          const statusMatch = readme.match(/\*\*Status\*\*:\s*([^\n]+)/);
          const status = statusMatch ? statusMatch[1].trim() : '⚪ No status';
          
          if (status.includes('🟢')) stats['🟢']++;
          else if (status.includes('🟡')) stats['🟡']++;
          else if (status.includes('🔴')) stats['🔴']++;
          else stats['⚪']++;
        } else {
          stats['🟠']++;
        }
      }
    }
    
    const health = Math.round((stats['🟢'] / total) * 100);
    const issues = stats['🔴'] + stats['🟠'];
    
    if (issues === 0) {
      return `Project Health: ${health}% stable (${stats['🟢']}/${total} commands)`;
    } else {
      return `Project Health: ${health}% stable (${stats['🟢']}/${total}) - ${issues} need attention`;
    }
  }

  /**
   * Show verbose dashboard with complete status
   */
  private static async showVerboseDashboard(context?: CommandContext): Promise<CommandResult> {
    console.log('📊 CONTINUUM PROJECT MANAGEMENT DASHBOARD\n');
    console.log('='.repeat(60));
    
    // Project health summary
    const healthStatus = await this.getProjectHealthOneLiner();
    console.log(`\n🏥 ${healthStatus}\n`);
    
    // Command status table
    console.log('📋 COMMAND STATUS TABLE\n');
    const table = await this.generateCommandStatusTable();
    
    // Convert markdown table to terminal-friendly format
    const lines = table.split('\n');
    for (const line of lines) {
      if (line.trim()) {
        if (line.includes('|')) {
          // Format table rows for terminal
          const cols = line.split('|').map(col => col.trim()).filter(col => col);
          if (cols.length >= 6) {
            const [status, command, icon, todos, updated, notes] = cols;
            console.log(`${status} ${command.padEnd(12)} ${icon} TODOs:${todos.padEnd(3)} Updated:${updated.padEnd(10)} ${notes}`);
          }
        } else {
          // Headers and other content
          console.log(line);
        }
      }
    }
    
    console.log('\n' + '='.repeat(60));
    
    // Quick action suggestions
    console.log('\n🚀 QUICK ACTIONS:');
    console.log('   • Fix broken commands: See 🔴 items above');
    console.log('   • Document untested: Add README.md to 🟠 commands');
    console.log('   • Complete testing: Finish 🟡 commands in progress');
    console.log('   • Update docs: python3 ai-portal.py --cmd help --sync');
    console.log('   • Run tests: npm test');
    
    console.log('\n💡 TIP: Use "help --sync" to update the main README.md with this dashboard');
    
    return this.createSuccessResult({
      verbose: true,
      health: await this.getProjectHealthOneLiner(),
      timestamp: new Date().toISOString()
    }, 'Verbose dashboard displayed');
  }

  /**
   * Get overview content for documentation
   */
  private static getOverviewContent(): string {
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

  /**
   * Get AI agent content for documentation
   */
  private static getAIAgentContent(): string {
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

  /**
   * Collect all README files from commands directory
   */
  private static async collectAllReadmes(baseDir: string): Promise<Map<string, ReadmeData>> {
    const readmes = new Map<string, ReadmeData>();
    
    const walkDirectory = async (dir: string): Promise<void> => {
      try {
        const entries = fs.readdirSync(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);
          
          if (entry.isDirectory()) {
            // Recursively walk subdirectories
            await walkDirectory(fullPath);
          } else if (entry.name === 'README.md') {
            // Found a README.md file
            try {
              const content = fs.readFileSync(fullPath, 'utf8');
              const relativePath = path.relative(baseDir, fullPath);
              readmes.set(relativePath, {
                path: fullPath,
                content: content,
                commandDir: path.dirname(fullPath)
              });
            } catch (error) {
              console.warn(`Could not read README.md at ${fullPath}: ${error instanceof Error ? error.message : String(error)}`);
            }
          }
        }
      } catch (error) {
        console.warn(`Could not read directory ${dir}: ${error instanceof Error ? error.message : String(error)}`);
      }
    };
    
    await walkDirectory(baseDir);
    return readmes;
  }

  /**
   * Get commands list for documentation
   */
  private static async getCommandsList(context?: CommandContext): Promise<string> {
    const content: string[] = [];
    
    try {
      // README-driven: Collect command info from README files
      const commandsDir = path.join(__dirname, '../..');
      const readmes = await this.collectAllReadmes(commandsDir);
      
      const commands: string[] = [];
      for (const [relativePath, readmeData] of readmes) {
        try {
          const definition = this.parseReadmeDefinition(readmeData.content);
          if (definition.name) {
            commands.push(`- **${definition.name}** ${definition.icon || ''} - ${definition.description || 'Command available'}`);
          }
        } catch (error) {
          console.warn(`Could not parse definition from ${relativePath}: ${error instanceof Error ? error.message : String(error)}`);
        }
      }
      
      if (commands.length > 0) {
        content.push('### Core Commands\n');
        content.push(commands.sort().join('\n'));
        content.push('\n\n');
        content.push('💡 **Get detailed help**: `python3 python-client/ai-portal.py --cmd [command] --help`\n\n');
      }
      
      // Fallback to live command registry if no READMEs found
      if (commands.length === 0) {
        const continuum = context?.continuum;
        const registry = continuum?.commandProcessor?.commandRegistry;
        if (registry && registry.commands) {
          const liveCommands = Array.from(registry.commands.entries())
            .map(([name, command]: [string, any]) => {
              if (command.getDefinition) {
                const def = command.getDefinition();
                return `- **${def.name}** ${def.icon || ''} - ${def.description}`;
              }
              return `- **${name}** - Command available`;
            })
            .sort()
            .join('\n');
          
          content.push('### Core Commands\n');
          content.push(liveCommands);
          content.push('\n\n');
          content.push('💡 **Get detailed help**: `python3 python-client/ai-portal.py --cmd [command] --help`\n\n');
        }
      }
      
    } catch (error) {
      // Final fallback
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

  /**
   * Get architecture content for documentation
   */
  private static getArchitectureContent(): string {
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

  /**
   * Get key locations content for documentation
   */
  private static getKeyLocationsContent(): string {
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

  /**
   * Display header with consistent formatting
   */
  private static displayHeader(title: string, subtitle: string): void {
    console.log(`${title}: ${subtitle}`);
    console.log('='.repeat(80));
  }

  /**
   * Parse README.md for command definition
   */
  private static parseReadmeDefinition(readme: string): ReadmeDefinition {
    const lines = readme.split('\n');
    const definition: ReadmeDefinition = { parameters: {} };
    
    let inDefinition = false;
    let inParams = false;
    let inTodos = false;
    const todos: string[] = [];
    
    for (const line of lines) {
      if (line.includes('## Definition')) {
        inDefinition = true;
        continue;
      }
      if (inDefinition && line.startsWith('##')) {
        inDefinition = false;
      }
      if (line.includes('## Parameters')) {
        inParams = true;
        continue;
      }
      if (inParams && line.startsWith('##')) {
        inParams = false;
      }
      if (line.includes('## TODO:')) {
        inTodos = true;
        continue;
      }
      if (inTodos && line.startsWith('##')) {
        inTodos = false;
      }
      
      if (inDefinition) {
        if (line.includes('**Name**:')) {
          definition.name = line.split('**Name**:')[1].trim();
        } else if (line.includes('**Description**:')) {
          definition.description = line.split('**Description**:')[1].trim();
        } else if (line.includes('**Icon**:')) {
          definition.icon = line.split('**Icon**:')[1].trim();
        } else if (line.includes('**Category**:')) {
          definition.category = line.split('**Category**:')[1].trim();
        } else if (line.includes('**Status**:')) {
          definition.status = line.split('**Status**:')[1].trim();
        }
      }
      
      if (inParams && line.includes('`') && line.includes(':')) {
        const param = line.match(/`([^`]+)`:\s*(.+)/);
        if (param) {
          definition.parameters[param[1]] = {
            type: 'string',
            description: param[2]
          };
        }
      }
      
      if (inTodos && line.includes('TODO:')) {
        todos.push(line.trim());
      }
    }
    
    // Add TODOs to description
    if (todos.length > 0) {
      definition.todos = todos;
      definition.description = (definition.description || '') + ` (⚠️ ${todos.length} TODOs pending)`;
    }
    
    return definition;
  }
}

export default HelpCommand;