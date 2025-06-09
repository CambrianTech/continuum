/**
 * Command Registry
 * Central registry for all Continuum commands
 */

const fs = require('fs');
const path = require('path');

class CommandRegistry {
  constructor() {
    this.commands = new Map();
    this.definitions = new Map();
    this.loadCommands();
  }
  
  loadCommands() {
    console.log('📚 Loading command definitions...');
    
    // Load core commands
    this.loadCommandsFromDirectory(path.join(__dirname, 'core'));
    this.loadCommandsFromDirectory(path.join(__dirname, 'automation'));
    this.loadCommandsFromDirectory(path.join(__dirname, 'browser'));
    this.loadCommandsFromDirectory(path.join(__dirname, 'gaming'));
    
    console.log(`📚 Loaded ${this.commands.size} commands`);
  }
  
  loadCommandsFromDirectory(dir) {
    if (!fs.existsSync(dir)) {
      console.log(`📂 Directory not found: ${dir}`);
      return;
    }
    
    const files = fs.readdirSync(dir);
    
    for (const file of files) {
      if (file.endsWith('.cjs') || file.endsWith('.js')) {
        try {
          const CommandClass = require(path.join(dir, file));
          
          if (CommandClass.getDefinition && CommandClass.execute) {
            const definition = CommandClass.getDefinition();
            const commandName = definition.name.toUpperCase();
            
            this.commands.set(commandName, CommandClass.execute.bind(CommandClass));
            this.definitions.set(commandName, definition);
            
            console.log(`📚 Loaded command: ${commandName} (${definition.category})`);
          }
        } catch (error) {
          console.error(`❌ Failed to load command from ${file}:`, error.message);
        }
      }
    }
  }
  
  getCommand(name) {
    return this.commands.get(name.toUpperCase());
  }
  
  getDefinition(name) {
    return this.definitions.get(name.toUpperCase());
  }
  
  getAllDefinitions() {
    return Array.from(this.definitions.values());
  }
  
  getCommandsByCategory(category) {
    return Array.from(this.definitions.values())
      .filter(def => def.category === category);
  }
  
  getCategories() {
    return [...new Set(Array.from(this.definitions.values()).map(def => def.category))];
  }
  
  executeCommand(commandName, params, continuum, encoding = 'utf-8') {
    const command = this.getCommand(commandName);
    
    if (!command) {
      return Promise.resolve({
        success: false,
        error: `Command '${commandName}' not found`
      });
    }
    
    return command(params, continuum, encoding);
  }
  
  generateManPage(commandName = null) {
    if (commandName) {
      // Single command manual
      const definition = this.getDefinition(commandName);
      if (!definition) {
        return `❌ Command '${commandName}' not found. Use 'continuum --man' to list all commands.`;
      }
      
      return `
📚 ${definition.icon} ${definition.name} - ${definition.description}

USAGE:
  {"command": "${definition.name}", "params": "${definition.params}"}

DESCRIPTION:
  ${definition.usage}

EXAMPLES:
${definition.examples.map(ex => `  curl -X POST http://localhost:9000/connect -d '{"command": "${definition.name}", ${ex.replace('{}', '"params": ""')}}'`).join('\n')}

PARAMETERS:
  ${definition.params || 'No parameters required'}

CATEGORY: ${definition.category}

For all commands: continuum --man
For specific command: continuum --man <COMMAND_NAME>
`;
    }
    
    // Full manual
    const categories = this.getCategories();
    let manual = `📚 CONTINUUM API REFERENCE - Complete Command Manual\n\n`;
    
    // Add AI Development Strategy
    manual += `🤖 AI DEVELOPMENT STRATEGY (ESSENTIAL FOR CLAUDE CODE):\n`;
    manual += `  When fixing UI bugs or implementing features, use this PROVEN WORKFLOW:\n\n`;
    manual += `  1. ITERATIVE DEVELOPMENT WITH VISUAL FEEDBACK:\n`;
    manual += `     • Execute JavaScript via base64 encoding to avoid quote escaping\n`;
    manual += `     • Take screenshots after each change to verify results\n`;
    manual += `     • Use WebSocket callbacks for event-driven development\n`;
    manual += `     • Iterate quickly with visual confirmation\n\n`;
    manual += `  2. EXAMPLE WORKFLOW FOR UI FIXES:\n`;
    manual += `     # Step 1: Encode and execute JavaScript\n`;
    manual += `     echo 'console.log("Testing..."); const elem = document.querySelector(".button");' | base64\n`;
    manual += `     curl -X POST http://localhost:9000/connect -d '{"command": "BROWSER_JS", "params": "BASE64_HERE", "encoding": "base64"}'\n\n`;
    manual += `     # Step 2: Take screenshot to see current state\n`;
    manual += `     curl -X POST http://localhost:9000/connect -d '{"command": "SCREENSHOT"}'\n\n`;
    manual += `     # Step 3: Create/fix UI elements with proper styling\n`;
    manual += `     echo 'const pane = document.createElement("div"); pane.innerHTML = "<div style=\\"background: rgba(0, 255, 136, 0.15); backdrop-filter: blur(15px);\\">Content</div>"; document.body.appendChild(pane);' | base64\n`;
    manual += `     curl -X POST http://localhost:9000/connect -d '{"command": "BROWSER_JS", "params": "BASE64_HERE", "encoding": "base64"}'\n\n`;
    manual += `  3. DESIGN PRINCIPLES:\n`;
    manual += `     • Glass morphism with cyan/blue gradients: rgba(0, 255, 136, 0.15)\n`;
    manual += `     • Angular video game aesthetics with clip-path polygons\n`;
    manual += `     • Smooth animations with cubic-bezier(0.4, 0, 0.2, 1)\n`;
    manual += `     • Small contextual panes, not massive overlays\n\n`;
    manual += `  4. CONSOLE MONITORING & ERROR VISIBILITY:\n`;
    manual += `     • All browser console.log/error output streams to Continuum terminal\n`;
    manual += `     • JavaScript errors appear in real-time for debugging\n`;
    manual += `     • WebSocket connection issues indicate browser connectivity problems\n`;
    manual += `     • Use console.log() in JavaScript for debugging feedback\n`;
    manual += `     \n`;
    manual += `     ⚠️ CRITICAL: API "success" ≠ JavaScript execution success\n`;
    manual += `     • Command success only means JavaScript was sent to browser\n`;
    manual += `     • Execution happens asynchronously - watch for console output\n`;
    manual += `     • Missing expected console.log indicates script syntax/execution failure\n\n`;
    manual += `  ⚡ CRITICAL: Fresh AI instances should IMMEDIATELY use this workflow when asked to fix UI\n`;
    manual += `      This approach delivers visual results in seconds, not hours of blind coding.\n\n`;
    
    categories.forEach(category => {
      const commands = this.getCommandsByCategory(category);
      manual += `${this.getCategoryIcon(category)} ${category.toUpperCase()} COMMANDS:\n`;
      
      commands.forEach(def => {
        manual += `  ${def.name.padEnd(25)} ${def.description}\n`;
      });
      manual += '\n';
    });
    
    manual += `📋 USAGE EXAMPLES:\n`;
    manual += `  # Get single command help:\n`;
    manual += `  continuum --man SCREENSHOT\n\n`;
    manual += `  # Execute command:\n`;
    manual += `  curl -X POST http://localhost:9000/connect -d '{"command": "SCREENSHOT"}'\n\n`;
    manual += `  # Use base64 for complex JavaScript:\n`;
    manual += `  echo 'console.log("test")' | base64\n`;
    manual += `  curl -X POST http://localhost:9000/connect -d '{"command": "BROWSER_JS", "params": "BASE64_HERE", "encoding": "base64"}'\n\n`;
    manual += `📚 Available commands: ${Array.from(this.definitions.keys()).join(', ')}\n`;
    
    return manual;
  }
  
  getCategoryIcon(category) {
    const icons = {
      'Core': '🔧',
      'Automation': '🎮',
      'Browser': '🌐',
      'Gaming': '🎯',
      'Visual': '👁️'
    };
    return icons[category] || '📁';
  }
  
  // Generate API documentation for endpoints
  generateAPISchema() {
    const schema = {
      commands: {},
      categories: this.getCategories(),
      endpoints: {
        'POST /connect': 'Execute commands',
        'GET /connect': 'Get capabilities',
        'GET /api/commands': 'Get this schema'
      }
    };
    
    this.getAllDefinitions().forEach(def => {
      schema.commands[def.name] = {
        description: def.description,
        category: def.category,
        params: def.params,
        usage: def.usage,
        examples: def.examples
      };
    });
    
    return schema;
  }
}

module.exports = CommandRegistry;