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
      // Get the actual current session information from the session manager
      // The session should have been created when the client connected
      
      // Try to get the session info from the WebSocket context or session manager
      let sessionInfo: any = null;
      
      // Check if we have access to the session manager daemon through context
      const sessionManagerDaemon = context?.websocket?.registeredDaemons?.get('session-manager');
      
      if (sessionManagerDaemon) {
        // Get the current active session
        const sessionResult = await sessionManagerDaemon.handleConnect({
          source: 'continuum-cli',
          owner: 'system',
          sessionPreference: 'current',
          capabilities: ['browser', 'commands', 'screenshots'],
          context: 'development',
          type: 'development'
        });
        
        if (sessionResult.success) {
          sessionInfo = {
            sessionId: sessionResult.data.sessionId,
            action: sessionResult.data.action,
            launched: sessionResult.data.launched,
            logPaths: sessionResult.data.logs,
            directories: {
              screenshots: sessionResult.data.screenshots
            },
            interface: sessionResult.data.interface,
            commands: sessionResult.data.commands
          };
        }
      }
      
      // Fallback: Find the most recent session directory if daemon access failed
      if (!sessionInfo) {
        const fs = await import('fs/promises');
        const path = await import('path');
        
        try {
          // Look for the most recent session in user/system
          const sessionBasePath = '.continuum/sessions/user/system';
          const sessions = await fs.readdir(sessionBasePath);
          
          if (sessions.length > 0) {
            // Get the most recent session (sessions are timestamped)
            const mostRecentSession = sessions.sort().pop();
            const sessionPath = path.join(sessionBasePath, mostRecentSession!);
            
            sessionInfo = {
              sessionId: mostRecentSession,
              action: 'joined_existing',
              launched: true,
              logPaths: {
                browser: `${sessionPath}/logs/browser.log`,
                server: `${sessionPath}/logs/server.log`
              },
              directories: {
                screenshots: `${sessionPath}/screenshots`
              },
              interface: 'http://localhost:9000',
              commands: {
                info: `continuum session-info ${mostRecentSession}`,
                stop: `continuum session-stop ${mostRecentSession}`
              }
            };
          }
        } catch (error) {
          // Final fallback if filesystem access fails
          console.warn('Could not access session directory:', error);
        }
      }
      
      // If we still don't have session info, return error
      if (!sessionInfo) {
        return this.createErrorResult('No active session found and could not create one');
      }

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