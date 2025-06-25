/**
 * TypeScript Command Processor - Modern command execution system
 * Replaces legacy CommandRegistry.cjs with full TypeScript architecture
 * Handles case-insensitive command lookup with proper parameter handling
 */

import { promises as fs } from 'fs';
import path from 'path';
import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../commands/core/BaseCommand';

export interface CommandModule {
  default: typeof BaseCommand;
  [key: string]: any;
}

export interface ProcessorConfig {
  commandDirs: string[];
  enableCaseInsensitive: boolean;
  enableTypeScriptOnly: boolean;
  logLevel: 'debug' | 'info' | 'warn' | 'error';
}

export class TypeScriptCommandProcessor {
  private commands = new Map<string, typeof BaseCommand>();
  private definitions = new Map<string, CommandDefinition>();
  private initialized = false;
  private config: ProcessorConfig;

  constructor(config: Partial<ProcessorConfig> = {}) {
    this.config = {
      commandDirs: [
        'src/commands/core',
        'src/commands/ui', 
        'src/commands/browser',
        'src/commands/file',
        'src/commands/docs',
        'src/commands/planning',
        'src/commands/development',
        'src/commands/monitoring',
        'src/commands/communication'
      ],
      enableCaseInsensitive: true,
      enableTypeScriptOnly: false, // Set to true when we want to drop CJS entirely
      logLevel: 'info',
      ...config
    };
  }

  /**
   * Initialize the command processor by scanning for TypeScript commands
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    this.log('info', '🚀 Initializing TypeScript Command Processor');
    
    let totalLoaded = 0;
    let totalErrors = 0;

    for (const commandDir of this.config.commandDirs) {
      const { loaded, errors } = await this.loadCommandsFromDirectory(commandDir);
      totalLoaded += loaded;
      totalErrors += errors;
    }

    this.initialized = true;
    this.log('info', `✅ TypeScript Command Processor initialized: ${totalLoaded} commands loaded, ${totalErrors} errors`);
    
    if (this.config.logLevel === 'debug') {
      this.logCommandSummary();
    }
  }

  /**
   * Execute a command with proper context and error handling
   */
  async executeCommand<T = any, R = any>(
    commandName: string, 
    params: T, 
    context?: CommandContext
  ): Promise<CommandResult<R>> {
    await this.initialize();

    // Case-insensitive command lookup
    const normalizedName = this.config.enableCaseInsensitive 
      ? commandName.toLowerCase() 
      : commandName;
    
    const CommandClass = this.commands.get(normalizedName);
    
    if (!CommandClass) {
      return {
        success: false,
        message: `Command '${commandName}' not found`,
        error: `Available commands: ${Array.from(this.commands.keys()).join(', ')}`,
        timestamp: new Date().toISOString()
      };
    }

    try {
      this.log('debug', `⚡ Executing TypeScript command: ${commandName}`);
      
      // Create enriched context
      const executionContext: CommandContext = {
        ...context,
        processor: 'typescript',
        executionId: this.generateExecutionId(),
        timestamp: new Date()
      };

      const result = await CommandClass.execute(params, executionContext);
      
      this.log('debug', `✅ Command ${commandName} completed: ${result.success ? 'SUCCESS' : 'FAILED'}`);
      return result;

    } catch (error) {
      this.log('error', `❌ Command ${commandName} failed: ${error.message}`);
      return {
        success: false,
        message: `Command execution failed: ${error.message}`,
        error: error.stack,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get command definition
   */
  getDefinition(commandName: string): CommandDefinition | undefined {
    const normalizedName = this.config.enableCaseInsensitive 
      ? commandName.toLowerCase() 
      : commandName;
    
    return this.definitions.get(normalizedName);
  }

  /**
   * Get all available commands
   */
  getAllCommands(): string[] {
    return Array.from(this.commands.keys());
  }

  /**
   * Get all command definitions
   */
  getAllDefinitions(): CommandDefinition[] {
    return Array.from(this.definitions.values());
  }

  /**
   * Get commands by category
   */
  getCommandsByCategory(category: string): CommandDefinition[] {
    return this.getAllDefinitions().filter(def => def.category === category);
  }

  /**
   * Get all categories
   */
  getCategories(): string[] {
    return [...new Set(this.getAllDefinitions().map(def => def.category))];
  }

  /**
   * Load commands from a directory recursively
   */
  private async loadCommandsFromDirectory(dir: string): Promise<{ loaded: number; errors: number }> {
    let loaded = 0;
    let errors = 0;

    try {
      const fullPath = path.resolve(dir);
      const exists = await fs.access(fullPath).then(() => true).catch(() => false);
      
      if (!exists) {
        this.log('warn', `Command directory not found: ${dir}`);
        return { loaded, errors };
      }

      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      
      for (const entry of entries) {
        const entryPath = path.join(fullPath, entry.name);
        
        if (entry.isDirectory()) {
          // Recursively load subdirectories
          const subResult = await this.loadCommandsFromDirectory(entryPath);
          loaded += subResult.loaded;
          errors += subResult.errors;
        } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) {
          // Load TypeScript command files
          try {
            await this.loadTypeScriptCommand(entryPath);
            loaded++;
          } catch (error) {
            this.log('error', `Failed to load TypeScript command ${entryPath}: ${error.message}`);
            errors++;
          }
        }
      }

    } catch (error) {
      this.log('error', `Failed to scan directory ${dir}: ${error.message}`);
      errors++;
    }

    return { loaded, errors };
  }

  /**
   * Load a single TypeScript command file
   */
  private async loadTypeScriptCommand(filePath: string): Promise<void> {
    try {
      this.log('debug', `🔷 Loading TypeScript command: ${path.basename(filePath)}`);
      
      // Dynamic import with file:// URL for TypeScript files
      const moduleUrl = `file://${filePath}`;
      const module: CommandModule = await import(moduleUrl);
      
      // Extract command class
      const CommandClass = module.default || 
                          Object.values(module).find(exp => 
                            exp && 
                            typeof exp === 'function' && 
                            typeof (exp as any).getDefinition === 'function' && 
                            typeof (exp as any).execute === 'function'
                          ) as typeof BaseCommand;

      if (!CommandClass) {
        throw new Error(`No valid command class found in ${filePath}`);
      }

      // Get command definition
      const definition = CommandClass.getDefinition();
      if (!definition || !definition.name) {
        throw new Error(`Invalid command definition in ${filePath}`);
      }

      // Register command with case handling
      const commandName = this.config.enableCaseInsensitive 
        ? definition.name.toLowerCase() 
        : definition.name;

      this.commands.set(commandName, CommandClass);
      this.definitions.set(commandName, definition);

      this.log('debug', `✅ Registered TypeScript command: ${commandName} (${definition.category})`);

    } catch (error) {
      throw new Error(`Failed to load TypeScript command from ${filePath}: ${error.message}`);
    }
  }

  /**
   * Generate unique execution ID
   */
  private generateExecutionId(): string {
    return `ts_exec_${Date.now().toString(36)}_${Math.random().toString(36).substring(2, 8)}`;
  }

  /**
   * Log command summary for debugging
   */
  private logCommandSummary(): void {
    this.log('debug', '📋 Loaded TypeScript Commands:');
    
    const categories = this.getCategories();
    for (const category of categories) {
      const commands = this.getCommandsByCategory(category);
      this.log('debug', `  📁 ${category}: ${commands.map(c => c.name).join(', ')}`);
    }
  }

  /**
   * Internal logging with levels
   */
  private log(level: ProcessorConfig['logLevel'], message: string): void {
    const levels = { debug: 0, info: 1, warn: 2, error: 3 };
    const currentLevel = levels[this.config.logLevel];
    const messageLevel = levels[level];

    if (messageLevel >= currentLevel) {
      const prefix = {
        debug: '🔍',
        info: 'ℹ️',
        warn: '⚠️',
        error: '❌'
      }[level];
      
      console.log(`${prefix} [TypeScriptCommandProcessor] ${message}`);
    }
  }
}

// Export singleton instance
export const typeScriptCommandProcessor = new TypeScriptCommandProcessor({
  logLevel: 'info',
  enableCaseInsensitive: true,
  enableTypeScriptOnly: false // Will be true once migration is complete
});

export default TypeScriptCommandProcessor;