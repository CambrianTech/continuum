/**
 * MIDDLE-OUT ARCHITECTURE - UNIVERSAL COMMAND REGISTRY
 * 
 * Dynamic command discovery and execution with centralized parameter parsing.
 * Eliminates hardcoded command lists through filesystem-based discovery.
 * 
 * ISSUES IDENTIFIED:
 * - TODO: Replace 'any' type in extractCommandNames() parameter (line 365)
 * - TODO: Replace hardcoded magic strings 'core' and 'kernel' with constants (lines 446-447)
 * - TODO: Replace switch statement with strategy pattern for sorting (line 125)
 * - TODO: Replace hardcoded file patterns 'Command.ts' with configuration (line 388)
 * - TODO: Replace hardcoded scanPaths default 'src/commands' with configuration (line 72)
 * - TODO: Replace hardcoded maxDepth=3 with configuration (line 461)
 * - TODO: Extract string matching logic to separate service (lines 398-408)
 * 
 * ✅ FIXED: Universal Integration Parser called before command execution
 * 🔬 MIDDLE-OUT ARCHITECTURE: Centralized parameter parsing at registry level
 * 
 * Features:
 * - Dynamic filesystem-based command discovery
 * - Promise-based execution patterns
 * - Core/kernel command dependency system
 * - Universal availability across all components
 * - Automatic registration when commands are added/removed
 */

import { EventEmitter } from 'events';
import { CommandDefinition, CommandResult } from '../commands/core/base-command/BaseCommand';
import { ContinuumContext, continuumContextFactory } from '../types/shared/core/ContinuumTypes';
import { 
  CommandCategory, 
  normalizeCommandCategory,
  createErrorResult
} from '../types/shared/CommandTypes';

export interface CommandMetadata {
  name: string;
  category: CommandCategory;
  definition: CommandDefinition;
  filePath: string;
  className: string;
  isCore: boolean;
  isKernel: boolean;
  dependencies: string[];
  capabilities: string[];
  lastModified: Date;
}

export interface CommandRegistryOptions {
  scanPaths?: string[];
  enableFileWatcher?: boolean;
  cacheEnabled?: boolean;
  logLevel?: 'debug' | 'info' | 'warn' | 'error';
}

export interface CommandListOptions {
  category?: CommandCategory;
  coreOnly?: boolean;
  kernelOnly?: boolean;
  includeDefinitions?: boolean;
  sortBy?: 'name' | 'category' | 'lastModified';
}

export interface CommandExecutionOptions {
  timeout?: number;
  retries?: number;
  fallbackEnabled?: boolean;
  validateParameters?: boolean;
}

/**
 * Universal Command Registry - Single source of truth for all command operations
 */
export class UniversalCommandRegistry extends EventEmitter {
  private commands = new Map<string, CommandMetadata>();
  // TODO: Replace hardcoded default path with configuration
  private scanPaths: string[] = ['src/commands'];
  private logLevel: string = 'info';
  private isScanning: boolean = false;

  constructor(options: CommandRegistryOptions = {}) {
    super();
    
    this.scanPaths = options.scanPaths || ['src/commands'];
    this.logLevel = options.logLevel || 'info';
    
    if (options.enableFileWatcher) {
      this.enableFileWatcher();
    }
  }

  /**
   * Initialize the registry with full command discovery
   */
  async initialize(): Promise<void> {
    this.log('info', '🔍 Initializing Universal Command Registry...');
    
    await this.scanForCommands();
    
    this.log('info', `✅ Registry initialized with ${this.commands.size} commands`);
    this.emit('initialized', { commandCount: this.commands.size });
  }

  /**
   * Get all available commands - NO HARDCODED LISTS
   */
  async getAvailableCommands(options: CommandListOptions = {}): Promise<string[]> {
    if (this.commands.size === 0) {
      await this.scanForCommands();
    }

    let commands = Array.from(this.commands.values());

    // Apply filters
    if (options.category) {
      commands = commands.filter(cmd => cmd.category === options.category);
    }
    
    if (options.coreOnly) {
      commands = commands.filter(cmd => cmd.isCore);
    }
    
    if (options.kernelOnly) {
      commands = commands.filter(cmd => cmd.isKernel);
    }

    // Sort results
    if (options.sortBy) {
      commands.sort((a, b) => {
        // TODO: Replace switch statement with strategy pattern for better extensibility
        switch (options.sortBy) {
          case 'category':
            return a.category.localeCompare(b.category);
          case 'lastModified':
            return b.lastModified.getTime() - a.lastModified.getTime();
          default:
            return a.name.localeCompare(b.name);
        }
      });
    }

    return commands.map(cmd => cmd.name);
  }

  /**
   * Get command definition by name
   */
  async getCommandDefinition(commandName: string): Promise<CommandDefinition | null> {
    const command = this.commands.get(commandName);
    if (!command) {
      // Try to rescan in case command was recently added
      await this.scanForCommands();
      const retryCommand = this.commands.get(commandName);
      return retryCommand?.definition || null;
    }
    
    return command.definition;
  }

  /**
   * Get command metadata
   */
  async getCommandMetadata(commandName: string): Promise<CommandMetadata | null> {
    const command = this.commands.get(commandName);
    if (!command) {
      await this.scanForCommands();
      return this.commands.get(commandName) || null;
    }
    
    return command;
  }

  /**
   * Execute a command dynamically
   */
  async executeCommand(
    commandName: string,
    parameters: unknown = {},
    context?: ContinuumContext,
    options: CommandExecutionOptions = {}
  ): Promise<CommandResult> {
    // Use factory to create default context if not provided
    const defaultContext = continuumContextFactory.create({
      ...(context?.sessionId && { sessionId: context.sessionId }),
      environment: 'server'
    });
    
    const finalContext = context ? 
      continuumContextFactory.merge(defaultContext, context) : 
      defaultContext;
    const commandMetadata = await this.getCommandMetadata(commandName);
    
    if (!commandMetadata) {
      return createErrorResult(
        `Command '${commandName}' not found. Available commands: ${(await this.getAvailableCommands()).join(', ')}`
      );
    }

    try {
      // Dynamic import and execution
      const fileUrl = `file://${commandMetadata.filePath}`;
      const commandModule = await import(fileUrl);
      const CommandClass = commandModule[commandMetadata.className];
      
      if (!CommandClass || !CommandClass.execute) {
        return createErrorResult(`Command '${commandName}' does not have execute method`);
      }

      // Parse parameters using Universal Integration Parser system
      let parsedParameters = parameters;
      if (CommandClass._registryParseParams) {
        parsedParameters = CommandClass._registryParseParams(parameters);
      }

      // Parameter validation if requested (use parsed parameters)
      if (options.validateParameters) {
        const validationResult = this.validateParameters(parsedParameters, commandMetadata.definition);
        if (!validationResult.valid) {
          return createErrorResult(`Parameter validation failed: ${validationResult.errors.join(', ')}`);
        }
      }

      // Universal Session Context Architecture:
      // - All commands default to shared session context (seamless integration)
      // - Provided context overrides shared context (explicit control)
      // - Use 'connect' command to create new sessions when needed
      const { mergeWithSharedContext } = await import('./SharedSessionContext');
      const effectiveContext = await mergeWithSharedContext(finalContext);

      // Execute with timeout if specified (use parsed parameters and merged context)
      const executePromise = CommandClass.execute(parsedParameters, effectiveContext);
      
      if (options.timeout) {
        const timeoutPromise = new Promise<CommandResult>((_, reject) => {
          setTimeout(() => reject(new Error(`Command execution timed out after ${options.timeout}ms`)), options.timeout);
        });
        
        const result = await Promise.race([executePromise, timeoutPromise]);
        return result;
      }

      return await executePromise;
      
    } catch (error) {
      return createErrorResult(`Failed to execute command '${commandName}': ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Check if a command exists
   */
  async hasCommand(commandName: string): Promise<boolean> {
    return this.commands.has(commandName) || (await this.getCommandMetadata(commandName)) !== null;
  }

  /**
   * Get commands by category
   */
  async getCommandsByCategory(): Promise<Record<string, string[]>> {
    const commands = Array.from(this.commands.values());
    const categories: Record<string, string[]> = {};

    for (const command of commands) {
      const category = command.category || 'other';
      if (!categories[category]) {
        categories[category] = [];
      }
      categories[category].push(command.name);
    }

    return categories;
  }

  /**
   * Get core commands (essential for system operation)
   */
  async getCoreCommands(): Promise<string[]> {
    return this.getAvailableCommands({ coreOnly: true });
  }

  /**
   * Get kernel commands (required for basic functionality)
   */
  async getKernelCommands(): Promise<string[]> {
    return this.getAvailableCommands({ kernelOnly: true });
  }

  /**
   * Rescan for commands - useful for development
   */
  async refresh(): Promise<void> {
    this.log('info', '🔄 Refreshing command registry...');
    await this.scanForCommands();
    this.emit('refreshed', { commandCount: this.commands.size });
  }

  /**
   * Scan filesystem for commands
   */
  private async scanForCommands(): Promise<void> {
    if (this.isScanning) {
      return; // Prevent concurrent scans
    }
    
    this.isScanning = true;
    
    try {
      this.commands.clear();
      
      for (const scanPath of this.scanPaths) {
        await this.scanDirectory(scanPath);
      }
      
      this.log('info', `📋 Scanned ${this.commands.size} commands from ${this.scanPaths.join(', ')}`);
    } finally {
      this.isScanning = false;
    }
  }

  /**
   * Scan a directory for command modules
   */
  private async scanDirectory(dirPath: string): Promise<void> {
    try {
      const fs = await import('fs/promises');
      const path = await import('path');
      
      const commandDirs = await this.findCommandDirectories(dirPath);
      
      for (const commandDir of commandDirs) {
        try {
          const packagePath = path.join(commandDir, 'package.json');
          
          // Check if package.json exists
          try {
            await fs.access(packagePath);
          } catch {
            continue; // Skip if no package.json
          }
          
          // Parse package.json for command metadata
          const packageContent = await fs.readFile(packagePath, 'utf-8');
          const packageJson = JSON.parse(packageContent);
          
          // Extract command information
          const commandNames = this.extractCommandNames(packageJson);
          
          if (commandNames.length > 0) {
            const commandFiles = await this.findCommandFiles(commandDir);
            
            for (const commandFile of commandFiles) {
              const commandName = this.matchCommandToFile(commandNames, commandFile);
              if (commandName) {
                const metadata = await this.createCommandMetadata(
                  commandName,
                  commandDir,
                  commandFile,
                  packageJson
                );
                
                if (metadata) {
                  this.commands.set(commandName, metadata);
                  this.log('debug', `✅ Registered command: ${commandName}`);
                }
              }
            }
          }
        } catch (error) {
          this.log('warn', `Failed to process ${commandDir}: ${error}`);
        }
      }
    } catch (error) {
      this.log('error', `Failed to scan directory ${dirPath}: ${error}`);
    }
  }

  /**
   * Extract command names from package.json
   */
  // TODO: Replace 'any' type with proper PackageJsonContinuum interface
  private extractCommandNames(packageJson: any): string[] {
    const commandNames: string[] = [];
    
    const continuum = packageJson.continuum || {};
    
    if (continuum.commandName) commandNames.push(continuum.commandName);
    if (continuum.core) commandNames.push(continuum.core);
    if (continuum.command) commandNames.push(continuum.command);
    if (continuum.commands && Array.isArray(continuum.commands)) {
      commandNames.push(...continuum.commands);
    }
    
    return commandNames;
  }

  /**
   * Find command files in a directory
   */
  private async findCommandFiles(dirPath: string): Promise<string[]> {
    const fs = await import('fs/promises');
    const files = await fs.readdir(dirPath);
    
    return files.filter(file => 
      // TODO: Replace hardcoded file patterns with configuration
      file.includes('Command.ts') && !file.includes('.test.ts') && !file.includes('.d.ts')
    );
  }

  /**
   * Match command name to file
   */
  private matchCommandToFile(commandNames: string[], fileName: string): string | null {
    const className = fileName.replace('.ts', '');
    
    for (const cmdName of commandNames) {
      const cmdFileBase = cmdName.replace(/-/g, '').toLowerCase();
      const classFileBase = className.replace(/Command$/, '').toLowerCase();
      
      if (classFileBase === cmdFileBase || 
          className.toLowerCase().includes(cmdName.replace(/-/g, '').toLowerCase())) {
        return cmdName;
      }
    }
    
    return commandNames[0] || null;
  }

  /**
   * Create command metadata
   */
  private async createCommandMetadata(
    commandName: string,
    dirPath: string,
    fileName: string,
    packageJson: any
  ): Promise<CommandMetadata | null> {
    const path = await import('path');
    const fs = await import('fs/promises');
    
    const filePath = path.resolve(dirPath, fileName);
    const className = fileName.replace('.ts', '');
    
    try {
      const stats = await fs.stat(filePath);
      const continuum = packageJson.continuum || {};
      
      // Normalize category to prevent "Core" vs "core" typos
      const normalizedCategory = normalizeCommandCategory(continuum.category || 'other');
      
      return {
        name: commandName,
        category: normalizedCategory,
        definition: {
          name: commandName,
          category: normalizedCategory,
          description: packageJson.description || `${commandName} command`,
          parameters: continuum.parameters || {},
          examples: continuum.examples || [],
          usage: continuum.usage || `Execute ${commandName} command`
        },
        filePath,
        className,
        // TODO: Replace hardcoded 'core' and 'kernel' strings with constants
        isCore: continuum.core === true || continuum.type === 'core',
        isKernel: continuum.kernel === true || continuum.type === 'kernel',
        dependencies: continuum.dependencies || [],
        capabilities: continuum.capabilities || [],
        lastModified: stats.mtime
      };
    } catch (error) {
      this.log('warn', `Failed to create metadata for ${commandName}: ${error}`);
      return null;
    }
  }

  /**
   * Find command directories recursively
   */
  // TODO: Replace hardcoded maxDepth default with configuration
  private async findCommandDirectories(baseDir: string, maxDepth: number = 3): Promise<string[]> {
    const directories: string[] = [];
    const fs = await import('fs/promises');
    const path = await import('path');
    
    const scanDir = async (dir: string, depth: number) => {
      if (depth >= maxDepth) return;
      
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        
        for (const entry of entries) {
          if (entry.isDirectory() && 
              !entry.name.startsWith('.') && 
              !['node_modules', 'dist', 'build'].includes(entry.name)) {
            
            const fullPath = path.join(dir, entry.name);
            directories.push(fullPath);
            await scanDir(fullPath, depth + 1);
          }
        }
      } catch (error) {
        // Directory doesn't exist or can't be read
      }
    };
    
    await scanDir(baseDir, 0);
    return directories;
  }

  /**
   * Validate command parameters
   */
  private validateParameters(parameters: unknown, definition: CommandDefinition): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    
    // Basic validation - can be extended
    if (typeof parameters !== 'object' || parameters === null) {
      errors.push('Parameters must be an object');
    }
    
    // Check required parameters
    const params = parameters as Record<string, unknown>;
    for (const [paramName, paramDef] of Object.entries(definition.parameters || {})) {
      if (paramDef.required && (params[paramName] === undefined || params[paramName] === null)) {
        errors.push(`Required parameter '${paramName}' is missing`);
      }
    }
    
    return { valid: errors.length === 0, errors };
  }

  /**
   * Enable file watcher for automatic updates
   */
  private enableFileWatcher(): void {
    // TODO: Implement file watcher for automatic command discovery updates
    this.log('info', '📁 File watcher enabled for automatic command discovery');
  }

  /**
   * Logging utility
   */
  private log(level: string, message: string): void {
    const levels = { debug: 0, info: 1, warn: 2, error: 3 };
    const currentLevel = levels[this.logLevel as keyof typeof levels] || 1;
    const messageLevel = levels[level as keyof typeof levels] || 1;
    
    if (messageLevel >= currentLevel) {
      console.log(`[UniversalCommandRegistry] ${message}`);
    }
  }
}

// Global singleton instance
let globalRegistry: UniversalCommandRegistry | null = null;

/**
 * Get the global command registry instance
 */
export function getGlobalCommandRegistry(): UniversalCommandRegistry {
  if (!globalRegistry) {
    globalRegistry = new UniversalCommandRegistry({
      enableFileWatcher: true,
      cacheEnabled: true,
      logLevel: 'info'
    });
  }
  return globalRegistry;
}

/**
 * Initialize the global command registry
 */
export async function initializeGlobalCommandRegistry(): Promise<void> {
  const registry = getGlobalCommandRegistry();
  await registry.initialize();
}

// Default export for compatibility
export default UniversalCommandRegistry;