/**
 * List Command - Server Implementation
 * 
 * Discovers and returns available commands from the CommandDaemon system.
 * Essential command for client command discovery.
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { type ListParams, type ListResult, type CommandSignature, createListResultFromParams } from '../shared/ListTypes';

export class ListServerCommand extends CommandBase<ListParams, ListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('list', context, subpath, commander);
  }

  /**
   * Server discovers available commands from the CommandDaemon
   */
  async execute(params: JTAGPayload): Promise<ListResult> {
    const listParams = params as ListParams;
    
    console.log(`📋 SERVER: Listing available commands (category: ${listParams.category ?? 'all'})`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        // Determine category based on command name or implementation
        let category: 'browser' | 'server' | 'system' = 'server';
        if (commandName.includes('screenshot') || commandName.includes('click') || commandName.includes('navigate')) {
          category = 'browser';
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

      console.log(`✅ SERVER: Found ${commandSignatures.length} available commands`);

      return createListResultFromParams(listParams, {
        success: true,
        commands: commandSignatures,
        totalCount: commandSignatures.length
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to list commands:`, error);
      
      return createListResultFromParams(listParams, {
        success: false,
        commands: [],
        totalCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}