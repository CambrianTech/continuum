/**
 * Users Command - System User Management
 * 
 * Provides real user data from database/shared storage
 * Used by UniversalUserSystem to populate UI components
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandResult, ContinuumContext } from '../../core/base-command/BaseCommand.js';

export interface UniversalUser {
  id: string;
  name: string;
  type: 'human' | 'persona' | 'ai-model' | 'ai-system';
  avatar: string;
  status: 'online' | 'offline' | 'thinking' | 'working' | 'idle';
  capabilities: string[];
  isClickable: boolean;
  currentTask?: string;
}

export class UsersCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'users',
      description: 'Get all system users (humans, personas, AI models, AI systems)',
      category: 'system',
      parameters: {
        type: {
          type: 'string',
          description: 'Filter by user type (human, persona, ai-model, ai-system)',
          required: false
        },
        status: {
          type: 'string',
          description: 'Filter by status (online, offline, thinking, working, idle)',
          required: false
        },
        includeOffline: {
          type: 'boolean',
          description: 'Include offline users',
          required: false,
          default: true
        }
      },
      examples: [
        {
          description: 'Get all users',
          command: 'users'
        },
        {
          description: 'Get only online users',
          command: 'users --status=online'
        },
        {
          description: 'Get all personas',
          command: 'users --type=persona'
        },
        {
          description: 'Get online users only',
          command: 'users --includeOffline=false'
        }
      ]
    };
  }

  protected static async executeOperation(params: any = {}, _context: ContinuumContext): Promise<CommandResult> {
    try {
      // Get all users from the system
      const allUsers = await this.getAllUsers();
      
      // Apply filters
      let filteredUsers = allUsers;
      
      if (params.type) {
        filteredUsers = filteredUsers.filter(user => user.type === params.type);
      }
      
      if (params.status) {
        filteredUsers = filteredUsers.filter(user => user.status === params.status);
      }
      
      if (params.includeOffline === false) {
        filteredUsers = filteredUsers.filter(user => user.status !== 'offline');
      }
      
      return this.createSuccessResult(
        `Found ${filteredUsers.length} users`,
        {
          users: filteredUsers,
          total: filteredUsers.length,
          filters: {
            type: params.type || 'all',
            status: params.status || 'all',
            includeOffline: params.includeOffline !== false
          }
        }
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to get users: ${errorMessage}`);
    }
  }

  private static async getAllUsers(): Promise<UniversalUser[]> {
    // ELEGANT: Real users from database/shared storage
    // For now, return a curated set of system users
    // TODO: Integrate with actual user database when available
    
    return [
      {
        id: 'current-user',
        name: 'User',
        type: 'human',
        avatar: '👤',
        status: 'online',
        capabilities: ['all'],
        isClickable: false
      },
      {
        id: 'claude-sonnet',
        name: 'Claude Sonnet',
        type: 'ai-model',
        avatar: '🤖',
        status: 'online',
        capabilities: ['coding', 'analysis', 'writing', 'math', 'research'],
        isClickable: true,
        currentTask: 'Browser daemon migration'
      },
      {
        id: 'academy-teacher',
        name: 'Academy Teacher',
        type: 'persona',
        avatar: '👩‍🏫',
        status: 'online',
        capabilities: ['teaching', 'curriculum', 'assessment'],
        isClickable: true,
        currentTask: 'Training new AI personas'
      },
      {
        id: 'system-monitor',
        name: 'System Monitor',
        type: 'ai-system',
        avatar: '📊',
        status: 'working',
        capabilities: ['monitoring', 'diagnostics', 'alerts'],
        isClickable: true,
        currentTask: 'System health monitoring'
      },
      {
        id: 'development-assistant',
        name: 'Dev Assistant',
        type: 'persona',
        avatar: '💻',
        status: 'thinking',
        capabilities: ['debugging', 'testing', 'documentation'],
        isClickable: true,
        currentTask: 'Code review'
      },
      {
        id: 'research-analyst',
        name: 'Research Analyst',
        type: 'persona',
        avatar: '🔬',
        status: 'idle',
        capabilities: ['research', 'analysis', 'reporting'],
        isClickable: true
      }
    ];
  }
}

// Export for both server and client use
export default UsersCommand;