/**
 * Help Command - Server Implementation
 *
 * Dynamically queries the 'list' command to provide help documentation.
 * Returns human-readable help text formatted for AI consumption.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { HelpParams, HelpResult } from '../shared/HelpTypes';
import { createHelpResultFromParams } from '../shared/HelpTypes';
import type { ListResult, CommandSignature } from '../../list/shared/ListTypes';
import { createListParams } from '../../list/shared/ListTypes';
import { Commands } from '../../../system/core/shared/Commands';

export class HelpServerCommand extends CommandBase<HelpParams, HelpResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('help', context, subpath, commander);
  }


  /**
   * Generate help documentation by querying 'list' command dynamically
   */
  async execute(params: JTAGPayload): Promise<HelpResult> {
    const helpParams = params as HelpParams;
    const commandName = helpParams.commandName ?? 'help';

    console.log(`📚 SERVER: Getting help for ${commandName}`);

    try {
      // Special case: Help for help command itself OR general tool format help
      if (commandName === 'help') {
        return createHelpResultFromParams(helpParams, {
          success: true,
          signature: {
            name: 'help',
            description: `Get help documentation for a specific command.

TOOL FORMAT (use this XML structure for ALL commands):
<tool name="command/name">
<paramName>value</paramName>
</tool>

Example with parameters:
<tool name="code/read">
<path>system/tools/server/ToolRegistry.ts</path>
</tool>

Example without parameters:
<tool name="list">
</tool>

To see a specific command's parameters: help --commandName="command/name"
To see all available commands: list`,
            params: {
              commandName: {
                type: 'string',
                required: false,
                description: 'Name of command to get help for (defaults to "help")'
              }
            }
          }
        });
      }

      // Query 'list' command dynamically to get all available commands
      const listParams = createListParams(helpParams.context, helpParams.sessionId, {
        includeDescription: true,
        includeSignature: true
      });

      const listResult = await Commands.execute<ListResult>('list', listParams) as ListResult;

      if (!listResult.success) {
        return createHelpResultFromParams(helpParams, {
          success: false,
          error: `Failed to query available commands: ${listResult.error ?? 'Unknown error'}`
        });
      }

      // Find the requested command in the list
      const schema: CommandSignature | undefined = listResult.commands.find(
        (cmd: CommandSignature) => cmd.name === commandName
      );

      if (!schema) {
        return createHelpResultFromParams(helpParams, {
          success: false,
          error: `Command '${commandName}' not found. Use 'list' to see available commands.`
        });
      }

      // Generate exact XML format example for this command
      const paramsExample = Object.keys(schema.params ?? {}).length > 0
        ? '\n' + Object.entries(schema.params ?? {})
            .map(([name, def]) => `<${name}>${def.description ?? 'value'}</${name}>`)
            .join('\n')
        : '';

      const usageExample = `

USAGE:
<tool name="${commandName}">${paramsExample}
</tool>`;

      // Enhance description with usage example
      const enhancedSchema = {
        ...schema,
        description: schema.description + usageExample
      };

      return createHelpResultFromParams(helpParams, {
        success: true,
        signature: enhancedSchema
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to generate help:`, error);

      return createHelpResultFromParams(helpParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
