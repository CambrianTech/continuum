/**
 * Projects Command - List active projects and their status
 */

import { BaseCommand } from '../base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../../types/CommandTypes';

export class ProjectsCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'projects',
      description: 'List active projects and their status',
      category: 'core',
      parameters: {},
      examples: [
        {
          description: 'List all projects',
          command: 'projects'
        }
      ]
    };
  }

  static async execute(_params: any = {}): Promise<CommandResult> {
    try {
      // Mock project data based on what we saw in the UI
      const projects = [
        {
          id: 'continuum-os',
          name: 'Continuum OS',
          status: 'active',
          progress: 75,
          timeAgo: '2 minutes ago',
          team: ['Claude Sonnet', 'Protocol Sheriff'],
          description: 'Core operating system for AI workforce construction'
        },
        {
          id: 'widget-system', 
          name: 'Widget System',
          status: 'active',
          progress: 45,
          timeAgo: '15 minutes ago',
          team: ['Code Specialist'],
          description: 'TypeScript widget architecture and health monitoring'
        }
      ];

      const activeCount = projects.filter(p => p.status === 'active').length;
      return this.createSuccessResult(
        `Found ${projects.length} projects (${activeCount} active)`,
        {
          projects,
          count: projects.length,
          activeCount
        }
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to get projects: ${errorMessage}`);
    }
  }
}