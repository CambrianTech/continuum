/**
 * CoreModule - Essential commands and fluent API
 * Provides: screenshot, share, findUser, diagnostics, etc.
 */

const CommandModule = require('./CommandModule.cjs');
const FluentAPI = require('./FluentAPI.cjs');
const CommandRegistry = require('../commands/CommandRegistry.cjs');

class CoreModule extends CommandModule {
  constructor() {
    super('Core', '1.0.0');
    // Create CommandRegistry for FluentAPI
    this.commandRegistry = new CommandRegistry();
  }

  async discoverAndWireCommands(continuum) {
    const fs = require('fs');
    const path = require('path');
    const commandsDir = path.join(__dirname, '../commands/core');
    
    console.log('🔍 Auto-discovering command modules...');
    
    const entries = fs.readdirSync(commandsDir, { withFileTypes: true });
    const loadedModules = [];
    const loadedLegacy = [];
    
    for (const entry of entries) {
      if (entry.isDirectory()) {
        const modulePath = path.join(commandsDir, entry.name);
        
        // Try new ES module system first
        const serverEntryPath = path.join(modulePath, 'index.server.js');
        const packagePath = path.join(modulePath, 'package.json');
        
        if (fs.existsSync(packagePath)) {
          try {
            // Load module metadata
            const packageInfo = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
            const commandName = packageInfo.continuum?.commandName || packageInfo.name || entry.name;
            
            console.log(`📦 Loading ES module: ${commandName} v${packageInfo.version}`);
            
            // Dynamic import of server entry point
            const moduleUrl = `file://${serverEntryPath}`;
            const serverModule = await import(moduleUrl);
            
            if (serverModule.default?.command) {
              this.registerCommand(commandName, serverModule.default.command);
              loadedModules.push(`${commandName}@${packageInfo.version}`);
              
              // Auto-initialize
              if (serverModule.default.initialize) {
                serverModule.default.initialize('server');
              }
              
              // Store for client-side loading
              this.commandModules = this.commandModules || {};
              this.commandModules[commandName] = {
                packageInfo,
                serverModule: serverModule.default,
                clientEntry: 'index.client.js'
              };
            }
          } catch (error) {
            console.error(`❌ ES Module ${entry.name} failed:`, error.message);
            
            // Fallback to legacy system
            const legacyIndexPath = path.join(modulePath, 'index.cjs');
            if (fs.existsSync(legacyIndexPath)) {
              try {
                const legacyModule = require(legacyIndexPath);
                const commandName = legacyModule.commandName || legacyModule.name || entry.name;
                if (legacyModule.server) {
                  this.registerCommand(commandName, legacyModule.server);
                  loadedModules.push(`${commandName} (legacy)`);
                }
              } catch (legacyError) {
                console.error(`❌ Legacy fallback ${entry.name} failed:`, legacyError.message);
              }
            }
          }
        }
      } else if (entry.name.endsWith('Command.cjs')) {
        // Auto-load standalone legacy commands
        try {
          const commandName = entry.name.replace('Command.cjs', '').toLowerCase();
          this.registerCommand(commandName, require(path.join(commandsDir, entry.name)));
          loadedLegacy.push(commandName);
        } catch (error) {
          console.error(`❌ Legacy ${entry.name} failed:`, error.message);
        }
      }
    }
    
    console.log(`✅ Loaded ${loadedModules.length} ES modules: ${loadedModules.join(', ')}`);
    console.log(`✅ Loaded ${loadedLegacy.length} legacy commands: ${loadedLegacy.join(', ')}`);
    console.log(`📋 Total commands registered: ${this.commands.size}`);
  }

  // Get client module entry points for dynamic ES module loading
  getClientModules() {
    const clientModules = {};
    
    if (this.commandModules) {
      for (const [commandName, moduleInfo] of Object.entries(this.commandModules)) {
        if (moduleInfo.clientEntry) {
          clientModules[commandName] = {
            name: commandName,
            version: moduleInfo.packageInfo?.version || '1.0.0',
            entryPoint: `/src/commands/core/${commandName}/${moduleInfo.clientEntry}`,
            type: 'module',
            capabilities: moduleInfo.packageInfo?.continuum?.capabilities || []
          };
        }
      }
    }
    
    return clientModules;
  }

  // Simple client wiring - just ES module imports  
  getClientWiring() {
    return {
      type: 'es-modules',
      modules: this.getClientModules(),
      loader: 'dynamic-import'
    };
  }

  async initialize(continuum) {
    await super.initialize(continuum);
    
    // Dynamic command discovery and wiring
    await this.discoverAndWireCommands(continuum);

    // Register elegant macros with proper CommandRegistry
    console.log('🔗 Creating FluentAPI macros with CommandRegistry...');
    const macros = FluentAPI.createMacros(this.commandRegistry);
    this.registerMacro('shareScreenshot', macros.shareScreenshot);
    this.registerMacro('quickDiagnostics', macros.quickDiagnostics);
    // Remove hardcoded user-specific macros - use proper chaining instead

    // Create the beautiful fluent API
    this.fluentAPI = FluentAPI.createMacros(this.commandRegistry);
    
    console.log(`✨ Core module loaded with ${this.commands.size} commands and ${this.macros.size} macros`);
    return true;
  }

  // Expose the fluent API for global use
  getFluentAPI() {
    return this.fluentAPI;
  }
}

module.exports = CoreModule;