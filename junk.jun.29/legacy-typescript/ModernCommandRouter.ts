/**
 * Modern Command Router - Bridge between legacy system and TypeScript processor
 * Enables gradual migration while providing path to full TypeScript replacement
 */

import { TypeScriptCommandProcessor } from './TypeScriptCommandProcessor';
import { CommandContext, CommandResult } from '../commands/core/BaseCommand';

export interface RouterConfig {
  preferTypeScript: boolean;
  fallbackToLegacy: boolean;
  logRouting: boolean;
}

export class ModernCommandRouter {
  private tsProcessor: TypeScriptCommandProcessor;
  private legacyRegistry: any = null; // Will hold CommandRegistry.cjs if needed
  private config: RouterConfig;

  constructor(config: Partial<RouterConfig> = {}) {
    this.config = {
      preferTypeScript: true,
      fallbackToLegacy: true,
      logRouting: true,
      ...config
    };

    this.tsProcessor = new TypeScriptCommandProcessor({
      logLevel: 'info',
      enableCaseInsensitive: true
    });
  }

  /**
   * Set legacy registry for fallback (during migration phase)
   */
  setLegacyRegistry(registry: any): void {
    this.legacyRegistry = registry;
    if (this.config.logRouting) {
      console.log('🌉 ModernCommandRouter: Legacy registry connected for fallback');
    }
  }

  /**
   * Route command execution to appropriate processor
   */
  async executeCommand<T = any, R = any>(
    commandName: string,
    params: T,
    context?: CommandContext
  ): Promise<CommandResult<R>> {
    
    // Ensure TypeScript processor is initialized
    await this.tsProcessor.initialize();

    // Try TypeScript processor first if preferred
    if (this.config.preferTypeScript) {
      const tsDefinition = this.tsProcessor.getDefinition(commandName);
      
      if (tsDefinition) {
        if (this.config.logRouting) {
          console.log(`🚀 Routing ${commandName} to TypeScript processor`);
        }
        return await this.tsProcessor.executeCommand(commandName, params, context);
      }
    }

    // Fallback to legacy if enabled and available
    if (this.config.fallbackToLegacy && this.legacyRegistry) {
      const legacyCommand = this.legacyRegistry.getCommand(commandName);
      
      if (legacyCommand) {
        if (this.config.logRouting) {
          console.log(`🔄 Falling back ${commandName} to legacy processor`);
        }
        
        // Execute legacy command with compatible signature
        try {
          const result = await legacyCommand(params, context?.continuum);
          return this.normalizeLegacyResult(result);
        } catch (error) {
          return {
            success: false,
            message: `Legacy command failed: ${error.message}`,
            error: error.stack,
            timestamp: new Date().toISOString()
          };
        }
      }
    }

    // Try TypeScript as last resort if we haven't already
    if (!this.config.preferTypeScript) {
      const tsDefinition = this.tsProcessor.getDefinition(commandName);
      
      if (tsDefinition) {
        if (this.config.logRouting) {
          console.log(`🚀 Last resort: Routing ${commandName} to TypeScript processor`);
        }
        return await this.tsProcessor.executeCommand(commandName, params, context);
      }
    }

    // Command not found in either processor
    const availableCommands = [
      ...this.tsProcessor.getAllCommands(),
      ...(this.legacyRegistry ? this.legacyRegistry.getAllDefinitions().map((d: any) => d.name) : [])
    ];

    return {
      success: false,
      message: `Command '${commandName}' not found`,
      error: `Available commands: ${availableCommands.join(', ')}`,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Get command definition from any processor
   */
  getDefinition(commandName: string): any {
    // Try TypeScript first
    const tsDefinition = this.tsProcessor.getDefinition(commandName);
    if (tsDefinition) return tsDefinition;

    // Try legacy
    if (this.legacyRegistry) {
      return this.legacyRegistry.getDefinition(commandName);
    }

    return null;
  }

  /**
   * Get all available commands from both processors
   */
  getAllCommands(): string[] {
    const tsCommands = this.tsProcessor.getAllCommands();
    const legacyCommands = this.legacyRegistry 
      ? this.legacyRegistry.getAllDefinitions().map((d: any) => d.name)
      : [];
    
    return [...new Set([...tsCommands, ...legacyCommands])];
  }

  /**
   * Get routing statistics
   */
  getStats(): {
    typeScriptCommands: number;
    legacyCommands: number;
    totalCommands: number;
    migrationProgress: number;
  } {
    const tsCommands = this.tsProcessor.getAllCommands().length;
    const legacyCommands = this.legacyRegistry 
      ? this.legacyRegistry.getAllDefinitions().length
      : 0;
    const totalCommands = this.getAllCommands().length;
    
    return {
      typeScriptCommands: tsCommands,
      legacyCommands: legacyCommands,
      totalCommands,
      migrationProgress: totalCommands > 0 ? (tsCommands / totalCommands) * 100 : 0
    };
  }

  /**
   * Enable TypeScript-only mode (disables legacy fallback)
   */
  enableTypeScriptOnlyMode(): void {
    this.config.preferTypeScript = true;
    this.config.fallbackToLegacy = false;
    this.legacyRegistry = null;
    
    console.log('🚀 ModernCommandRouter: TypeScript-only mode enabled - legacy system disabled');
  }

  /**
   * Normalize legacy command results to modern format
   */
  private normalizeLegacyResult(result: any): CommandResult {
    // Legacy results might have different formats
    if (typeof result === 'object' && result !== null) {
      return {
        success: result.success ?? true,
        message: result.message ?? 'Legacy command completed',
        data: result.data ?? result,
        error: result.error,
        timestamp: result.timestamp ?? new Date().toISOString()
      };
    }

    // Handle primitive results
    return {
      success: true,
      message: 'Legacy command completed',
      data: result,
      timestamp: new Date().toISOString()
    };
  }
}

// Export singleton instance
export const modernCommandRouter = new ModernCommandRouter({
  preferTypeScript: true,
  fallbackToLegacy: true,
  logRouting: true
});

export default ModernCommandRouter;