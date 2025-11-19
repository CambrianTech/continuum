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
import type { CommandSignature } from '../../list/shared/ListTypes';
import * as fs from 'fs';
import * as path from 'path';

export class HelpServerCommand extends CommandBase<HelpParams, HelpResult> {
  private schemas: Record<string, CommandSignature> = {};

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('help', context, subpath, commander);
    this.loadSchemas();
  }

  private loadSchemas(): void {
    try {
      // Load pre-generated schemas from build-time script
      const schemasPath = path.join(__dirname, '../../../generated/command-schemas.json');

      if (fs.existsSync(schemasPath)) {
        const schemasJson = fs.readFileSync(schemasPath, 'utf-8');
        this.schemas = JSON.parse(schemasJson);
        console.log(`📚 Loaded ${Object.keys(this.schemas).length} command schemas from generated file`);
      } else {
        console.warn(`⚠️  Command schemas file not found: ${schemasPath}`);
        console.warn(`   Run: npx tsx scripts/generate-command-schemas.ts`);
      }
    } catch (error) {
      console.error(`❌ Failed to load command schemas:`, error);
    }
  }


  /**
   * Generate help documentation
   */
  async execute(params: JTAGPayload): Promise<HelpResult> {
    const helpParams = params as HelpParams;
    const commandName = helpParams.commandName || 'help';

    console.log(`📚 SERVER: Getting help for ${commandName}`);

    try {
      // Special case: Help for help command itself
      if (commandName === 'help') {
        return createHelpResultFromParams(helpParams, {
          success: true,
          signature: {
            name: 'help',
            description: 'Get help documentation for a specific command',
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

      // Look up schema from generated schemas
      const schema = this.schemas[commandName];

      if (!schema) {
        // Check if command exists in CommandDaemon
        if (!this.commander.commands.has(commandName)) {
          return createHelpResultFromParams(helpParams, {
            success: false,
            error: `Command '${commandName}' not found. Use 'list' to see available commands.`
          });
        }

        // Command exists but no schema generated yet
        return createHelpResultFromParams(helpParams, {
          success: true,
          signature: {
            name: commandName,
            description: `${commandName} command`,
            params: {}
          }
        });
      }

      return createHelpResultFromParams(helpParams, {
        success: true,
        signature: schema
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
