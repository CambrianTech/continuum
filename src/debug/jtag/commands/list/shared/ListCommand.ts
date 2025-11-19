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

    console.log(`📋 ${env.toUpperCase()}: Listing available commands`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        // Get user-facing parameters (exclude framework injection params)
        const userParams = this.extractUserFacingParams(commandName);

        const signature: CommandSignature = {
          name: commandName,
          description: userParams.description || `${commandName} command`,
          params: userParams.params,
          returns: {
            success: { type: 'boolean', description: 'Operation success status' }
          }
        };

        commandSignatures.push(signature);
      }


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

  /**
   * Extract user-facing parameters for a command (exclude framework injection params)
   *
   * Framework params that are auto-injected and should NOT be exposed to tools:
   * - context (JTAGContext)
   * - sessionId (UUID)
   * - backend (JTAGEnvironment)
   */
  private extractUserFacingParams(commandName: string): {
    description: string;
    params: Record<string, { type: string; required: boolean; description?: string }>;
  } {
    // Define user-facing params for each command
    // This is a static registry that should be kept in sync with actual command types
    const commandParams: Record<string, { description: string; params: Record<string, any> }> = {
      // Code commands
      'code/read': {
        description: 'Read file contents from the codebase',
        params: {
          path: { type: 'string', required: true, description: 'File path (relative to repository root)' },
          startLine: { type: 'number', required: false, description: 'Start line (1-indexed)' },
          endLine: { type: 'number', required: false, description: 'End line (1-indexed)' },
          includeMetadata: { type: 'boolean', required: false, description: 'Include file metadata' },
          forceRefresh: { type: 'boolean', required: false, description: 'Force bypass cache' }
        }
      },

      // Chat commands
      'chat/send': {
        description: 'Send chat message to a room',
        params: {
          message: { type: 'string', required: true, description: 'Message text to send' },
          room: { type: 'string', required: false, description: 'Room name or ID (defaults to general)' },
          senderId: { type: 'UUID', required: false, description: 'Sender user ID (defaults to current user)' },
          replyToId: { type: 'UUID', required: false, description: 'Reply to message ID (creates threaded reply)' },
          isSystemTest: { type: 'boolean', required: false, description: 'Mark as system test (AIs will ignore)' }
        }
      },

      'chat/export': {
        description: 'Export chat messages to markdown format',
        params: {
          room: { type: 'string', required: false, description: 'Room name or ID (defaults to all rooms)' },
          limit: { type: 'number', required: false, description: 'Max number of messages to export' },
          output: { type: 'string', required: false, description: 'Output file path (defaults to stdout)' },
          includeSystem: { type: 'boolean', required: false, description: 'Include system messages' }
        }
      },

      // Data commands
      'data/create': {
        description: 'Create new data record in collection',
        params: {
          collection: { type: 'string', required: true, description: 'Collection name' },
          data: { type: 'object', required: true, description: 'Data to store' }
        }
      },

      'data/read': {
        description: 'Read data record by ID',
        params: {
          collection: { type: 'string', required: true, description: 'Collection name' },
          id: { type: 'UUID', required: true, description: 'Record ID' }
        }
      },

      'data/list': {
        description: 'List data records in collection',
        params: {
          collection: { type: 'string', required: true, description: 'Collection name' },
          filter: { type: 'object', required: false, description: 'Filter conditions' },
          orderBy: { type: 'array', required: false, description: 'Sort order' },
          limit: { type: 'number', required: false, description: 'Max number of records' }
        }
      },

      'data/update': {
        description: 'Update existing data record',
        params: {
          collection: { type: 'string', required: true, description: 'Collection name' },
          id: { type: 'UUID', required: true, description: 'Record ID' },
          data: { type: 'object', required: true, description: 'Updated data' }
        }
      },

      'data/delete': {
        description: 'Delete data record by ID',
        params: {
          collection: { type: 'string', required: true, description: 'Collection name' },
          id: { type: 'UUID', required: true, description: 'Record ID' }
        }
      },

      // Decision commands
      'decision/propose': {
        description: 'Propose a decision for collaborative voting',
        params: {
          question: { type: 'string', required: true, description: 'Decision question' },
          context: { type: 'string', required: false, description: 'Context/background information' },
          options: { type: 'array', required: true, description: 'Array of option strings' },
          scope: { type: 'string', required: false, description: 'Scope: room ID or "global" (defaults to current room)' }
        }
      },

      'decision/rank': {
        description: 'Submit ranked preferences for a decision',
        params: {
          decisionId: { type: 'UUID', required: true, description: 'Decision ID' },
          rankings: { type: 'array', required: true, description: 'Array of ranked option IDs (highest to lowest preference)' },
          reasoning: { type: 'string', required: false, description: 'Explanation of ranking rationale' }
        }
      },

      'decision/finalize': {
        description: 'Finalize a decision after voting period',
        params: {
          decisionId: { type: 'UUID', required: true, description: 'Decision ID to finalize' }
        }
      },

      // Screenshot command
      'screenshot': {
        description: 'Capture screenshot of browser viewport or element',
        params: {
          querySelector: { type: 'string', required: false, description: 'CSS selector for element to screenshot (defaults to body)' },
          filename: { type: 'string', required: false, description: 'Output filename (defaults to auto-generated)' },
          fullPage: { type: 'boolean', required: false, description: 'Capture full page (not just viewport)' }
        }
      },

      // System commands
      'ping': {
        description: 'Health check for system connectivity',
        params: {}
      },

      'list': {
        description: 'List all available commands with their signatures',
        params: {
          includeDescription: { type: 'boolean', required: false, description: 'Include descriptions' },
          includeSignature: { type: 'boolean', required: false, description: 'Include parameter signatures' }
        }
      },

      'tree': {
        description: 'Display hierarchical command structure',
        params: {
          filter: { type: 'string', required: false, description: 'Filter to specific command path (e.g., "ai" shows ai/*)' },
          showDescriptions: { type: 'boolean', required: false, description: 'Show command descriptions alongside names' },
          maxDepth: { type: 'number', required: false, description: 'Maximum depth to display (1 = top level only, 2 = one level deep, etc.)' }
        }
      }
    };

    // Return command params or generic fallback
    return commandParams[commandName] || {
      description: `${commandName} command`,
      params: {}
    };
  }
}