/**
 * Legacy Integration Layer
 * Connects TypeScript CommandProcessorBridge with existing JavaScript infrastructure
 * Provides seamless migration path without breaking existing functionality
 */

const path = require('path');

class LegacyCommandProcessorIntegration {
  constructor() {
    this.bridge = null;
    this.legacyRegistry = null;
    this.initialized = false;
    
    console.log('🔗 Legacy Command Processor Integration initialized');
  }

  /**
   * Initialize with legacy command registry
   */
  async initialize(legacyRegistry) {
    try {
      console.log('🚀 Initializing TypeScript-Legacy integration...');
      
      this.legacyRegistry = legacyRegistry;
      
      // Dynamically import TypeScript bridge
      const bridgeModule = await import('./CommandProcessorBridge.js');
      this.bridge = new bridgeModule.CommandProcessorBridge({
        migrationMode: 'parallel',
        migrationPercentage: 25, // Start conservatively
        logLevel: 'info',
        fallbackOnError: true
      });

      // Connect legacy registry to bridge
      this.bridge.setLegacyRegistry(legacyRegistry);
      
      // Initialize the bridge
      await this.bridge.initialize();
      
      this.initialized = true;
      console.log('✅ TypeScript-Legacy integration complete');
      
      // Log integration status
      const health = this.bridge.getHealth();
      console.log('📊 Integration Status:', {
        typescript: health.typescript.commands,
        legacy: health.legacy.commands,
        mode: health.bridge.config.migrationMode
      });
      
    } catch (error) {
      console.error('❌ Failed to initialize TypeScript integration:', error.message);
      this.initialized = false;
      throw error;
    }
  }

  /**
   * Execute command with intelligent routing
   */
  async executeCommand(commandName, params, context) {
    if (!this.initialized || !this.bridge) {
      // Fallback to legacy only
      console.log('⚠️ TypeScript bridge not available, using legacy fallback');
      return await this.legacyRegistry.executeCommand(commandName, params, context);
    }

    try {
      return await this.bridge.executeCommand(commandName, params, context);
    } catch (error) {
      console.error(`❌ Bridge execution failed for ${commandName}:`, error.message);
      
      // Fallback to legacy
      console.log('🔄 Falling back to legacy execution');
      return await this.legacyRegistry.executeCommand(commandName, params, context);
    }
  }

  /**
   * Get command definition (try both systems)
   */
  getDefinition(commandName) {
    if (this.initialized && this.bridge) {
      return this.bridge.getCommandDefinition(commandName);
    }
    
    return this.legacyRegistry?.getDefinition?.(commandName);
  }

  /**
   * Get all available commands
   */
  getAllCommands() {
    if (this.initialized && this.bridge) {
      return this.bridge.getAllCommands();
    }
    
    return this.legacyRegistry?.getAllCommands?.() || [];
  }

  /**
   * Check if command exists in any system
   */
  hasCommand(commandName) {
    const commands = this.getAllCommands();
    return commands.includes(commandName) || commands.includes(commandName.toLowerCase());
  }

  /**
   * Get integration health and statistics
   */
  getIntegrationHealth() {
    if (!this.initialized || !this.bridge) {
      return {
        status: 'legacy-only',
        typescript: false,
        legacy: true,
        bridge: null
      };
    }

    const health = this.bridge.getHealth();
    const stats = this.bridge.getStats();

    return {
      status: 'integrated',
      typescript: health.typescript.available,
      legacy: health.legacy.available,
      bridge: {
        mode: health.bridge.config.migrationMode,
        percentage: health.bridge.config.migrationPercentage,
        stats: {
          total: stats.totalCommands,
          typescript: stats.typescriptCommands,
          legacy: stats.legacyCommands,
          migrations: stats.successfulMigrations,
          fallbacks: stats.fallbacks,
          errors: stats.errors
        }
      }
    };
  }

  /**
   * Update migration settings
   */
  updateMigrationSettings(settings) {
    if (this.initialized && this.bridge) {
      this.bridge.updateConfig(settings);
      console.log('🔧 Migration settings updated:', settings);
    } else {
      console.warn('⚠️ Cannot update migration settings - bridge not initialized');
    }
  }

  /**
   * Set specific command migration route
   */
  setCommandRoute(commandName, route) {
    if (this.initialized && this.bridge) {
      this.bridge.setMigrationRoute(commandName, route);
      console.log(`🎯 Command route set: ${commandName} → ${route}`);
    } else {
      console.warn('⚠️ Cannot set command route - bridge not initialized');
    }
  }

  /**
   * Get migration recommendations
   */
  getMigrationRecommendations() {
    if (!this.initialized || !this.bridge) {
      return {
        status: 'bridge-unavailable',
        recommendations: ['Initialize TypeScript bridge first']
      };
    }

    const health = this.bridge.getHealth();
    const stats = this.bridge.getStats();
    const recommendations = [];

    // Analyze performance
    if (stats.fallbacks > stats.successfulMigrations) {
      recommendations.push('High fallback rate detected - check TypeScript command implementations');
    }

    if (stats.errors > 0) {
      recommendations.push('Command execution errors detected - review error logs');
    }

    // Analyze coverage
    const tsCommands = health.typescript.commands;
    const legacyCommands = health.legacy.commands;
    
    if (tsCommands < legacyCommands * 0.5) {
      recommendations.push('Low TypeScript coverage - prioritize core command migration');
    }

    // Migration strategy
    if (health.bridge.config.migrationPercentage < 50 && stats.successfulMigrations > 10) {
      recommendations.push('Consider increasing migration percentage for better coverage');
    }

    return {
      status: 'analyzed',
      stats,
      coverage: {
        typescript: tsCommands,
        legacy: legacyCommands,
        percentage: Math.round((tsCommands / (tsCommands + legacyCommands)) * 100)
      },
      recommendations
    };
  }
}

module.exports = LegacyCommandProcessorIntegration;