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
    this.initialized = false;
    this.initPromise = this.loadCommands().then(() => {
      this.initialized = true;
    }).catch(error => {
      console.error('❌ Failed to load commands:', error.message);
      this.initialized = true; // Mark as initialized even on failure
    });
  }

  async waitForInitialization() {
    await this.initPromise;
    return this.initialized;
  }
  
  async loadCommands() {
    console.log('📚 Loading command definitions...');
    
    // Load commands from all module directories
    const commandsDir = __dirname;
    const moduleDirectories = fs.readdirSync(commandsDir, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name)
      .filter(name => !name.startsWith('.') && !name.includes('test'));
    
    console.log(`📁 Found ${moduleDirectories.length} command modules: ${moduleDirectories.join(', ')}`);
    
    for (const moduleName of moduleDirectories) {
      console.log(`📦 Loading module: ${moduleName}`);
      await this.loadCommandsFromDirectory(path.join(__dirname, moduleName));
    }
    
    console.log(`📚 Loaded ${this.commands.size} commands`);
  }
  
  async loadCommandsFromDirectory(dir) {
    if (!fs.existsSync(dir)) {
      console.log(`📂 Directory not found: ${dir}`);
      return;
    }
    
    const files = fs.readdirSync(dir);
    let loadedCount = 0;
    let errorCount = 0;
    
    for (const file of files) {
      const filePath = path.join(dir, file);
      const stat = fs.statSync(filePath);
      
      // If it's a directory, recursively load from subdirectory
      if (stat.isDirectory()) {
        // Skip test directories
        if (file.includes('test') || file.includes('Test')) {
          console.log(`⏩ Skipping test directory: ${file}`);
          continue;
        }
        
        await this.loadCommandsFromDirectory(filePath);
        continue;
      }
      
      // Skip test files, utility files, and non-command files
      if (file.includes('test') || file.includes('Test') || file.includes('-test') || 
          file.includes('runner') || file.includes('validation-test') ||
          file.startsWith('index.') || file.includes('Utils') || file.includes('utils') ||
          file.includes('Config') || file.includes('config') || file.includes('Definition') ||
          file.includes('Animator') || file.includes('Renderer') || 
          file.includes('.client.') || file.includes('.server.')) {
        console.log(`⏩ Skipping non-command file: ${file}`);
        continue;
      }
      
      if (file.endsWith('.cjs') || file.endsWith('.js') || file.endsWith('.ts')) {
        try {
          let CommandClass;
          
          // Handle different module types gracefully
          if (file.endsWith('.cjs')) {
            // CommonJS module - use require
            delete require.cache[require.resolve(filePath)];
            CommandClass = require(filePath);
          } else if (file.endsWith('.ts')) {
            // TypeScript module - compile and require (future)
            console.log(`🔷 TypeScript command detected: ${file} (compilation not yet implemented)`);
            continue;
          } else if (file.endsWith('.js')) {
            // Could be ES module or CommonJS - try both
            try {
              // First try CommonJS (most existing commands)
              delete require.cache[require.resolve(filePath)];
              CommandClass = require(filePath);
            } catch (requireError) {
              // If require fails with "Cannot use import statement", try ES module import
              if (requireError.message.includes('Cannot use import statement') || 
                  requireError.message.includes('Unexpected token')) {
                try {
                  console.log(`🔄 Attempting ES module import for: ${file}`);
                  const moduleUrl = `file://${filePath}`;
                  const esModule = await import(moduleUrl);
                  CommandClass = esModule.default || esModule;
                } catch (importError) {
                  throw new Error(`Failed both require() and import(): ${requireError.message} | ${importError.message}`);
                }
              } else {
                throw requireError;
              }
            }
          }
          
          // Validate command class structure
          if (!CommandClass) {
            console.warn(`⚠️ Command file ${file} exported null/undefined`);
            continue;
          }
          
          if (!CommandClass.getDefinition || typeof CommandClass.getDefinition !== 'function') {
            console.warn(`⚠️ Command ${file} missing getDefinition() method`);
            continue;
          }
          
          if (!CommandClass.execute || typeof CommandClass.execute !== 'function') {
            console.warn(`⚠️ Command ${file} missing execute() method`);
            continue;
          }
          
          // Validate command definition
          let definition;
          try {
            definition = CommandClass.getDefinition();
          } catch (defError) {
            console.error(`❌ Command ${file} getDefinition() failed:`, defError.message);
            continue;
          }
          
          if (!definition || !definition.name) {
            console.error(`❌ Command ${file} has invalid definition (missing name)`);
            continue;
          }
          
          const commandName = definition.name.toUpperCase();
          
          // Check for duplicate commands
          if (this.commands.has(commandName)) {
            console.warn(`⚠️ Duplicate command: ${commandName} (overriding previous)`);
          }
          
          // Register command
          this.commands.set(commandName, CommandClass.execute.bind(CommandClass));
          this.definitions.set(commandName, definition);
          
          console.log(`📚 Loaded command: ${commandName} (${definition.category || 'uncategorized'})`);
          loadedCount++;
          
        } catch (error) {
          errorCount++;
          console.error(`❌ Failed to load command from ${file}:`, error.message);
          
          if (error.code === 'MODULE_NOT_FOUND') {
            console.error(`   Missing dependency in ${file}. Check require() statements.`);
            console.error(`   Full path: ${error.requireStack?.[0] || 'unknown'}`);
          } else if (error instanceof SyntaxError) {
            console.error(`   Syntax error in ${file}. Check JavaScript syntax.`);
          } else {
            console.error(`   Error details:`, error.stack);
          }
          
          // Continue loading other commands instead of stopping
          console.log(`   ⏭️ Skipping ${file} - continuing with other commands...`);
        }
      }
    }
    
    console.log(`📂 Directory ${path.basename(dir)}: ${loadedCount} loaded, ${errorCount} failed`);
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
    console.log(`🎯 CommandRegistry: executing ${commandName}`);
    console.log(`🔬 PROBE: CommandRegistry.executeCommand called for ${commandName}`);
    
    // Publish command execution event to EventBus
    if (continuum && continuum.eventBus) {
      console.log(`📡 CommandRegistry: EventBus found, publishing event`);
      continuum.eventBus.processMessage('command_execution', {
        command: commandName,
        params: params,
        timestamp: new Date().toISOString()
      }, 'command-registry');
    } else {
      console.log(`📡 CommandRegistry: No EventBus (continuum: ${!!continuum}, eventBus: ${!!continuum?.eventBus})`);
    }
    const command = this.getCommand(commandName);
    
    if (!command) {
      return Promise.resolve({
        success: false,
        error: `Command '${commandName}' not found`
      });
    }
    
    // Generate trace ID for CommandRegistry execution  
    const registryTraceId = `registry_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    console.log(`🆔 REGISTRY_TRACE_START: ${registryTraceId} - CommandRegistry executing ${commandName}`);
    console.log(`🔧 REGISTRY_TRACE_${registryTraceId}: Raw params type: ${typeof params}, value: ${params}`);
    
    // Parse JSON parameters if they're a string
    let parsedParams = params;
    if (typeof params === 'string') {
      try {
        parsedParams = JSON.parse(params);
        console.log(`✅ REGISTRY_TRACE_${registryTraceId}: Successfully parsed JSON params:`, parsedParams);
      } catch (parseError) {
        console.log(`⚠️ REGISTRY_TRACE_${registryTraceId}: Could not parse params as JSON for ${commandName}, using as string`);
        parsedParams = params;
      }
    }
    
    console.log(`🔧 REGISTRY_TRACE_${registryTraceId}: Calling ${commandName} with final params:`, parsedParams);
    return command(parsedParams, continuum, encoding);
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