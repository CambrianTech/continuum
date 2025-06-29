/**
 * Agents Command - List available AI agents/personas
 */

import { BaseCommand } from '../base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../../types/CommandTypes';

export class AgentsCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'agents',
      description: 'List available AI agents and personas',
      category: 'core',
      parameters: {},
      examples: [
        {
          description: 'List all agents',
          command: 'agents'
        }
      ]
    };
  }

  static async execute(params: any = {}): Promise<CommandResult> {
    try {
      // Mock agent data for now
      const agents = [
        {
          id: 'claude',
          name: 'Claude',
          type: 'assistant', 
          status: 'available',
          description: 'AI assistant for general tasks'
        },
        {
          id: 'developer',
          name: 'Developer',
          type: 'persona',
          status: 'available', 
          description: 'Specialized in software development'
        }
      ];

      return this.createSuccessResult({
        agents,
        count: agents.length
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to get agents: ${errorMessage}`);
    }
  }
}