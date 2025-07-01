/**
 * Session Command - Query and manage Continuum sessions
 * 
 * Provides access to the session management system via commands
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session',
      description: 'Query and manage Continuum sessions',
      category: 'system',
      parameters: {}
    };
  }

  static async execute(params: any, context: any): Promise<CommandResult> {
    try {
      const { action, sessionId, filter, identity } = params;
      
      // Get SessionManager from registered daemons
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return {
          success: false,
          error: 'Session manager not available'
        };
      }

      switch (action) {
        case 'list':
          return this.handleList(sessionManager, filter);
          
        case 'current':
          return this.handleCurrent(sessionManager, context);
          
        case 'latest':
          return this.handleLatest(sessionManager, filter);
          
        case 'join':
          if (!sessionId) {
            return { success: false, error: 'sessionId required for join action' };
          }
          return this.handleJoin(sessionManager, sessionId, context);
          
        case 'create':
          if (!identity) {
            return { success: false, error: 'identity required for create action' };
          }
          return this.handleCreate(sessionManager, identity);
          
        case 'close':
          if (!sessionId) {
            return { success: false, error: 'sessionId required for close action' };
          }
          return this.handleClose(sessionManager, sessionId);
          
        case 'info':
          if (!sessionId) {
            return { success: false, error: 'sessionId required for info action' };
          }
          return this.handleInfo(sessionManager, sessionId);
          
        default:
          return {
            success: false,
            error: `Unknown action: ${action}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Session command failed: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    // Try to get session manager daemon from WebSocket daemon's registered daemons
    if (context?.websocket?.registeredDaemons) {
      const sessionManagerDaemon = context.websocket.registeredDaemons.get('session-manager');
      return sessionManagerDaemon || null;
    }
    
    // Fallback: look for session manager in context
    return context?.sessionManager || null;
  }

  private static async handleList(sessionManager: any, filter?: any): Promise<CommandResult> {
    try {
      const message = {
        id: `list-${Date.now()}`,
        from: 'session-command',
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

      return {
        success: true,
        data: {
          sessions: response.data.sessions.map((session: any) => ({
            id: session.id,
            type: session.type,
            owner: session.owner,
            created: session.created,
            lastActive: session.lastActive,
            isActive: session.isActive,
            storageDir: session.artifacts.storageDir
          })),
          count: response.data.total
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

  private static async handleCurrent(sessionManager: any, context: any): Promise<CommandResult> {
    // Get current session from connection context
    const connectionId = context?.connectionId;
    if (!connectionId) {
      return { success: false, error: 'No connection context available' };
    }

    const identity = sessionManager.getConnectionIdentity(connectionId);
    if (!identity) {
      return { success: false, error: 'Connection not identified' };
    }

    // Find current active session for this connection
    const currentSession = sessionManager.getLatestSession({
      starter: identity.starter,
      user: identity.identity.user || identity.identity.name,
      active: true
    });

    if (!currentSession) {
      return { success: false, error: 'No current session found' };
    }

    return {
      success: true,
      data: {
        session: {
          id: currentSession.id,
          identity: currentSession.identity,
          type: currentSession.type,
          owner: currentSession.owner,
          created: currentSession.created,
          lastActive: currentSession.lastActive,
          isActive: currentSession.isActive,
          storageDir: currentSession.artifacts.storageDir
        }
      }
    };
  }

  private static async handleLatest(sessionManager: any, filter?: any): Promise<CommandResult> {
    const latestSession = sessionManager.getLatestSession(filter);
    
    if (!latestSession) {
      return {
        success: false,
        error: 'No sessions found matching filter criteria'
      };
    }

    return {
      success: true,
      data: {
        session: {
          id: latestSession.id,
          identity: latestSession.identity,
          type: latestSession.type,
          owner: latestSession.owner,
          created: latestSession.created,
          lastActive: latestSession.lastActive,
          isActive: latestSession.isActive,
          storageDir: latestSession.artifacts.storageDir
        }
      }
    };
  }

  private static async handleJoin(sessionManager: any, sessionId: string, context: any): Promise<CommandResult> {
    const connectionId = context?.connectionId;
    if (!connectionId) {
      return { success: false, error: 'No connection context available' };
    }

    try {
      const joinedSessionId = await sessionManager.joinSession(connectionId, sessionId);
      
      return {
        success: true,
        data: {
          sessionId: joinedSessionId,
          action: 'joined',
          message: `Successfully joined session ${sessionId}`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to join session: ${errorMessage}`
      };
    }
  }

  private static async handleCreate(sessionManager: any, identity: any): Promise<CommandResult> {
    try {
      const sessionId = await sessionManager.createSession(identity);
      
      return {
        success: true,
        data: {
          sessionId,
          action: 'created',
          message: `Successfully created session ${sessionId}`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to create session: ${errorMessage}`
      };
    }
  }

  private static async handleClose(sessionManager: any, sessionId: string): Promise<CommandResult> {
    try {
      await sessionManager.closeSession(sessionId);
      
      return {
        success: true,
        data: {
          sessionId,
          action: 'closed',
          message: `Successfully closed session ${sessionId}`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to close session: ${errorMessage}`
      };
    }
  }

  private static async handleInfo(sessionManager: any, sessionId: string): Promise<CommandResult> {
    const session = sessionManager.getSession(sessionId);
    
    if (!session) {
      return {
        success: false,
        error: `Session ${sessionId} not found`
      };
    }

    return {
      success: true,
      data: {
        session: {
          id: session.id,
          identity: session.identity,
          type: session.type,
          owner: session.owner,
          created: session.created,
          lastActive: session.lastActive,
          isActive: session.isActive,
          artifacts: {
            storageDir: session.artifacts.storageDir,
            logs: {
              server: session.artifacts.logs.server.length,
              client: session.artifacts.logs.client.length
            },
            screenshots: session.artifacts.screenshots.length,
            files: session.artifacts.files.length,
            recordings: session.artifacts.recordings.length
          },
          mainBrowser: session.mainBrowser,
          devTools: session.devTools
        }
      }
    };
  }
}