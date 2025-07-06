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
      description: 'Connect to daemon system with session management',
      parameters: {
        sessionId: {
          type: 'string',
          description: 'Specific session ID to connect to',
          optional: true
        },
        sessionType: {
          type: 'string',
          description: 'Type of session (development, persona, etc.)',
          optional: true,
          default: 'development'
        },
        owner: {
          type: 'string',
          description: 'Session owner (user, system, etc.)',
          optional: true,
          default: 'user'
        },
        forceNew: {
          type: 'boolean',
          description: 'Force create new session instead of reusing existing',
          optional: true,
          default: false
        }
      },
      examples: [
        { 
          description: 'Connect to shared development session', 
          command: `{}` 
        },
        { 
          description: 'Connect to specific session', 
          command: `{"sessionId": "development-system-abc123"}` 
        },
        { 
          description: 'Create new persona session', 
          command: `{"sessionType": "persona", "owner": "alice", "forceNew": true}` 
        }
      ],
      usage: 'Thin client entry point with session affinity - like web session cookies'
    };
  }

  protected static async executeOperation(params: any = {}, context?: CommandContext): Promise<CommandResult> {
    try {
      // Get session manager daemon through proper WebSocket context
      const sessionManagerDaemon = context?.websocket?.registeredDaemons?.get('session-manager');
      
      if (!sessionManagerDaemon) {
        return this.createErrorResult('SessionManagerDaemon not available in context - system not properly initialized');
      }

      // Extract session parameters from the request
      const sessionId = params.sessionId || '';
      const sessionType = params.sessionType || 'development';
      const owner = params.owner || 'user';
      const forceNew = params.forceNew || false;
      
      // Determine session preference based on parameters
      let sessionPreference: string;
      if (forceNew) {
        sessionPreference = 'new';
      } else if (sessionId) {
        sessionPreference = sessionId;
      } else {
        sessionPreference = 'current'; // Default to shared session
      }

      // Connect to session manager with parameters
      const sessionResult = await sessionManagerDaemon.handleConnect({
        source: 'continuum-cli',
        owner: owner,
        sessionPreference: sessionPreference,
        capabilities: ['browser', 'commands', 'screenshots'],
        context: 'development',
        type: sessionType
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