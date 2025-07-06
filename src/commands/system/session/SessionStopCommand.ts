/**
 * Session Stop Command - Stop/terminate a session
 */

import { BaseCommand, CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';

export class SessionStopCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-stop',
      description: 'Stop/terminate a session',
      category: 'system',
      parameters: {
        sessionId: {
          type: 'string',
          description: 'Session ID to stop (leave blank for current session)',
          required: false
        },
        force: {
          type: 'boolean',
          description: 'Force stop without cleanup',
          required: false
        },
        saveArtifacts: {
          type: 'boolean',
          description: 'Save artifacts before stopping',
          required: false,
          default: true
        }
      },
      examples: [
        { description: 'Stop current session', command: 'session-stop' },
        { description: 'Stop specific session', command: 'session-stop abc123' },
        { description: 'Force stop without cleanup', command: 'session-stop abc123 --force' }
      ]
    };
  }

  static async execute(params: any, context: CommandContext): Promise<CommandResult> {
    try {
      const { sessionId, force = false, saveArtifacts = true } = params;
      
      // Get SessionManager from context
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return this.createErrorResult('Session manager not available');
      }

      // Get session to stop (current if no ID provided)
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

      // Check if session has active connections
      const connections = sessionManager.getSessionConnections(targetSession.id);
      if (connections.length > 0 && !force) {
        return this.createErrorResult(`Session ${targetSession.id} has ${connections.length} active connections. Use --force to stop anyway.`);
      }

      // Stop the session
      const stopResult = await sessionManager.stopSession(targetSession.id, {
        force,
        saveArtifacts,
        reason: 'Manual stop via command'
      });

      if (!stopResult.success) {
        return this.createErrorResult(stopResult.error || 'Failed to stop session');
      }

      // Emit session stop event
      sessionManager.emitSessionEvent({
        type: 'session_stopped',
        sessionId: targetSession.id,
        timestamp: new Date(),
        metadata: {
          stoppedBy: 'session-stop-command',
          force,
          saveArtifacts,
          connectionCount: connections.length
        }
      });

      return this.createSuccessResult(`Session ${targetSession.id} stopped successfully`, {
        sessionId: targetSession.id,
        stopped: true,
        force,
        artifactsSaved: saveArtifacts,
        connectionsTerminated: connections.length,
        stopTime: new Date().toISOString()
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Session stop failed: ${errorMessage}`);
    }
  }

  private static async getSessionManager(context: CommandContext): Promise<any> {
    // Try to get session manager daemon from WebSocket daemon's registered daemons
    if (context?.websocket && typeof context.websocket === 'object' && 'registeredDaemons' in context.websocket) {
      const websocketWithDaemons = context.websocket as any;
      const sessionManagerDaemon = websocketWithDaemons.registeredDaemons?.get('session-manager');
      return sessionManagerDaemon || null;
    }
    
    // Fallback: look for session manager in context
    return context?.sessionManager || null;
  }
}