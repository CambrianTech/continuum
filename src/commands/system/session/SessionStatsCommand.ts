/**
 * Session Stats Command - Get system-wide session statistics
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionStatsCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-stats',
      description: 'Get session statistics and system overview',
      category: 'system',
      parameters: {
        includeDetails: {
          type: 'boolean',
          description: 'Include detailed session breakdown',
          required: false
        },
        groupBy: {
          type: 'string',
          description: 'Group statistics by field',
          required: false,
          enum: ['type', 'owner', 'starter', 'active']
        }
      },
      examples: [
        {
          description: 'Get detailed session statistics',
          command: 'session-stats --includeDetails=true --groupBy=type'
        }
      ]
    };
  }

  static async execute(params: any, context: any): Promise<CommandResult> {
    try {
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return { success: false, error: 'Session manager not available' };
      }

      const { includeDetails = false, groupBy } = params;

      // Get system stats
      const statsMessage = {
        id: `stats-${Date.now()}`,
        from: 'session-stats-command',
        to: 'session-manager',
        type: 'get_session_stats',
        timestamp: new Date(),
        data: {}
      };

      const statsResponse = await sessionManager.handleMessage(statsMessage);
      
      if (!statsResponse.success) {
        return {
          success: false,
          error: statsResponse.error || 'Failed to get session stats'
        };
      }

      let result = {
        overview: statsResponse.data,
        timestamp: new Date().toISOString()
      };

      // Add detailed breakdown if requested
      if (includeDetails || groupBy) {
        const listMessage = {
          id: `list-details-${Date.now()}`,
          from: 'session-stats-command',
          to: 'session-manager',
          type: 'list_sessions',
          timestamp: new Date(),
          data: { filter: {} }
        };

        const listResponse = await sessionManager.handleMessage(listMessage);
        
        if (listResponse.success) {
          const sessions = listResponse.data.sessions;
          
          if (includeDetails) {
            result.details = this.generateDetailedStats(sessions);
          }
          
          if (groupBy) {
            result.groupedBy = {
              field: groupBy,
              groups: this.groupSessions(sessions, groupBy)
            };
          }
        }
      }

      return {
        success: true,
        data: result
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get session stats: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    if (context?.websocket?.registeredDaemons) {
      return context.websocket.registeredDaemons.get('session-manager') || null;
    }
    return context?.sessionManager || null;
  }

  private static generateDetailedStats(sessions: any[]): any {
    const now = new Date();
    const oneHourAgo = new Date(now.getTime() - 60 * 60 * 1000);
    const oneDayAgo = new Date(now.getTime() - 24 * 60 * 60 * 1000);
    const oneWeekAgo = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000);

    const activeSessions = sessions.filter(s => s.isActive);
    const recentSessions = sessions.filter(s => new Date(s.lastActive) > oneHourAgo);
    const todaySessions = sessions.filter(s => new Date(s.created) > oneDayAgo);
    const weekSessions = sessions.filter(s => new Date(s.created) > oneWeekAgo);

    // Calculate average session duration for completed sessions
    const completedSessions = sessions.filter(s => !s.isActive);
    const avgDuration = completedSessions.length > 0 
      ? completedSessions.reduce((sum, s) => {
          const duration = new Date(s.lastActive).getTime() - new Date(s.created).getTime();
          return sum + duration;
        }, 0) / completedSessions.length
      : 0;

    return {
      activity: {
        activeSessions: activeSessions.length,
        recentlyActive: recentSessions.length,
        createdToday: todaySessions.length,
        createdThisWeek: weekSessions.length
      },
      duration: {
        averageSessionDurationMs: Math.round(avgDuration),
        averageSessionDurationHours: Math.round(avgDuration / (60 * 60 * 1000) * 100) / 100
      },
      storage: {
        totalSessions: sessions.length,
        uniqueOwners: new Set(sessions.map(s => s.owner)).size,
        storageDirectories: sessions.map(s => s.artifacts?.storageDir).filter(Boolean)
      }
    };
  }

  private static groupSessions(sessions: any[], groupBy: string): any {
    const groups: Record<string, any> = {};

    sessions.forEach(session => {
      let key: string;
      
      switch (groupBy) {
        case 'type':
          key = session.type;
          break;
        case 'owner':
          key = session.owner;
          break;
        case 'active':
          key = session.isActive ? 'active' : 'inactive';
          break;
        case 'starter':
          // Try to extract starter from session ID pattern
          key = session.id.split('-')[0] || 'unknown';
          break;
        default:
          key = 'unknown';
      }

      if (!groups[key]) {
        groups[key] = {
          count: 0,
          sessions: [],
          active: 0,
          inactive: 0
        };
      }

      groups[key].count++;
      groups[key].sessions.push({
        id: session.id,
        owner: session.owner,
        created: session.created,
        isActive: session.isActive
      });
      
      if (session.isActive) {
        groups[key].active++;
      } else {
        groups[key].inactive++;
      }
    });

    return groups;
  }
}