/**
 * Connect Command - Thin client entry point to daemon system
 * 
 * Returns session information for display by thin clients
 * Delegates to SessionManagerDaemon for actual session management
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand.js';

export class ConnectCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'connect',
      category: 'kernel',
      icon: '🔌',
      description: 'Connect to daemon system and return session information',
      parameters: {},
      examples: [
        { 
          description: 'Connect to daemon system', 
          command: `{}` 
        }
      ],
      usage: 'Thin client entry point - returns session info for display'
    };
  }

  protected static async executeOperation(_params: any = {}, context?: CommandContext): Promise<CommandResult> {
    try {
      // Get session manager daemon through proper WebSocket context
      const sessionManagerDaemon = context?.websocket?.registeredDaemons?.get('session-manager');
      
      if (!sessionManagerDaemon) {
        return this.createErrorResult('SessionManagerDaemon not available in context - system not properly initialized');
      }

      // Get the current active session from session manager
      const sessionResult = await sessionManagerDaemon.handleConnect({
        source: 'continuum-cli',
        owner: 'system',
        sessionPreference: 'current',
        capabilities: ['browser', 'commands', 'screenshots'],
        context: 'development',
        type: 'development'
      });
      
      if (!sessionResult.success) {
        return this.createErrorResult(`Session manager connection failed: ${sessionResult.error}`);
      }

      // Transform session manager response to ConnectCommand format
      const sessionInfo = {
        sessionId: sessionResult.data.sessionId,
        action: sessionResult.data.action,
        launched: sessionResult.data.launched.browser || sessionResult.data.launched.webserver, // Convert object to boolean
        logPaths: sessionResult.data.logs,
        directories: {
          screenshots: sessionResult.data.screenshots
        },
        interface: sessionResult.data.interface,
        commands: sessionResult.data.commands
      };

      return this.createSuccessResult(
        `Connected to development session ${sessionInfo.sessionId}`,
        {
          session: sessionInfo,
          sessionContext: {
            sessionId: sessionInfo.sessionId,
            logPaths: sessionInfo.logPaths,
            directories: sessionInfo.directories
          }
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Connection failed: ${errorMessage}`);
    }
  }

}