/**
 * Session Fork Command - Fork an existing session
 */

import { BaseCommand, CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';

export class SessionForkCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-fork',
      description: 'Fork an existing session to create a new one',
      category: 'system',
      parameters: {
        sessionId: {
          type: 'string' as const,
          description: 'Session ID to fork from',
          required: true
        },
        newOwner: {
          type: 'string' as const, 
          description: 'Owner of the new forked session',
          required: false
        },
        type: {
          type: 'string' as const,
          description: 'Type of the new session (development, testing, production, persona)',
          required: false
        }
      },
      examples: [
        { description: 'Fork current session', command: 'session-fork abc123' },
        { description: 'Fork with new owner', command: 'session-fork abc123 --newOwner=alice' }
      ]
    };
  }

  static async execute(params: any, context: CommandContext): Promise<CommandResult> {
    try {
      const { sessionId, newOwner = 'system', type = 'development' } = params;
      
      if (!sessionId) {
        return this.createErrorResult('sessionId is required');
      }

      // Get SessionManager from context
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return this.createErrorResult('Session manager not available');
      }

      // Use connection orchestration to fork
      const forkResult = await sessionManager.handleConnect({
        source: 'session-fork-command',
        owner: newOwner,
        sessionPreference: `fork:${sessionId}`,
        capabilities: ['browser', 'commands', 'screenshots'],
        context: 'forked-session',
        type
      });

      if (!forkResult.success) {
        return this.createErrorResult(forkResult.error || 'Fork operation failed');
      }

      return this.createSuccessResult(`Forked session ${sessionId} to ${forkResult.data.sessionId}`, {
        originalSessionId: sessionId,
        newSessionId: forkResult.data.sessionId,
        action: forkResult.data.action,
        newOwner,
        logs: forkResult.data.logs,
        screenshots: forkResult.data.screenshots,
        commands: forkResult.data.commands
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Session fork failed: ${errorMessage}`);
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