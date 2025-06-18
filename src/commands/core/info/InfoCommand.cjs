/**
 * InfoCommand - Parent class for information display commands
 * Provides common functionality for help, agents, and other info commands
 */

const BaseCommand = require('../../BaseCommand.cjs');

class InfoCommand extends BaseCommand {
  // Common helper methods for info commands
  
  static getDefinition() {
    // README-driven: Read definition from README.md
    const fs = require('fs');
    const path = require('path');
    
    try {
      const readmePath = path.join(__dirname, 'README.md');
      const readme = fs.readFileSync(readmePath, 'utf8');
      return this.parseReadmeDefinition(readme);
    } catch (error) {
      // Fallback definition if README.md not found
      return {
        name: 'info',
        description: 'Display system information and server status',
        icon: 'ℹ️',
        category: 'system',
        parameters: {
          section: { type: 'string', required: false, description: 'Information section to display' }
        }
      };
    }
  }
  
  static parseReadmeDefinition(readme) {
    // Parse README.md for command definition (shared with HelpCommand)
    const lines = readme.split('\n');
    const definition = { parameters: {} };
    
    let inDefinition = false;
    let inParams = false;
    let inTodos = false;
    const todos = [];
    
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
    
    // Add TODOs to definition
    if (todos.length > 0) {
      definition.todos = todos;
      definition.description += ` (⚠️ ${todos.length} TODOs pending)`;
    }
    
    return definition;
  }
  
  static async execute(params, continuum) {
    const options = this.parseParams(params);
    
    this.displayHeader('💻 System Information', 'Continuum Server Status');
    
    // System information
    const systemInfo = this.getSystemInfo();
    const serverInfo = this.getServerInfo(continuum);
    
    if (!options.section || options.section === 'overview') {
      this.displaySection('🖥️ SYSTEM', systemInfo);
      this.displaySection('🚀 SERVER', serverInfo);
    } else if (options.section === 'system') {
      this.displaySection('🖥️ SYSTEM', systemInfo);
    } else if (options.section === 'server') {
      this.displaySection('🚀 SERVER', serverInfo);
    } else if (options.section === 'memory') {
      const memoryInfo = this.getMemoryInfo();
      this.displaySection('💾 MEMORY', memoryInfo);
    } else if (options.section === 'connections') {
      const connectionInfo = this.getConnectionInfo(continuum);
      this.displaySection('🔗 CONNECTIONS', connectionInfo);
    }
    
    return this.createSuccessResult({ 
      system: systemInfo,
      server: serverInfo,
      version: this.getVersion() 
    }, 'System information displayed');
  }
  
  static getSystemInfo() {
    const os = require('os');
    return `  Platform: ${os.platform()} ${os.arch()}
  Node.js: ${process.version}
  OS: ${os.type()} ${os.release()}
  CPU: ${os.cpus()[0].model} (${os.cpus().length} cores)
  Uptime: ${Math.floor(os.uptime() / 3600)}h ${Math.floor((os.uptime() % 3600) / 60)}m`;
  }
  
  static getServerInfo(continuum) {
    const uptime = process.uptime();
    const uptimeFormatted = `${Math.floor(uptime / 3600)}h ${Math.floor((uptime % 3600) / 60)}m ${Math.floor(uptime % 60)}s`;
    
    return `  Version: ${this.getVersion()}
  Uptime: ${uptimeFormatted}
  PID: ${process.pid}
  Working Directory: ${process.cwd()}
  Node Args: ${process.argv.slice(2).join(' ') || 'none'}`;
  }
  
  static getMemoryInfo() {
    const memUsage = process.memoryUsage();
    const formatBytes = (bytes) => (bytes / 1024 / 1024).toFixed(2) + ' MB';
    
    return `  RSS: ${formatBytes(memUsage.rss)}
  Heap Used: ${formatBytes(memUsage.heapUsed)}
  Heap Total: ${formatBytes(memUsage.heapTotal)}
  External: ${formatBytes(memUsage.external)}`;
  }
  
  static getConnectionInfo(continuum) {
    return `  WebSocket Server: Active
  Port: 9000 (default)
  Active Connections: ${continuum?.webSocketServer?.clients?.size || 0}
  Command Bus: Ready`;
  }
  
  static getVersion() {
    try {
      const fs = require('fs');
      const path = require('path');
      const packagePath = path.join(process.cwd(), 'package.json');
      const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      return packageJson.version;
    } catch (error) {
      return 'unknown';
    }
  }
  
  static displayCommandRegistry() {
    try {
      const CommandRegistry = require('../CommandRegistry.cjs');
      const registry = new CommandRegistry();
      
      console.log('\n🎓 COMMAND REFERENCE:');
      console.log('  Available Commands (dynamically loaded):');
      
      for (const [name, definition] of registry.definitions.entries()) {
        console.log(`    ${definition.icon} ${name} - ${definition.description}`);
        if (definition.examples && definition.examples.length > 0) {
          console.log(`      Example: ${definition.examples[0]}`);
        }
      }
    } catch (error) {
      console.log('\n  Commands: Run continuum to see dynamically loaded command list');
    }
  }
  
  static displayHeader(title, subtitle = null) {
    const version = this.getVersion();
    console.log(`\n${title} v${version}${subtitle ? ' - ' + subtitle : ''}\n`);
  }
  
  static displaySection(title, content) {
    console.log(`${title}:`);
    console.log(content);
    console.log('');
  }
  
  // Standard sections that both help and agents commands use
  static getBuiltInTestingSection() {
    return `🧪 BUILT-IN SYSTEM TESTING:
  continuum.diagnostics()                     # Run complete system test with fresh logs
  continuum.diagnostics('screenshot')         # Test screenshot system only
  continuum.diagnostics('isolated')           # Run in isolated subdirectories
  # Tests create unique subdirs, fresh logs, and can't be fooled by old files`;
  }
  
  static getTrustTheProcessSection() {
    return `🚨 CRITICAL: TRUST THE PROCESS - Follow this exactly:
  cd python-client && python trust_the_process.py    # Single command does everything!

📋 BABY STEPS DEVELOPMENT CYCLE:
  1️⃣  Clear old data: Avoid confusion/cheating
  2️⃣  Make small change: Max 50 lines, one file only  
  3️⃣  Bump version: Auto-increment for tracking
  4️⃣  Test immediately: Screenshot + console + validation ← AUTOMATED
  5️⃣  Fix ANY errors: Zero tolerance for breaking system
  6️⃣  Commit when stable: Only when everything works`;
  }
  
  static getSafetyRulesSection() {
    return `🛡️ SAFETY RULES (Never Break These):
  • NEVER break the system (immediate rollback if anything fails)
  • NEVER commit broken code (test everything first)
  • ALWAYS increase stability (every commit improves system)
  • ALWAYS follow surgical precision (small, careful changes)
  • ALWAYS edit existing files (avoid creating new files)`;
  }
  
  static getSuccessCriteriaSection() {
    return `🎯 SUCCESS CRITERIA (All Must Pass):
  • All tests pass ✅
  • No console errors ✅
  • Screenshots capture correctly ✅
  • Version numbers match ✅
  • System more stable than before ✅`;
  }
}

module.exports = InfoCommand;