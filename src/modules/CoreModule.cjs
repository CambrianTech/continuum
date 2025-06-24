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
    
    try { console.log('🔍 Auto-discovering command modules...'); } catch(e) {}
    
    let entries = [];
    const loadedModules = [];
    const loadedLegacy = [];
    
    try {
      entries = fs.readdirSync(commandsDir, { withFileTypes: true });
    } catch (error) {
      try { console.error(`❌ Cannot read commands directory: ${error.message}`); } catch(e) {}
      return; // Don't crash the daemon, just skip command loading
    }
    
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
            
            try { console.log(`📦 Loading ES module: ${commandName} v${packageInfo.version}`); } catch(e) {}
            
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
            try { console.error(`❌ ES Module ${entry.name} failed:`, error.message); } catch(e) {}
            
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
                try { console.error(`❌ Legacy fallback ${entry.name} failed:`, legacyError.message); } catch(e) {}
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
          try { console.error(`❌ Legacy ${entry.name} failed:`, error.message); } catch(e) {}
        }
      }
    }
    
    try { console.log(`✅ Loaded ${loadedModules.length} ES modules: ${loadedModules.join(', ')}`); } catch(e) {}
    try { console.log(`✅ Loaded ${loadedLegacy.length} legacy commands: ${loadedLegacy.join(', ')}`); } catch(e) {}
    try { console.log(`📋 Total commands registered: ${this.commands.size}`); } catch(e) {}
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
    try {
      await super.initialize(continuum);
      
      // Wait for CommandRegistry to finish loading
      try {
        await this.commandRegistry.waitForInitialization();
        try { console.log('✅ CommandRegistry initialized successfully'); } catch(e) {}
      } catch (error) {
        try { console.error(`❌ CommandRegistry initialization failed: ${error.message}`); } catch(e) {}
      }
      
      // Dynamic command discovery and wiring - don't let this crash the daemon
      try {
        await this.discoverAndWireCommands(continuum);
      } catch (error) {
        try { console.error(`❌ Command discovery failed: ${error.message}`); } catch(e) {}
        // Continue with daemon startup even if command loading fails
      }

      // Register elegant macros with proper CommandRegistry
      try { console.log('🔗 Creating FluentAPI macros with CommandRegistry...'); } catch(e) {}
      
      try {
        const macros = FluentAPI.createMacros(this.commandRegistry);
        this.registerMacro('shareScreenshot', macros.shareScreenshot);
        this.registerMacro('quickDiagnostics', macros.quickDiagnostics);
        
        // Create the beautiful fluent API
        this.fluentAPI = FluentAPI.createMacros(this.commandRegistry);
      } catch (error) {
        try { console.error(`❌ FluentAPI creation failed: ${error.message}`); } catch(e) {}
        // Continue without FluentAPI if it fails
      }
      
      try { console.log(`✨ Core module loaded with ${this.commands.size} commands and ${this.macros.size} macros`); } catch(e) {}
      return true;
    } catch (error) {
      try { console.error(`❌ CoreModule initialization failed: ${error.message}`); } catch(e) {}
      return false; // Return false but don't crash
    }
  }

  // Expose the fluent API for global use
  getFluentAPI() {
    return this.fluentAPI;
  }
}

module.exports = CoreModule;