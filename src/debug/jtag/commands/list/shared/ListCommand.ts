/**
 * List Command - Shared Implementation
 * 
 * Universal command that discovers and returns available commands from the CommandDaemon system.
 * This is the SINGLE DEPENDENCY command that enables all dynamic command discovery.
 * Works in both browser and server environments.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type ListParams, type ListResult, type CommandSignature, createListResultFromParams } from './ListTypes';

export class ListCommand extends CommandBase<ListParams, ListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('list', context, subpath, commander);
  }

  /**
   * Universal command discovery - works in both browser and server environments
   */
  async execute(params: JTAGPayload): Promise<ListResult> {
    const listParams = params as ListParams;
    const env = this.context.environment;
    
    console.log(`📋 ${env.toUpperCase()}: Listing available commands (category: ${listParams.category ?? 'all'})`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        // Determine category based on command name or implementation
        let category: 'browser' | 'server' | 'system' = env === 'browser' ? 'browser' : 'server';
        
        // Override category based on command characteristics
        if (commandName.includes('file/') || commandName.includes('compile-')) {
          category = 'server';
        } else if (commandName.includes('screenshot') || commandName.includes('click') || commandName.includes('navigate')) {
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

      console.log(`✅ ${env.toUpperCase()}: Found ${commandSignatures.length} available commands`);

      return createListResultFromParams(listParams, {
        success: true,
        commands: commandSignatures,
        totalCount: commandSignatures.length
      });

    } catch (error) {
      console.error(`❌ ${env.toUpperCase()}: Failed to list commands:`, error);
      
      return createListResultFromParams(listParams, {
        success: false,
        commands: [],
        totalCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}