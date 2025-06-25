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
    
    console.log('🌉 CommandSystemBridge: Initializing dual-system support');
    this.initializeModernProcessor();
  }

  /**
   * Initialize the modern TypeScript processor
   */
  async initializeModernProcessor() {
    try {
      console.log('🚀 Loading modern TypeScript CommandProcessor...');
      
      // Dynamic import of TypeScript module
      const { commandProcessor } = await import('../modules/command-processor/index.js');
      this.modernProcessor = commandProcessor;
      
      await this.modernProcessor.initialize();
      this.modernProcessorReady = true;
      
      const commands = this.modernProcessor.getAllCommands();
      console.log(`✅ Modern CommandProcessor ready: ${commands.length} TypeScript commands loaded`);
      console.log(`📋 TypeScript commands: ${commands.join(', ')}`);
      
      // Enable routing once modern processor is ready
      this.routingEnabled = true;
      
    } catch (error) {
      console.warn('⚠️  Modern CommandProcessor failed to load:', error.message);
      console.log('🔄 Falling back to legacy CommandRegistry only');
      this.modernProcessorReady = false;
      this.routingEnabled = false;
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
          
          // Fall through to legacy system
          console.log(`🔄 Falling back to legacy processor for ${commandName}`);
        }
      }
    }

    // Use legacy CommandRegistry
    console.log(`🔄 Using legacy CommandRegistry for ${commandName}`);
    this.stats.legacyExecutions++;
    
    return await this.legacyRegistry.executeCommand(commandName, params, continuum, encoding);
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
   * Generate unique execution ID
   */
  generateExecutionId() {
    return `bridge_${Date.now().toString(36)}_${Math.random().toString(36).substring(2, 8)}`;
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