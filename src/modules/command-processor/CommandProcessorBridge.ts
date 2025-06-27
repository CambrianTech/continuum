/**
 * CommandProcessorBridge - TypeScript-Legacy Integration Bridge
 * Provides seamless migration path from legacy JavaScript commands to TypeScript
 * Handles traffic routing, fallback mechanisms, and gradual migration
 */

import { TypeScriptCommandProcessor } from './CommandProcessor.js';
import { CommandResult } from './types.js';
import { CommandContext } from '../../commands/BaseCommand.js';

export interface BridgeConfig {
  enableTypeScript: boolean;
  enableLegacy: boolean;
  migrationMode: 'off' | 'parallel' | 'typescript-first' | 'typescript-only';
  logLevel: 'debug' | 'info' | 'warn' | 'error';
  fallbackOnError: boolean;
  migrationPercentage: number; // 0-100, percentage of traffic to route to TypeScript
}

export interface BridgeStats {
  totalCommands: number;
  typescriptCommands: number;
  legacyCommands: number;
  migrationRoutes: number;
  successfulMigrations: number;
  fallbacks: number;
  errors: number;
}

export class CommandProcessorBridge {
  private typeScriptProcessor: TypeScriptCommandProcessor;
  private legacyRegistry: any; // Will be injected
  private config: BridgeConfig;
  private stats: BridgeStats;
  private migrationRoutes = new Map<string, 'typescript' | 'legacy'>();

  constructor(config: Partial<BridgeConfig> = {}) {
    this.config = {
      enableTypeScript: true,
      enableLegacy: true,
      migrationMode: 'parallel',
      logLevel: 'info',
      fallbackOnError: true,
      migrationPercentage: 25, // Start with 25% traffic to TypeScript
      ...config
    };

    this.stats = {
      totalCommands: 0,
      typescriptCommands: 0,
      legacyCommands: 0,
      migrationRoutes: 0,
      successfulMigrations: 0,
      fallbacks: 0,
      errors: 0
    };

    this.typeScriptProcessor = new TypeScriptCommandProcessor({
      logLevel: this.config.logLevel,
      enableCaseInsensitive: true
    });

    this.log('info', '🌉 CommandProcessorBridge initialized');
  }

  /**
   * Set legacy command registry for fallback
   */
  setLegacyRegistry(registry: any): void {
    this.legacyRegistry = registry;
    this.log('info', '🔗 Legacy registry connected to bridge');
  }

  /**
   * Initialize both processors
   */
  async initialize(): Promise<void> {
    this.log('info', '🚀 Initializing CommandProcessorBridge...');

    try {
      // Initialize TypeScript processor
      if (this.config.enableTypeScript) {
        await this.typeScriptProcessor.initialize();
        this.log('info', '✅ TypeScript processor initialized');
      }

      // Wait for legacy registry if provided
      if (this.config.enableLegacy && this.legacyRegistry) {
        await this.legacyRegistry.waitForInitialization();
        this.log('info', '✅ Legacy registry initialized');
      }

      // Analyze command overlap for migration routing
      await this.analyzeCommandOverlap();

      this.log('info', '🌉 CommandProcessorBridge ready');
    } catch (error) {
      this.log('error', `❌ Bridge initialization failed: ${error.message}`);
      throw error;
    }
  }

  /**
   * Execute command with intelligent routing
   */
  async executeCommand<T = any, R = any>(
    commandName: string,
    params: T,
    context?: CommandContext
  ): Promise<CommandResult> {
    this.stats.totalCommands++;

    try {
      const route = this.determineRoute(commandName);
      this.log('debug', `🎯 Routing ${commandName} to ${route}`);

      let result: CommandResult<R>;

      if (route === 'typescript') {
        result = await this.executeTypeScript(commandName, params, context);
        this.stats.typescriptCommands++;
      } else {
        result = await this.executeLegacy(commandName, params, context);
        this.stats.legacyCommands++;
      }

      // Track successful migrations
      if (route === 'typescript' && this.legacyRegistry?.hasCommand?.(commandName)) {
        this.stats.successfulMigrations++;
      }

      return result;

    } catch (error) {
      this.stats.errors++;
      this.log('error', `❌ Command execution failed: ${commandName} - ${error.message}`);

      return {
        success: false,
        error: `Bridge execution failed: ${error.message}`,
        timestamp: new Date().toISOString(),
        metadata: {
          bridge: true,
          route: 'error',
          originalError: error.message
        }
      };
    }
  }

  /**
   * Execute command via TypeScript processor
   */
  private async executeTypeScript<T, R>(
    commandName: string,
    params: T,
    context?: CommandContext
  ): Promise<CommandResult> {
    try {
      return await this.typeScriptProcessor.executeCommand(commandName, params, context);
    } catch (error) {
      // Fallback to legacy if enabled
      if (this.config.fallbackOnError && this.config.enableLegacy) {
        this.log('warn', `⚠️ TypeScript execution failed, falling back to legacy: ${commandName}`);
        this.stats.fallbacks++;
        return await this.executeLegacy(commandName, params, context);
      }
      throw error;
    }
  }

  /**
   * Execute command via legacy registry
   */
  private async executeLegacy<T, R>(
    commandName: string,
    params: T,
    context?: CommandContext
  ): Promise<CommandResult> {
    if (!this.legacyRegistry) {
      throw new Error('Legacy registry not available');
    }

    // Convert to legacy format
    const legacyResult = await this.legacyRegistry.executeCommand(commandName, params, context);

    // Normalize result format
    return {
      success: legacyResult.success ?? true,
      data: legacyResult.data || legacyResult.result,
      error: legacyResult.error,
      timestamp: legacyResult.timestamp || new Date().toISOString(),
      metadata: {
        ...legacyResult.metadata,
        bridge: true,
        route: 'legacy'
      }
    };
  }

  /**
   * Determine routing strategy for a command
   */
  private determineRoute(commandName: string): 'typescript' | 'legacy' {
    // Check explicit migration routes first
    const explicitRoute = this.migrationRoutes.get(commandName.toLowerCase());
    if (explicitRoute) {
      return explicitRoute;
    }

    // Apply migration mode logic
    switch (this.config.migrationMode) {
      case 'typescript-only':
        return 'typescript';

      case 'typescript-first':
        // Try TypeScript first, fallback handled in execution
        return 'typescript';

      case 'parallel':
        // Route based on migration percentage and command availability
        const hasTypeScript = this.typeScriptProcessor.getAllCommands().includes(commandName.toLowerCase());
        const hasLegacy = this.legacyRegistry?.hasCommand?.(commandName) ?? false;

        if (hasTypeScript && !hasLegacy) {
          return 'typescript';
        }
        if (hasLegacy && !hasTypeScript) {
          return 'legacy';
        }
        if (hasTypeScript && hasLegacy) {
          // Use migration percentage for overlapping commands
          return Math.random() * 100 < this.config.migrationPercentage ? 'typescript' : 'legacy';
        }
        // Neither exists - prefer TypeScript for better error messages
        return 'typescript';

      case 'off':
      default:
        return 'legacy';
    }
  }

  /**
   * Analyze command overlap between TypeScript and legacy systems
   */
  private async analyzeCommandOverlap(): Promise<void> {
    if (!this.config.enableTypeScript || !this.config.enableLegacy) {
      return;
    }

    const tsCommands = new Set(this.typeScriptProcessor.getAllCommands());
    const legacyCommandsResult = this.legacyRegistry?.getAllCommands?.();
    const legacyCommands = new Set(Array.isArray(legacyCommandsResult) ? legacyCommandsResult : []);

    const overlap = new Set([...tsCommands].filter(cmd => legacyCommands.has(cmd)));
    const tsOnly = new Set([...tsCommands].filter(cmd => !legacyCommands.has(cmd)));
    const legacyOnly = new Set([...legacyCommands].filter(cmd => !tsCommands.has(cmd)));

    this.log('info', `📊 Command Analysis:`);
    this.log('info', `  TypeScript: ${tsCommands.size} commands`);
    this.log('info', `  Legacy: ${legacyCommands.size} commands`);
    this.log('info', `  Overlap: ${overlap.size} commands`);
    this.log('info', `  TypeScript-only: ${tsOnly.size} commands`);
    this.log('info', `  Legacy-only: ${legacyOnly.size} commands`);

    // Set explicit routes for TypeScript-only commands
    for (const cmd of tsOnly) {
      this.migrationRoutes.set(cmd, 'typescript');
      this.stats.migrationRoutes++;
    }

    // Set explicit routes for legacy-only commands
    for (const cmd of legacyOnly) {
      this.migrationRoutes.set(cmd, 'legacy');
    }
  }

  /**
   * Get available commands from both systems
   */
  getAllCommands(): string[] {
    const commands = new Set<string>();

    if (this.config.enableTypeScript) {
      this.typeScriptProcessor.getAllCommands().forEach(cmd => commands.add(cmd));
    }

    if (this.config.enableLegacy && this.legacyRegistry?.getAllCommands) {
      const legacyCommands = this.legacyRegistry.getAllCommands();
      if (Array.isArray(legacyCommands)) {
        legacyCommands.forEach((cmd: string) => commands.add(cmd));
      }
    }

    return Array.from(commands).sort();
  }

  /**
   * Get command definition (try TypeScript first, then legacy)
   */
  getCommandDefinition(commandName: string): any {
    if (this.config.enableTypeScript) {
      const tsDef = this.typeScriptProcessor.getDefinition(commandName);
      if (tsDef) return tsDef;
    }

    if (this.config.enableLegacy && this.legacyRegistry?.getDefinition) {
      return this.legacyRegistry.getDefinition(commandName);
    }

    return null;
  }

  /**
   * Get bridge statistics
   */
  getStats(): BridgeStats {
    return { ...this.stats };
  }

  /**
   * Update migration configuration
   */
  updateConfig(updates: Partial<BridgeConfig>): void {
    Object.assign(this.config, updates);
    this.log('info', `🔧 Bridge config updated: ${JSON.stringify(updates)}`);
  }

  /**
   * Force migration route for specific command
   */
  setMigrationRoute(commandName: string, route: 'typescript' | 'legacy'): void {
    this.migrationRoutes.set(commandName.toLowerCase(), route);
    this.stats.migrationRoutes++;
    this.log('info', `🎯 Migration route set: ${commandName} → ${route}`);
  }

  /**
   * Get bridge health status
   */
  getHealth(): any {
    return {
      bridge: {
        initialized: true,
        config: this.config,
        stats: this.stats,
        routes: Object.fromEntries(this.migrationRoutes)
      },
      typescript: this.config.enableTypeScript ? {
        available: true,
        commands: this.typeScriptProcessor.getAllCommands().length,
        categories: this.typeScriptProcessor.getCategories()
      } : { available: false },
      legacy: this.config.enableLegacy ? {
        available: !!this.legacyRegistry,
        commands: this.legacyRegistry?.getAllCommands?.()?.length || 0
      } : { available: false }
    };
  }

  /**
   * Internal logging
   */
  private log(level: BridgeConfig['logLevel'], message: string): void {
    const levels = { debug: 0, info: 1, warn: 2, error: 3 };
    if (levels[level] >= levels[this.config.logLevel]) {
      const prefix = { debug: '🔍', info: 'ℹ️', warn: '⚠️', error: '❌' }[level];
      console.log(`${prefix} [CommandBridge] ${message}`);
    }
  }
}

// Export singleton bridge instance
export const commandProcessorBridge = new CommandProcessorBridge({
  migrationMode: 'parallel',
  migrationPercentage: 25,
  logLevel: 'info'
});

export default CommandProcessorBridge;