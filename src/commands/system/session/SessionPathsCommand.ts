/**
 * Session Paths Command - Get session file paths for integration
 * Perfect for getting log paths, screenshot directories, etc.
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionPathsCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-paths',
      description: 'Get file paths for session artifacts (logs, screenshots, etc.)',
      category: 'system',
      parameters: {
        sessionId: {
          type: 'string',
          description: 'Session ID to get paths for',
          required: false
        },
        owner: {
          type: 'string',
          description: 'Get paths for latest session by owner',
          required: false
        },
        pathType: {
          type: 'string',
          description: 'Specific path type to return',
          required: false,
          enum: ['logs', 'screenshots', 'files', 'recordings', 'devtools', 'all']
        },
        format: {
          type: 'string',
          description: 'Output format',
          required: false,
          enum: ['object', 'array', 'shell']
        }
      },
      examples: [
        {
          description: 'Get session log paths',
          command: 'session-paths --sessionId=my-session --pathType=logs'
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

      const { sessionId, owner, pathType = 'all', format = 'object' } = params;

      let targetSessionId = sessionId;

      // If no sessionId but owner provided, find latest session for owner
      if (!targetSessionId && owner) {
        const listMessage = {
          id: `find-latest-${Date.now()}`,
          from: 'session-paths-command',
          to: 'session-manager',
          type: 'list_sessions',
          timestamp: new Date(),
          data: {
            filter: {
              owner,
              active: true
            }
          }
        };

        const listResponse = await sessionManager.handleMessage(listMessage);
        
        if (!listResponse.success || listResponse.data.sessions.length === 0) {
          return {
            success: false,
            error: `No active sessions found for owner: ${owner}`
          };
        }

        // Get the most recent session
        const sessions = listResponse.data.sessions;
        const latestSession = sessions.reduce((latest: any, session: any) => 
          new Date(session.lastActive) > new Date(latest.lastActive) ? session : latest
        );
        
        targetSessionId = latestSession.id;
      }

      if (!targetSessionId) {
        return {
          success: false,
          error: 'Either sessionId or owner must be provided'
        };
      }

      // Get session info
      const infoMessage = {
        id: `info-${Date.now()}`,
        from: 'session-paths-command',
        to: 'session-manager',
        type: 'get_session',
        timestamp: new Date(),
        data: { sessionId: targetSessionId }
      };

      const infoResponse = await sessionManager.handleMessage(infoMessage);
      
      if (!infoResponse.success) {
        return {
          success: false,
          error: infoResponse.error || `Session ${targetSessionId} not found`
        };
      }

      const session = infoResponse.data.session;
      const storageDir = session.artifacts.storageDir;

      // Build paths object
      const allPaths = {
        sessionId: session.id,
        owner: session.owner,
        storageDir,
        logs: {
          browser: `${storageDir}/logs/browser.log`,
          server: `${storageDir}/logs/server.log`,
          directory: `${storageDir}/logs`
        },
        screenshots: {
          directory: `${storageDir}/screenshots`,
          pattern: `${storageDir}/screenshots/*.png`
        },
        files: {
          directory: `${storageDir}/files`,
          pattern: `${storageDir}/files/*`
        },
        recordings: {
          directory: `${storageDir}/recordings`,
          pattern: `${storageDir}/recordings/*.webm`
        },
        devtools: {
          directory: `${storageDir}/devtools`,
          pattern: `${storageDir}/devtools/*.json`
        }
      };

      // Filter by pathType if specified
      let result: any;
      if (pathType === 'all') {
        result = allPaths;
      } else {
        result = {
          sessionId: session.id,
          owner: session.owner,
          storageDir,
          [pathType]: allPaths[pathType as keyof typeof allPaths]
        };
      }

      // Format output
      const formattedResult = this.formatPaths(result, format);

      return {
        success: true,
        data: formattedResult
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get session paths: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    if (context?.websocket?.registeredDaemons) {
      return context.websocket.registeredDaemons.get('session-manager') || null;
    }
    return context?.sessionManager || null;
  }

  private static formatPaths(paths: any, format: string): any {
    switch (format) {
      case 'array':
        return this.pathsToArray(paths);
        
      case 'shell':
        return this.pathsToShellVars(paths);
        
      case 'object':
      default:
        return { paths };
    }
  }

  private static pathsToArray(paths: any): any {
    const result: string[] = [];
    
    const addPaths = (obj: any, prefix = '') => {
      Object.entries(obj).forEach(([key, value]) => {
        if (typeof value === 'string' && (value.includes('/') || value.includes('\\'))) {
          result.push(value);
        } else if (typeof value === 'object' && value !== null) {
          addPaths(value, prefix ? `${prefix}.${key}` : key);
        }
      });
    };
    
    addPaths(paths);
    return { paths: result };
  }

  private static pathsToShellVars(paths: any): any {
    const vars: Record<string, string> = {};
    
    vars.SESSION_ID = paths.sessionId;
    vars.SESSION_OWNER = paths.owner;
    vars.SESSION_DIR = paths.storageDir;
    
    if (paths.logs) {
      vars.BROWSER_LOG = paths.logs.browser;
      vars.SERVER_LOG = paths.logs.server;
      vars.LOGS_DIR = paths.logs.directory;
    }
    
    if (paths.screenshots) {
      vars.SCREENSHOTS_DIR = paths.screenshots.directory;
    }
    
    if (paths.files) {
      vars.FILES_DIR = paths.files.directory;
    }
    
    if (paths.recordings) {
      vars.RECORDINGS_DIR = paths.recordings.directory;
    }
    
    if (paths.devtools) {
      vars.DEVTOOLS_DIR = paths.devtools.directory;
    }

    return {
      variables: vars,
      export_commands: Object.entries(vars).map(([key, value]) => `export ${key}="${value}"`)
    };
  }
}