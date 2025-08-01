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
    
    console.log(`📋 BROWSER: Listing available commands (category: ${listParams.category ?? 'all'})`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        // Determine category based on command name or implementation
        let category: 'browser' | 'server' | 'system' = 'browser';
        if (commandName.includes('file/') || commandName.includes('compile-')) {
          category = 'server';
        } else if (commandName.includes('list') || commandName.includes('health')) {
          category = 'system';
        }

        // Filter by category if specified
        if (listParams.category && listParams.category !== 'all' && category !== listParams.category) {
          continue;
        }

        const signature: CommandSignature = {
          name: commandName,
          description: `${commandName} command - ${category} operation`,
          category,
          params: {
            // TODO: Extract actual parameter types from command
            context: { type: 'JTAGContext', required: true },
            sessionId: { type: 'UUID', required: true }
          },
          returns: {
            // TODO: Extract actual return types from command
            success: { type: 'boolean', description: 'Operation success status' },
            context: { type: 'JTAGContext', description: 'Request context' },
            sessionId: { type: 'UUID', description: 'Session identifier' }
          }
        };

        commandSignatures.push(signature);
      }

      console.log(`✅ BROWSER: Found ${commandSignatures.length} available commands`);

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