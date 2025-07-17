/**
 * Agents Command - List available AI agents/personas
 */

import { DirectCommand } from '../direct-command/DirectCommand';
import { CommandDefinition, CommandResult, ContinuumContext } from '../base-command/BaseCommand';

export class AgentsCommand extends DirectCommand {
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

  protected static async executeOperation(_params: any = {}, _context: ContinuumContext): Promise<CommandResult> {
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