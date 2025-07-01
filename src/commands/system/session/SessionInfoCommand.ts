/**
 * Session Info Command - Get detailed session information
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionInfoCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-info',
      description: 'Get detailed information about a specific session',
      category: 'system',
      parameters: {
        sessionId: {
          type: 'string',
          description: 'Session ID to get info for',
          required: true
        },
        includeArtifacts: {
          type: 'boolean',
          description: 'Include artifact file listings',
          required: false
        },
        includePaths: {
          type: 'boolean',
          description: 'Include full file paths',
          required: false
        }
      },
      examples: [
        {
          description: 'Get session info with artifacts',
          command: 'session-info --sessionId=my-session --includeArtifacts=true'
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

      const { sessionId, includeArtifacts = false, includePaths = true } = params;

      const message = {
        id: `info-${Date.now()}`,
        from: 'session-info-command',
        to: 'session-manager',
        type: 'get_session',
        timestamp: new Date(),
        data: { sessionId }
      };

      const response = await sessionManager.handleMessage(message);
      
      if (!response.success) {
        return {
          success: false,
          error: response.error || `Session ${sessionId} not found`
        };
      }

      const session = response.data.session;

      // Format session info
      const sessionInfo = {
        id: session.id,
        type: session.type,
        owner: session.owner,
        created: session.created,
        lastActive: session.lastActive,
        isActive: session.isActive,
        
        // Storage info
        storage: {
          directory: includePaths ? session.artifacts.storageDir : this.getRelativePath(session.artifacts.storageDir),
          logs: {
            browser: includePaths ? `${session.artifacts.storageDir}/logs/browser.log` : 'logs/browser.log',
            server: includePaths ? `${session.artifacts.storageDir}/logs/server.log` : 'logs/server.log'
          },
          directories: {
            screenshots: includePaths ? `${session.artifacts.storageDir}/screenshots` : 'screenshots/',
            files: includePaths ? `${session.artifacts.storageDir}/files` : 'files/',
            recordings: includePaths ? `${session.artifacts.storageDir}/recordings` : 'recordings/',
            devtools: includePaths ? `${session.artifacts.storageDir}/devtools` : 'devtools/'
          }
        },

        // Process info
        processes: session.processes || {},

        // Session settings
        settings: {
          autoCleanup: session.shouldAutoCleanup,
          cleanupAfterMs: session.cleanupAfterMs,
          cleanupAfterHours: Math.round(session.cleanupAfterMs / (60 * 60 * 1000))
        }
      };

      // Include artifact counts if requested
      if (includeArtifacts) {
        sessionInfo.artifacts = {
          logs: {
            server: session.storage?.logs?.server ? 1 : 0,
            client: session.storage?.logs?.browser ? 1 : 0
          },
          screenshots: session.storage?.directories?.screenshots ? 1 : 0,
          files: session.storage?.directories?.files ? 1 : 0,
          recordings: session.storage?.directories?.recordings ? 1 : 0,
          devtools: session.artifacts.devtools?.length || 0
        };
      }

      return {
        success: true,
        data: { session: sessionInfo }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get session info: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    if (context?.websocket?.registeredDaemons) {
      return context.websocket.registeredDaemons.get('session-manager') || null;
    }
    return context?.sessionManager || null;
  }

  private static getRelativePath(fullPath: string): string {
    const cwd = process.cwd();
    if (fullPath.startsWith(cwd)) {
      return fullPath.substring(cwd.length + 1);
    }
    return fullPath;
  }
}