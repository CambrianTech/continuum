/**
 * Personas Command - List available AI personas
 */

import { DirectCommand } from '../direct-command/DirectCommand.js';
import { CommandDefinition, CommandResult, ContinuumContext } from '../base-command/BaseCommand.js';

export class PersonasCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'personas',
      description: 'List available AI personas and their capabilities',
      category: 'core',
      parameters: {
        active: {
          type: 'boolean',
          description: 'Show only active personas',
          required: false,
          default: false
        }
      },
      examples: [
        {
          description: 'List all personas',
          command: 'personas'
        },
        {
          description: 'List active personas only',
          command: 'personas --active=true'
        }
      ]
    };
  }

  protected static async executeOperation(params: any = {}, _context?: ContinuumContext): Promise<CommandResult> {
    try {
      const showActiveOnly = params.active || false;

      // Mock personas data
      const mockPersonas = [
        {
          id: 'claude-developer',
          name: 'Claude Developer',
          description: 'Expert software developer and code reviewer',
          capabilities: ['typescript', 'react', 'node.js', 'testing', 'debugging'],
          status: 'active',
          lastUsed: new Date(Date.now() - 300000).toISOString(),
          sessions: 15
        },
        {
          id: 'claude-architect',
          name: 'Claude Architect',
          description: 'System architecture and design specialist',
          capabilities: ['system-design', 'microservices', 'databases', 'scalability'],
          status: 'active',
          lastUsed: new Date(Date.now() - 600000).toISOString(),
          sessions: 8
        },
        {
          id: 'claude-ui-ux',
          name: 'Claude UI/UX',
          description: 'User interface and experience design expert',
          capabilities: ['ui-design', 'user-experience', 'accessibility', 'css'],
          status: 'inactive',
          lastUsed: new Date(Date.now() - 86400000).toISOString(),
          sessions: 3
        }
      ];

      const filteredPersonas = showActiveOnly 
        ? mockPersonas.filter(p => p.status === 'active')
        : mockPersonas;

      return this.createSuccessResult(
        `Found ${filteredPersonas.length} personas (${showActiveOnly ? 'active' : 'all'})`,
        {
          personas: filteredPersonas,
          count: filteredPersonas.length,
          filter: showActiveOnly ? 'active' : 'all'
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to retrieve personas: ${errorMessage}`);
    }
  }
}