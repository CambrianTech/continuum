/**
 * Session List Command - List and filter sessions
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionListCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-list',
      description: 'List all sessions with optional filtering',
      category: 'system',
      parameters: {
        filter: {
          type: 'object',
          description: 'Filter criteria',
          required: false,
          properties: {
            type: { type: 'string' },
            owner: { type: 'string' },
            active: { type: 'boolean' },
            starter: { type: 'string' }
          }
        },
        format: {
          type: 'string',
          description: 'Output format',
          required: false,
          enum: ['table', 'json', 'summary']
        }
      }
    };
  }

  static async execute(params: any, context: any): Promise<CommandResult> {
    try {
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return { success: false, error: 'Session manager not available' };
      }

      const { filter, format = 'table' } = params;

      const message = {
        id: `list-${Date.now()}`,
        from: 'session-list-command',
        to: 'session-manager',
        type: 'list_sessions',
        timestamp: new Date(),
        data: { filter }
      };

      const response = await sessionManager.handleMessage(message);
      
      if (!response.success) {
        return {
          success: false,
          error: response.error || 'Failed to list sessions'
        };
      }

      const sessions = response.data.sessions;

      return {
        success: true,
        data: {
          sessions: this.formatSessions(sessions, format),
          total: sessions.length,
          filter: filter || {},
          format
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to list sessions: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    if (context?.websocket?.registeredDaemons) {
      return context.websocket.registeredDaemons.get('session-manager') || null;
    }
    return context?.sessionManager || null;
  }

  private static formatSessions(sessions: any[], format: string): any {
    switch (format) {
      case 'summary':
        return sessions.map(session => ({
          id: session.id,
          owner: session.owner,
          type: session.type,
          created: session.created,
          active: session.isActive
        }));
        
      case 'json':
        return sessions;
        
      case 'table':
      default:
        return sessions.map(session => ({
          ID: session.id,
          Owner: session.owner,
          Type: session.type,
          Created: new Date(session.created).toLocaleString(),
          Active: session.isActive ? '✅' : '❌',
          'Storage Dir': session.artifacts?.storageDir || 'N/A'
        }));
    }
  }
}