/**
 * TypeScript Command Registry
 * Clean, typed command management system for systematic migration
 */

import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from './core/BaseCommand';

export interface RegisteredCommand {
  name: string;
  execute: (params: any, context?: CommandContext) => Promise<CommandResult>;
  definition: CommandDefinition;
  category: string;
  isTypeScript: boolean;
}

/**
 * Registry for TypeScript commands with migration support
 */
export class TypeScriptCommandRegistry {
  private commands = new Map<string, RegisteredCommand>();
  private migrationLog = new Map<string, { from: string; to: string; migratedAt: Date }>();

  /**
   * Register a TypeScript command
   */
  registerCommand(CommandClass: typeof BaseCommand): void {
    try {
      const definition = CommandClass.getDefinition();
      const commandName = definition.name.toUpperCase();

      const registeredCommand: RegisteredCommand = {
        name: commandName,
        execute: CommandClass.execute.bind(CommandClass),
        definition,
        category: definition.category,
        isTypeScript: true
      };

      this.commands.set(commandName, registeredCommand);
      console.log(`📚 TypeScript Command Registered: ${commandName} (${definition.category})`);

    } catch (error) {
      console.error(`❌ Failed to register TypeScript command:`, error);
    }
  }

  /**
   * Execute command with full type safety
   */
  async executeCommand(
    commandName: string, 
    params: any, 
    context?: CommandContext
  ): Promise<CommandResult> {
    const command = this.commands.get(commandName.toUpperCase());
    
    if (!command) {
      return {
        success: false,
        message: `TypeScript command '${commandName}' not found`,
        error: `Available TypeScript commands: ${Array.from(this.commands.keys()).join(', ')}`,
        timestamp: new Date().toISOString()
      };
    }

    try {
      console.log(`🎯 Executing TypeScript command: ${commandName}`);
      const result = await command.execute(params, context);
      
      // Add execution metadata
      return {
        ...result,
        metadata: {
          commandName,
          isTypeScript: true,
          executedAt: new Date().toISOString()
        }
      };

    } catch (error) {
      console.error(`❌ TypeScript command execution failed: ${commandName}`, error);
      return {
        success: false,
        message: `TypeScript command '${commandName}' execution failed`,
        error: error.message,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get command definition
   */
  getCommandDefinition(commandName: string): CommandDefinition | null {
    const command = this.commands.get(commandName.toUpperCase());
    return command?.definition || null;
  }

  /**
   * List all registered TypeScript commands
   */
  getAllCommands(): RegisteredCommand[] {
    return Array.from(this.commands.values());
  }

  /**
   * Get commands by category
   */
  getCommandsByCategory(category: string): RegisteredCommand[] {
    return Array.from(this.commands.values())
      .filter(cmd => cmd.category.toLowerCase() === category.toLowerCase());
  }

  /**
   * Check if command exists
   */
  hasCommand(commandName: string): boolean {
    return this.commands.has(commandName.toUpperCase());
  }

  /**
   * Log command migration
   */
  logMigration(commandName: string, fromPath: string, toPath: string): void {
    this.migrationLog.set(commandName.toUpperCase(), {
      from: fromPath,
      to: toPath,
      migratedAt: new Date()
    });
    
    console.log(`🔄 Command migrated: ${commandName} (.cjs → .ts)`);
  }

  /**
   * Get migration status
   */
  getMigrationStatus(): Array<{ command: string; status: string; details: any }> {
    const commands = this.getAllCommands();
    const migrations = Array.from(this.migrationLog.entries());
    
    return commands.map(cmd => ({
      command: cmd.name,
      status: 'migrated',
      details: {
        category: cmd.category,
        isTypeScript: cmd.isTypeScript,
        migratedAt: this.migrationLog.get(cmd.name)?.migratedAt
      }
    }));
  }

  /**
   * Generate command documentation
   */
  generateDocumentation(): string {
    const commands = this.getAllCommands();
    const categories = [...new Set(commands.map(cmd => cmd.category))];
    
    let docs = '# TypeScript Commands Documentation\n\n';
    
    for (const category of categories) {
      docs += `## ${category}\n\n`;
      const categoryCommands = this.getCommandsByCategory(category);
      
      for (const cmd of categoryCommands) {
        docs += `### ${cmd.definition.name}\n`;
        docs += `${cmd.definition.description}\n\n`;
        docs += `**Usage:** ${cmd.definition.usage || 'No usage information'}\n\n`;
        docs += `**Examples:**\n`;
        if (cmd.definition.examples && Array.isArray(cmd.definition.examples)) {
          cmd.definition.examples.forEach(example => {
            docs += `- ${example}\n`;
          });
        } else {
          docs += '- No examples available\n';
        }
        docs += '\n';
      }
    }
    
    return docs;
  }

  /**
   * Create migration bridge for legacy system
   */
  createLegacyBridge(): { [key: string]: Function } {
    const bridge: { [key: string]: Function } = {};
    
    for (const [name, command] of this.commands) {
      bridge[name] = async (params: any, continuum?: any, encoding?: string) => {
        const context: CommandContext = {
          continuum,
          encoding,
          webSocketServer: continuum?.webSocketServer,
          continuonStatus: continuum?.continuonStatus
        };
        
        return await this.executeCommand(name, params, context);
      };
    }
    
    return bridge;
  }
}