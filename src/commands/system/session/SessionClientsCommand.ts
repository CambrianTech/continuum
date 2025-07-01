/**
 * Session Clients Command - List clients connected to a session
 */

import { BaseCommand, CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';

export class SessionClientsCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-clients',
      description: 'List clients connected to a session',
      category: 'system',
      parameters: {
        sessionId: {
          type: 'string',
          description: 'Session ID (leave blank for current session)',
          required: false
        },
        format: {
          type: 'string',
          description: 'Output format',
          required: false,
          enum: ['table', 'json', 'simple']
        }
      },
      examples: [
        { description: 'List clients in current session', command: 'session-clients' },
        { description: 'List clients in specific session', command: 'session-clients abc123' },
        { description: 'List clients as JSON', command: 'session-clients --format=json' }
      ]
    };
  }

  static async execute(params: any, context: CommandContext): Promise<CommandResult> {
    try {
      const { sessionId, format = 'table' } = params;
      
      // Get SessionManager from context
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return this.createErrorResult('Session manager not available');
      }

      // Get session (current if no ID provided)
      let targetSession;
      if (sessionId) {
        targetSession = sessionManager.getSession(sessionId);
        if (!targetSession) {
          return this.createErrorResult(`Session ${sessionId} not found`);
        }
      } else {
        // Get current session
        targetSession = sessionManager.getLatestSession({
          owner: 'system',
          type: 'development',
          active: true
        });
        if (!targetSession) {
          return this.createErrorResult('No current session found');
        }
      }

      // Get connected clients for this session
      const connections = sessionManager.getSessionConnections(targetSession.id);
      
      const clientsInfo = connections.map((conn: any) => ({
        connectionId: conn.connectionId,
        source: conn.identity?.starter || 'unknown',
        owner: conn.identity?.identity?.name || conn.identity?.identity?.user || 'unknown',
        connectedAt: conn.connectedAt,
        lastActive: conn.lastActive || conn.connectedAt,
        capabilities: conn.capabilities || []
      }));

      return this.createSuccessResult(`Found ${clientsInfo.length} clients in session ${targetSession.id}`, {
        sessionId: targetSession.id,
        clientCount: clientsInfo.length,
        clients: clientsInfo,
        format
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Session clients query failed: ${errorMessage}`);
    }
  }

  private static async getSessionManager(context: CommandContext): Promise<any> {
    // Try to get session manager daemon from WebSocket daemon's registered daemons
    if (context?.websocket?.registeredDaemons) {
      const sessionManagerDaemon = context.websocket.registeredDaemons.get('session-manager');
      return sessionManagerDaemon || null;
    }
    
    // Fallback: look for session manager in context
    return context?.sessionManager || null;
  }
}