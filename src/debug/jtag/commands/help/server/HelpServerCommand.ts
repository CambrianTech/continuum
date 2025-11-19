/**
 * Help Command - Server Implementation
 *
 * Auto-generates help documentation by querying the list command.
 * Returns human-readable help text formatted for AI consumption.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { HelpParams, HelpResult } from '../shared/HelpTypes';
import { createHelpResultFromParams } from '../shared/HelpTypes';
import { Commands } from '../../../system/core/shared/Commands';
import type { ListResult, CommandSignature } from '../../list/shared/ListTypes';

export class HelpServerCommand extends CommandBase<HelpParams, HelpResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('help', context, subpath, commander);
  }

  /**
   * Generate help documentation
   */
  async execute(params: JTAGPayload): Promise<HelpResult> {
    const helpParams = params as HelpParams;

    console.log(`📚 SERVER: Generating help documentation${helpParams.commandName ? ` for ${helpParams.commandName}` : ''}`);

    try {
      // Get all commands via list command
      const listResult = await Commands.execute('list', {
        context: helpParams.context,
        sessionId: helpParams.sessionId
      }) as ListResult;

      if (!listResult.success || !listResult.commands) {
        throw new Error('Failed to fetch command list');
      }

      // If specific command requested, show detailed help for that command
      if (helpParams.commandName) {
        const command = listResult.commands.find((cmd: CommandSignature) => cmd.name === helpParams.commandName);

        if (!command) {
          return createHelpResultFromParams(helpParams, {
            success: false,
            error: `Command '${helpParams.commandName}' not found`,
            helpText: `Unknown command: ${helpParams.commandName}\n\nUse 'help' without parameters to see all available commands.`
          });
        }

        return createHelpResultFromParams(helpParams, {
          success: true,
          signature: command,
          helpText: this.formatDetailedHelp(command, helpParams.showExamples ?? true)
        });
      }

      // Otherwise show general help with all commands
      const helpText = this.formatGeneralHelp([...listResult.commands]);

      return createHelpResultFromParams(helpParams, {
        success: true,
        helpText
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to generate help:`, error);

      return createHelpResultFromParams(helpParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        helpText: 'Failed to generate help documentation. See error for details.'
      });
    }
  }

  /**
   * Format detailed help for a specific command
   */
  private formatDetailedHelp(command: CommandSignature, showExamples: boolean): string {
    let help = `📚 ${command.name}\n`;
    help += `${'='.repeat(command.name.length + 3)}\n\n`;
    help += `${command.description}\n\n`;
    help += `Category: ${command.category}\n\n`;

    // Required parameters (exclude internal system parameters)
    const requiredParams = Object.entries(command.params || {})
      .filter(([name, def]) => def.required && !['context', 'sessionId'].includes(name));

    if (requiredParams.length > 0) {
      help += `REQUIRED PARAMETERS:\n`;
      for (const [name, def] of requiredParams) {
        help += `  • ${name} (${def.type})${def.description ? `: ${def.description}` : ''}\n`;
      }
      help += '\n';
    }

    // Optional parameters (exclude internal system parameters)
    const optionalParams = Object.entries(command.params || {})
      .filter(([name, def]) => !def.required && !['context', 'sessionId'].includes(name));

    if (optionalParams.length > 0) {
      help += `OPTIONAL PARAMETERS:\n`;
      for (const [name, def] of optionalParams) {
        help += `  • ${name} (${def.type})${def.description ? `: ${def.description}` : ''}\n`;
      }
      help += '\n';
    }

    // Return type (exclude standard result fields that all commands have)
    if (command.returns) {
      const returnFields = Object.entries(command.returns)
        .filter(([name]) => !['success', 'context', 'sessionId'].includes(name));

      if (returnFields.length > 0) {
        help += `RETURNS:\n`;
        for (const [name, def] of returnFields) {
          help += `  • ${name} (${def.type})${def.description ? `: ${def.description}` : ''}\n`;
        }
        help += '\n';
      }
    }

    // Usage examples
    if (showExamples) {
      help += this.generateExamples(command);
    }

    return help;
  }

  /**
   * Format general help listing all commands
   */
  private formatGeneralHelp(commands: CommandSignature[]): string {
    let help = `📚 JTAG COMMAND REFERENCE\n`;
    help += `${'='.repeat(25)}\n\n`;
    help += `Total commands available: ${commands.length}\n\n`;
    help += `To get detailed help for a specific command:\n`;
    help += `  <tool_use>\n`;
    help += `  <tool_name>help</tool_name>\n`;
    help += `  <parameters>\n`;
    help += `  <commandName>command/name</commandName>\n`;
    help += `  </parameters>\n`;
    help += `  </tool_use>\n\n`;

    // Group by category
    const categories = ['browser', 'server', 'system'] as const;

    for (const category of categories) {
      const categoryCommands = commands.filter(cmd => cmd.category === category);

      if (categoryCommands.length === 0) continue;

      help += `\n${category.toUpperCase()} COMMANDS (${categoryCommands.length}):\n`;
      help += `${'-'.repeat(category.length + 15)}\n`;

      for (const cmd of categoryCommands.sort((a, b) => a.name.localeCompare(b.name))) {
        help += `  • ${cmd.name}\n`;
        help += `    ${cmd.description}\n`;
      }
    }

    return help;
  }

  /**
   * Generate usage examples for a command
   */
  private generateExamples(command: CommandSignature): string {
    let examples = `USAGE EXAMPLES:\n\n`;

    // Generate XML example
    examples += `1. Basic usage:\n`;
    examples += `<tool_use>\n`;
    examples += `<tool_name>${command.name}</tool_name>\n`;
    examples += `<parameters>\n`;

    // Add required parameters
    const requiredParams = Object.entries(command.params || {})
      .filter(([name, def]) => def.required && !['context', 'sessionId'].includes(name));

    if (requiredParams.length > 0) {
      for (const [name, def] of requiredParams) {
        const exampleValue = this.getExampleValue(name, def.type);
        examples += `<${name}>${exampleValue}</${name}>\n`;
      }
    }

    examples += `</parameters>\n`;
    examples += `</tool_use>\n`;

    // Command-specific examples
    if (command.name === 'code/find') {
      examples += `\n2. Find files containing "PersonaUser":\n`;
      examples += `<tool_use>\n`;
      examples += `<tool_name>code/find</tool_name>\n`;
      examples += `<parameters>\n`;
      examples += `<pattern>PersonaUser</pattern>\n`;
      examples += `</parameters>\n`;
      examples += `</tool_use>\n`;
    } else if (command.name === 'data/list') {
      examples += `\n2. List all users:\n`;
      examples += `<tool_use>\n`;
      examples += `<tool_name>data/list</tool_name>\n`;
      examples += `<parameters>\n`;
      examples += `<collection>users</collection>\n`;
      examples += `</parameters>\n`;
      examples += `</tool_use>\n`;
    } else if (command.name === 'code/read') {
      examples += `\n2. Read a specific file:\n`;
      examples += `<tool_use>\n`;
      examples += `<tool_name>code/read</tool_name>\n`;
      examples += `<parameters>\n`;
      examples += `<path>system/user/server/PersonaUser.ts</path>\n`;
      examples += `</parameters>\n`;
      examples += `</tool_use>\n`;
    }

    return examples;
  }

  /**
   * Get example value for a parameter type
   */
  private getExampleValue(name: string, type: string): string {
    // Type-specific examples
    if (type === 'string') {
      if (name.includes('path') || name.includes('file')) return 'path/to/file.ts';
      if (name.includes('pattern')) return 'searchPattern';
      if (name.includes('collection')) return 'users';
      if (name.includes('selector')) return 'div.example';
      if (name.includes('url')) return 'https://example.com';
      return 'exampleValue';
    }

    if (type === 'number') return '10';
    if (type === 'boolean') return 'true';
    if (type === 'object') return '{"key":"value"}';
    if (type === 'array') return '["item1","item2"]';

    return 'value';
  }
}
