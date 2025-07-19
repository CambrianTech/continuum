// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Converting to typed parameter execution pattern
/**
 * Help Command - Placeholder
 * 
 * LEGACY CODE MOVED TO: junk.jun.29/legacy-typescript/help/
 * 
 * TODO: Implement modern TypeScript help command that:
 * - Extends BaseCommand properly
 * - Uses CommandDefinition interface correctly
 * - Provides command discovery and documentation
 * - Integrates with module README system
 * 
 * Original functionality: Command help system with README integration
 */

import { BaseCommand, CommandDefinition, ContinuumContext, CommandResult } from '../base-command/BaseCommand';

// ✅ STRONGLY TYPED PARAMETERS - Eliminates 'any' types
interface HelpParameters {
  command?: string;
}

/**
 * Type guard for HelpParameters
 */
function validateHelpParameters(params: unknown): params is HelpParameters {
  if (typeof params !== 'object' || params === null) {
    return false;
  }
  
  const obj = params as Record<string, unknown>;
  
  // command is optional but must be string if present
  if (obj.command !== undefined && typeof obj.command !== 'string') {
    return false;
  }
  
  return true;
}

export class HelpCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'help',
      category: 'core',
      icon: '❓',
      description: 'Show help for commands (placeholder - needs modern implementation)',
      parameters: { command: { type: 'string' as const, description: 'Command name to get help for', required: false } },
      examples: [
        {
          description: 'Show general help',
          command: 'help'
        },
        {
          description: 'Show help for screenshot command',
          command: 'help screenshot'
        }
      ],
      usage: 'Get help for available commands'
    };
  }

  /**
   * ✅ STRONGLY TYPED EXECUTION - Framework → Your Strong Type
   * Eliminates 'any' types through inline validation pattern
   */
  static async execute(parameters: unknown, context: ContinuumContext): Promise<CommandResult> {
    try {
      // Parse CLI arguments: --command=screenshot → { command: "screenshot" }
      if (typeof parameters !== 'object' || parameters === null) {
        throw new Error('Parameters must be a non-null object');
      }
      const parsedParams = this.preprocessParameters(parameters); // ✅ Automatic CLI parsing
      
      // Validate with descriptive error messages
      if (!validateHelpParameters(parsedParams)) {
        throw new Error('Invalid help parameters. Expected: { command?: string }');
      }
      
      // Type assertion safe after validation
      const typedParams = parsedParams as HelpParameters;
      
      return await HelpCommand.executeTyped(typedParams, context);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: errorMessage, timestamp: new Date().toISOString() };
    }
  }
  
  /**
   * Strongly typed execution with preserved business logic
   */
  private static async executeTyped(params: HelpParameters, context: ContinuumContext): Promise<CommandResult> {
    const { command } = params;
    
    // 🔍 DEBUG: Log parameter parsing for help command
    console.log(`🔍 [HelpCommand] Typed params:`, JSON.stringify(params, null, 2));
    console.log(`🔍 [HelpCommand] Extracted command:`, command);
    
    try {
      // Use dynamic command discovery to get all available commands
      const availableCommands = await this.discoverAvailableCommands(context);
      
      if (command) {
        // Show help for specific command
        const commandDefinition = await this.getCommandDefinition(command, context);
        
        if (!commandDefinition) {
          return this.createErrorResult(
            `Command '${command}' not found`,
            `Available commands: ${availableCommands.join(', ')}`
          );
        }
        
        return this.createSuccessResult(
          `Help for command: ${command}`,
          {
            command: commandDefinition.name,
            category: commandDefinition.category,
            description: commandDefinition.description,
            parameters: commandDefinition.parameters,
            examples: commandDefinition.examples,
            usage: commandDefinition.usage
          }
        );
      } else {
        // Show list of all available commands
        const commandsByCategory = await this.categorizeCommands(availableCommands, context);
        
        return this.createSuccessResult(
          `Available commands (${availableCommands.length} total)`,
          {
            commands: availableCommands,
            commandsByCategory,
            usage: 'Use "help <command>" to get detailed help for a specific command'
          }
        );
      }
    } catch (error) {
      return this.createErrorResult(
        'Failed to get help information',
        error instanceof Error ? error.message : String(error)
      );
    }
  }
  
  
  /**
   * Discover available commands dynamically - NO HARDCODED LISTS
   */
  private static async discoverAvailableCommands(_context: ContinuumContext): Promise<string[]> {
    try {
      const { getGlobalCommandRegistry } = await import('../../../services/UniversalCommandRegistry');
      const registry = getGlobalCommandRegistry();
      return await registry.getAvailableCommands();
    } catch (error) {
      console.warn('Failed to discover commands:', error);
      return [];
    }
  }
  
  /**
   * Get definition for a specific command
   */
  private static async getCommandDefinition(command: string, _context: ContinuumContext): Promise<CommandDefinition | null> {
    try {
      const { getGlobalCommandRegistry } = await import('../../../services/UniversalCommandRegistry');
      const registry = getGlobalCommandRegistry();
      return await registry.getCommandDefinition(command);
    } catch (error) {
      console.warn(`Failed to get definition for command '${command}':`, error);
      return null;
    }
  }
  
  /**
   * Categorize commands by their category property
   */
  private static async categorizeCommands(commands: string[], context: ContinuumContext): Promise<Record<string, string[]>> {
    const categories: Record<string, string[]> = {};
    
    for (const command of commands) {
      const definition = await this.getCommandDefinition(command, context);
      const category = definition?.category || 'other';
      
      if (!categories[category]) {
        categories[category] = [];
      }
      categories[category].push(command);
    }
    
    return categories;
  }
}