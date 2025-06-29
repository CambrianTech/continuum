/**
 * Command System Bridge - Live integration of modern TypeScript processor
 * Allows gradual migration while proving the new system works in production
 */

const CommandRegistry = require('../commands/CommandRegistry.cjs');

class CommandSystemBridge {
  constructor() {
    this.legacyRegistry = new CommandRegistry();
    this.modernProcessor = null;
    this.modernProcessorReady = false;
    this.routingEnabled = false;
    this.stats = {
      legacyExecutions: 0,
      modernExecutions: 0,
      migrationErrors: 0
    };
    
    // BACKWARD COMPATIBILITY: Expose commands as Map for WebSocket server and other legacy code
    this.commands = new Map();
    this.commandRegistry = this.legacyRegistry; // Legacy compatibility
    
    console.log('🌉 CommandSystemBridge: Initializing dual-system support');
    this.initializeModernProcessor();
    this.updateCommandsMap();
    
    // TEMPORARY FIX: Manually register selftest command
    this.manuallyRegisterSelfTest();
  }

  /**
   * Initialize the modern TypeScript processor
   */
  async initializeModernProcessor() {
    try {
      console.log('🚀 Loading modern TypeScript CommandProcessor...');
      
      // Dynamic import of TypeScript module using tsx
      require('tsx/cjs');
      const { commandProcessor } = await import('../modules/command-processor/index.ts');
      this.modernProcessor = commandProcessor;
      
      await this.modernProcessor.initialize();
      this.modernProcessorReady = true;
      
      const commands = this.modernProcessor.getAllCommands();
      console.log(`✅ Modern CommandProcessor ready: ${commands.length} TypeScript commands loaded`);
      console.log(`📋 TypeScript commands: ${commands.join(', ')}`);
      
      // Enable routing once modern processor is ready
      this.routingEnabled = true;
      
      // Update commands map with modern processor commands
      this.updateCommandsMap();
      
    } catch (error) {
      console.warn('⚠️  Modern CommandProcessor failed to load:', error.message);
      console.log('🔄 Falling back to legacy CommandRegistry only');
      this.modernProcessorReady = false;
      this.routingEnabled = false;
      
      // Update commands map with only legacy commands
      this.updateCommandsMap();
    }
  }

  /**
   * Route command execution to appropriate processor
   */
  async executeCommand(commandName, params, continuum, encoding = 'utf-8') {
    // If routing is enabled and modern processor has the command, use it
    if (this.routingEnabled && this.modernProcessorReady) {
      const modernDefinition = this.modernProcessor.getDefinition(commandName);
      
      if (modernDefinition) {
        console.log(`🚀 Routing ${commandName} to modern TypeScript processor`);
        
        try {
          // Create context compatible with TypeScript commands
          const context = {
            continuum,
            continuonStatus: continuum?.continuonStatus,
            processor: 'typescript-bridge',
            executionId: this.generateExecutionId(),
            timestamp: new Date()
          };
          
          const result = await this.modernProcessor.executeCommand(commandName, params, context);
          this.stats.modernExecutions++;
          
          console.log(`✅ Modern processor result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
          return result;
          
        } catch (error) {
          console.error(`❌ Modern processor failed for ${commandName}:`, error.message);
          this.stats.migrationErrors++;
          
          // FALLBACK DISABLED: Don't fall back to legacy system
          return {
            success: false,
            message: `Modern processor failed for ${commandName}`,
            error: error.message,
            processor: 'typescript-failed'
          };
        }
      }
    }

    // FALLBACK DISABLED: No legacy CommandRegistry fallback
    console.log(`❌ Command ${commandName} not found in modern processor - fallback disabled`);
    return {
      success: false,
      message: `Command ${commandName} not available in modern processor`,
      error: 'Legacy fallback disabled',
      processor: 'no-fallback'
    };
  }

  /**
   * Get command definition from either system
   */
  getDefinition(commandName) {
    // Try modern first
    if (this.modernProcessorReady) {
      const modernDef = this.modernProcessor.getDefinition(commandName);
      if (modernDef) return modernDef;
    }
    
    // Fall back to legacy
    return this.legacyRegistry.getDefinition(commandName);
  }

  /**
   * Get command function from either system
   */
  getCommand(commandName) {
    // For compatibility, we return a wrapper function
    return async (params, continuum, encoding) => {
      return await this.executeCommand(commandName, params, continuum, encoding);
    };
  }

  /**
   * Get all definitions from both systems
   */
  getAllDefinitions() {
    const legacyDefs = this.legacyRegistry.getAllDefinitions();
    
    if (!this.modernProcessorReady) {
      return legacyDefs;
    }
    
    const modernDefs = this.modernProcessor.getAllDefinitions();
    
    // Merge definitions, with modern taking precedence
    const allDefs = [...legacyDefs];
    const legacyNames = new Set(legacyDefs.map(def => def.name.toLowerCase()));
    
    for (const modernDef of modernDefs) {
      if (!legacyNames.has(modernDef.name.toLowerCase())) {
        allDefs.push(modernDef);
      }
    }
    
    return allDefs;
  }

  /**
   * Get migration statistics
   */
  getStats() {
    const legacyCommands = this.legacyRegistry.getAllDefinitions().length;
    const modernCommands = this.modernProcessorReady ? this.modernProcessor.getAllCommands().length : 0;
    const totalCommands = legacyCommands + modernCommands;
    
    return {
      ...this.stats,
      legacyCommands,
      modernCommands,
      totalCommands,
      modernProcessorReady: this.modernProcessorReady,
      routingEnabled: this.routingEnabled,
      migrationProgress: totalCommands > 0 ? (modernCommands / totalCommands) * 100 : 0
    };
  }

  /**
   * Enable TypeScript-only mode (for final migration)
   */
  enableTypeScriptOnlyMode() {
    if (!this.modernProcessorReady) {
      throw new Error('Cannot enable TypeScript-only mode: modern processor not ready');
    }
    
    console.log('🚀 Enabling TypeScript-only mode - disabling legacy CommandRegistry');
    this.legacyRegistry = null;
    this.routingEnabled = false; // No routing needed - everything goes to modern
    
    return {
      modernCommands: this.modernProcessor.getAllCommands().length,
      message: 'TypeScript-only mode enabled successfully'
    };
  }

  /**
   * TEMPORARY: Manually register selftest command
   */
  manuallyRegisterSelfTest() {
    try {
      const SelfTestCommand = require('../commands/development/selftest/SelfTestCommand.cjs');
      const definition = SelfTestCommand.getDefinition();
      
      console.log('🔧 Manually registering selftest command:', definition.name);
      
      // Add to legacy registry using the same pattern as the registry
      this.legacyRegistry.commands.set(definition.name.toLowerCase(), SelfTestCommand.execute.bind(SelfTestCommand));
      this.legacyRegistry.definitions.set(definition.name.toLowerCase(), definition);
      
      // Update commands map
      this.updateCommandsMap();
      
      console.log('✅ SelfTest command manually registered successfully');
    } catch (error) {
      console.error('❌ Failed to manually register selftest command:', error.message);
    }
  }

  /**
   * Process tool commands from AI responses - required by PlannerAI
   */
  async processToolCommands(response) {
    console.log('🔍 CommandSystemBridge: Processing AI protocol...');
    
    // Use legacy CommandProcessor for tool command processing
    if (this.legacyRegistry.commandProcessor) {
      return await this.legacyRegistry.commandProcessor.processToolCommands(response);
    }
    
    // Fallback: create a temporary CommandProcessor instance
    const CommandProcessor = require('./CommandProcessor.cjs');
    const processor = new CommandProcessor();
    return await processor.processToolCommands(response);
  }

  /**
   * Generate unique execution ID
   */
  generateExecutionId() {
    return `bridge_${Date.now().toString(36)}_${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Update the commands Map for backward compatibility with legacy code
   * This ensures WebSocket server and other code can access commands via .commands.entries()
   */
  updateCommandsMap() {
    this.commands.clear();
    
    // Add legacy commands as wrapper functions
    const legacyCommands = this.legacyRegistry.getAllDefinitions();
    for (const definition of legacyCommands) {
      const commandName = definition.name.toLowerCase();
      
      // Create wrapper that delegates to executeCommand
      const wrapper = {
        execute: async (params) => {
          return await this.executeCommand(commandName, params, this.continuum);
        },
        getDefinition: () => definition
      };
      
      this.commands.set(commandName, wrapper);
    }
    
    // Add modern TypeScript commands if available
    if (this.modernProcessorReady && this.modernProcessor) {
      const modernCommands = this.modernProcessor.getAllCommands();
      for (const commandName of modernCommands) {
        const lowerName = commandName.toLowerCase();
        
        // Modern commands take precedence over legacy
        const wrapper = {
          execute: async (params) => {
            return await this.executeCommand(lowerName, params, this.continuum);
          },
          getDefinition: () => this.modernProcessor.getDefinition(commandName)
        };
        
        this.commands.set(lowerName, wrapper);
      }
    }
    
    console.log(`🗺️  CommandSystemBridge: Updated commands map with ${this.commands.size} commands`);
  }

  /**
   * Wait for modern processor to be ready
   */
  async waitForModernProcessor(timeoutMs = 5000) {
    const startTime = Date.now();
    
    while (!this.modernProcessorReady && (Date.now() - startTime) < timeoutMs) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    
    if (!this.modernProcessorReady) {
      throw new Error('Modern processor failed to initialize within timeout');
    }
    
    return true;
  }
}

module.exports = CommandSystemBridge;