/**
 * Command Discovery System using Core Module Patterns
 * 
 * Bridges the CommandProcessorDaemon and command execution system with the
 * universal core module discovery system. Provides type-safe command discovery,
 * dependency management, and consistent patterns with other module types.
 */

import { ModuleDiscovery, type ModuleInfo, type ModuleDependency } from '../../../core/modules/index.js';
import type { CommandDefinition } from '../base-command/BaseCommand.js';

export interface CommandMetadata {
  name: string;
  category: string;
  type: 'core' | 'extension' | 'integration';
  filePath: string;
  module: ModuleInfo;
  definition?: CommandDefinition | undefined;
}

export interface CommandDiscoveryOptions {
  includeOptional?: boolean;
  categoryFilter?: string[];
  typeFilter?: ('core' | 'extension' | 'integration')[];
}

export class CommandDiscovery {
  private moduleDiscovery: ModuleDiscovery;
  private commandCache: Map<string, CommandMetadata> = new Map();

  constructor(rootDir?: string) {
    this.moduleDiscovery = ModuleDiscovery.getInstance(rootDir);
  }

  /**
   * Get all available command names
   */
  async getAvailableCommands(options: CommandDiscoveryOptions = {}): Promise<string[]> {
    const commandModules = await this.moduleDiscovery.discoverModules('command');
    const allCommands: CommandMetadata[] = [];
    
    for (const module of commandModules) {
      if (!module.hasPackageJson) continue;
      
      // Extract direct commands and sub-commands
      const directCommand = this.extractCommandMetadata(module);
      if (directCommand) allCommands.push(directCommand);
      
      const subCommands = await this.extractSubCommands(module);
      allCommands.push(...subCommands);
    }

    // Apply filters and cache
    const filteredCommands = this.applyFilters(allCommands, options);
    filteredCommands.forEach(cmd => this.commandCache.set(cmd.name, cmd));

    return filteredCommands.map(cmd => cmd.name).sort();
  }

  /**
   * Apply filters to command list
   */
  private applyFilters(commands: CommandMetadata[], options: CommandDiscoveryOptions): CommandMetadata[] {
    let filtered = commands;
    
    if (options.categoryFilter?.length) {
      filtered = filtered.filter(cmd => options.categoryFilter!.includes(cmd.category));
    }

    if (options.typeFilter?.length) {
      filtered = filtered.filter(cmd => options.typeFilter!.includes(cmd.type));
    }

    return filtered;
  }

  /**
   * Get command metadata by name
   */
  async getCommandMetadata(commandName: string): Promise<CommandMetadata | null> {
    // Check cache first
    if (this.commandCache.has(commandName)) {
      return this.commandCache.get(commandName)!;
    }

    // Search through all command modules
    const commandModules = await this.moduleDiscovery.discoverModules('command');
    
    for (const module of commandModules) {
      const metadata = this.extractCommandMetadata(module);
      if (metadata && metadata.name === commandName) {
        this.commandCache.set(commandName, metadata);
        return metadata;
      }
    }

    return null;
  }

  /**
   * Get command definition for execution
   */
  async getCommandDefinition(commandName: string): Promise<CommandDefinition | null> {
    const metadata = await this.getCommandMetadata(commandName);
    if (!metadata) return null;

    // Load the definition if not cached
    if (!metadata.definition) {
      metadata.definition = await this.loadCommandDefinition(metadata) || undefined;
    }

    return metadata.definition || null;
  }

  /**
   * Check if a command exists and is available
   */
  async hasCommand(commandName: string): Promise<boolean> {
    const metadata = await this.getCommandMetadata(commandName);
    return metadata !== null;
  }

  /**
   * Get commands by category
   */
  async getCommandsByCategory(category: string): Promise<string[]> {
    return this.getAvailableCommands({ categoryFilter: [category] });
  }

  /**
   * Get commands by type
   */
  async getCommandsByType(type: 'core' | 'extension' | 'integration'): Promise<string[]> {
    return this.getAvailableCommands({ typeFilter: [type] });
  }

  /**
   * Create dependency iterator for command dependencies
   */
  createCommandDependencyIterator(dependencies: Record<string, ModuleDependency>) {
    return this.moduleDiscovery.createDependencyIterator(dependencies);
  }

  /**
   * Clear discovery cache
   */
  clearCache(): void {
    this.commandCache.clear();
    this.moduleDiscovery.clearCache();
  }

  /**
   * Extract sub-commands using npm/package.json intelligence
   */
  private async extractSubCommands(categoryModule: ModuleInfo): Promise<CommandMetadata[]> {
    // Let the core module discovery system handle the heavy lifting
    // We just need to filter the results for commands within this category
    const allCommandModules = await this.moduleDiscovery.discoverModules('command');
    
    return allCommandModules
      .filter(module => 
        module.path.startsWith(categoryModule.path + '/') && 
        module.path !== categoryModule.path
      )
      .map(module => this.extractCommandMetadata(module, categoryModule.packageData?.continuum?.category))
      .filter(Boolean) as CommandMetadata[];
  }

  /**
   * Extract command metadata from module info
   */
  private extractCommandMetadata(module: ModuleInfo, parentCategory?: string): CommandMetadata | null {
    if (!module.packageData?.continuum) return null;

    const continuum = module.packageData.continuum;
    
    // Extract command name from various fields
    let commandName: string | null = null;
    if (continuum.commandName) {
      commandName = continuum.commandName;
    } else if (continuum.command) {
      commandName = continuum.command;
    } else if (continuum.core) {
      commandName = continuum.core;
    } else if (Array.isArray(continuum.commands) && continuum.commands.length > 0) {
      commandName = continuum.commands[0]; // Use first command name
    } else if (parentCategory) {
      // For sub-commands, use the directory name as command name (this is the leaf command)
      commandName = module.name;
    } else {
      // For category-level modules without explicit command name, skip them
      // We only want the leaf commands, not the category containers
      return null;
    }

    if (!commandName) return null;

    // Determine category and type
    const category = parentCategory || continuum.category || this.getCategoryFromPath(module.path);
    const type = continuum.type || this.getTypeFromCategory(category);

    // Build file path
    const filePath = this.getCommandFilePath(module, commandName);

    return {
      name: commandName,
      category,
      type: type as 'core' | 'extension' | 'integration',
      filePath,
      module
    };
  }

  /**
   * Load command definition from file
   */
  private async loadCommandDefinition(metadata: CommandMetadata): Promise<CommandDefinition | null> {
    try {
      // Dynamic import using the file path
      const module = await import(metadata.filePath);
      
      // Look for common export patterns
      if (module.default && typeof module.default.getDefinition === 'function') {
        return module.default.getDefinition();
      }
      
      if (module.getDefinition && typeof module.getDefinition === 'function') {
        return module.getDefinition();
      }

      // Look for static getDefinition method
      const commandClass = this.findCommandClass(module);
      if (commandClass && typeof commandClass.getDefinition === 'function') {
        return commandClass.getDefinition();
      }

      return null;
    } catch (error) {
      console.warn(`Failed to load command definition for ${metadata.name}:`, error);
      return null;
    }
  }

  /**
   * Find command class in module exports
   */
  private findCommandClass(moduleExports: any): any {
    // Look for exports ending with 'Command'
    for (const [exportName, exportValue] of Object.entries(moduleExports)) {
      if (exportName.endsWith('Command') && typeof exportValue === 'function') {
        return exportValue;
      }
    }
    return null;
  }

  /**
   * Get category from module path
   */
  private getCategoryFromPath(path: string): string {
    const pathParts = path.split('/');
    const commandsIndex = pathParts.lastIndexOf('commands');
    if (commandsIndex >= 0 && commandsIndex < pathParts.length - 1) {
      return pathParts[commandsIndex + 1];
    }
    return 'misc';
  }

  /**
   * Get type from category
   */
  private getTypeFromCategory(category: string): string {
    const coreCategories = ['core', 'system', 'kernel', 'base'];
    const integrationCategories = ['academy', 'persona', 'websocket'];
    
    if (coreCategories.includes(category.toLowerCase())) {
      return 'core';
    }
    if (integrationCategories.includes(category.toLowerCase())) {
      return 'integration';
    }
    return 'extension';
  }

  /**
   * Get command file path from module and command name
   */
  private getCommandFilePath(module: ModuleInfo, commandName: string): string {
    // Try common naming patterns
    const basePath = module.path;
    const capitalizedName = commandName.charAt(0).toUpperCase() + commandName.slice(1);
    
    const possiblePaths = [
      `${basePath}/${capitalizedName}Command.ts`,
      `${basePath}/${capitalizedName}.ts`,
      `${basePath}/${commandName}Command.ts`,
      `${basePath}/${commandName}.ts`,
      `${basePath}/index.ts`
    ];

    // Return the first path that might exist
    // In a real implementation, we'd check fs.existsSync, but for TypeScript imports
    // we'll just use the most likely pattern
    return possiblePaths[0];
  }
}

// Export singleton instance for consistency with other discovery systems
export const commandDiscovery = new CommandDiscovery();