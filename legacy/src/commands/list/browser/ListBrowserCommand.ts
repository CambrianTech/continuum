/**
 * List Command - Browser Implementation
 * 
 * Discovers and returns available commands from the browser-side CommandDaemon system.
 * Essential command for client command discovery - works locally in browser.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type ListParams, type ListResult, type CommandSignature, createListResultFromParams } from '../shared/ListTypes';

export class ListBrowserCommand extends CommandBase<ListParams, ListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('list', context, subpath, commander);
  }

  /**
   * Browser discovers available commands from the local CommandDaemon
   */
  async execute(params: JTAGPayload): Promise<ListResult> {
    const listParams = params as ListParams;

    console.log(`📋 BROWSER: Listing available commands`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        const signature: CommandSignature = {
          name: commandName,
          description: `${commandName} command`
        };

        commandSignatures.push(signature);
      }



      return createListResultFromParams(listParams, {
        success: true,
        commands: commandSignatures,
        totalCount: commandSignatures.length
      });

    } catch (error) {
      console.error(`❌ BROWSER: Failed to list commands:`, error);
      
      return createListResultFromParams(listParams, {
        success: false,
        commands: [],
        totalCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}