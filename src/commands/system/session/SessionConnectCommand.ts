/**
 * Session Connect Command - Connect to existing session or create new
 * Implements the CLI default behavior: connect to existing or create new
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionConnectCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-connect',
      description: 'Connect to existing session or create new one (CLI default behavior)',
      category: 'system',
      parameters: {
        name: {
          type: 'string',
          description: 'User/connection name',
          required: true
        },
        starter: {
          type: 'string',
          description: 'Connection type',
          required: false,
          enum: ['cli', 'portal', 'persona', 'git-hook', 'api', 'test']
        },
        type: {
          type: 'string',
          description: 'Session type preference',
          required: false,
          enum: ['development', 'debugging', 'testing', 'automation', 'collaboration']
        },
        project: {
          type: 'string',
          description: 'Project context',
          required: false
        },
        branch: {
          type: 'string',
          description: 'Git branch context',
          required: false
        },
        forceNew: {
          type: 'boolean',
          description: 'Force creation of new session instead of connecting to existing',
          required: false
        },
        autoCleanup: {
          type: 'boolean',
          description: 'Enable automatic cleanup for new sessions',
          required: false
        }
      }
    };
  }

  static async execute(params: any, context: any): Promise<CommandResult> {
    try {
      const sessionManager = await this.getSessionManager(context);
      if (!sessionManager) {
        return { success: false, error: 'Session manager not available' };
      }

      const {
        name,
        starter = 'cli',
        type = 'development',
        project,
        branch,
        forceNew = false,
        autoCleanup = true
      } = params;

      // Step 1: Look for existing session if not forcing new
      if (!forceNew) {
        const listMessage = {
          id: `find-${Date.now()}`,
          from: 'session-connect-command',
          to: 'session-manager',
          type: 'list_sessions',
          timestamp: new Date(),
          data: {
            filter: {
              owner: name,
              active: true
            }
          }
        };

        const listResponse = await sessionManager.handleMessage(listMessage);
        
        if (listResponse.success && listResponse.data.sessions.length > 0) {
          // Found existing session(s) - connect to the most recent
          const sessions = listResponse.data.sessions;
          const latestSession = sessions.reduce((latest: any, session: any) => 
            new Date(session.lastActive) > new Date(latest.lastActive) ? session : latest
          );

          return {
            success: true,
            data: {
              action: 'connected',
              sessionId: latestSession.id,
              session: {
                id: latestSession.id,
                type: latestSession.type,
                owner: latestSession.owner,
                created: latestSession.created,
                lastActive: latestSession.lastActive,
                storageDir: latestSession.artifacts.storageDir,
                logPaths: {
                  browser: `${latestSession.artifacts.storageDir}/logs/browser.log`,
                  server: `${latestSession.artifacts.storageDir}/logs/server.log`
                }
              },
              message: `Connected to existing session: ${latestSession.id}`
            }
          };
        }
      }

      // Step 2: Create new session
      const sessionType = this.mapTypeToSessionType(type);
      const options: any = {
        autoCleanup,
        cleanupAfterMs: 2 * 60 * 60 * 1000 // 2 hours default
      };

      // Add context for session naming
      if (project || branch) {
        options.sessionContext = [branch, project].filter(Boolean).join('-');
      }

      const createMessage = {
        id: `create-${Date.now()}`,
        from: 'session-connect-command',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: sessionType,
          owner: name,
          options
        }
      };

      const createResponse = await sessionManager.handleMessage(createMessage);
      
      if (!createResponse.success) {
        return {
          success: false,
          error: createResponse.error || 'Failed to create new session'
        };
      }

      const session = createResponse.data.session;

      return {
        success: true,
        data: {
          action: 'created',
          sessionId: session.id,
          session: {
            id: session.id,
            type: session.type,
            owner: session.owner,
            created: session.created,
            storageDir: session.artifacts.storageDir,
            logPaths: {
              browser: `${session.artifacts.storageDir}/logs/browser.log`,
              server: `${session.artifacts.storageDir}/logs/server.log`
            },
            settings: {
              autoCleanup: session.shouldAutoCleanup,
              cleanupAfterHours: Math.round(session.cleanupAfterMs / (60 * 60 * 1000))
            }
          },
          identity: {
            starter,
            name,
            type,
            metadata: {
              project,
              branch
            }
          },
          message: `Created new session: ${session.id}`
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to connect to session: ${errorMessage}`
      };
    }
  }

  private static async getSessionManager(context: any): Promise<any> {
    if (context?.websocket?.registeredDaemons) {
      return context.websocket.registeredDaemons.get('session-manager') || null;
    }
    return context?.sessionManager || null;
  }

  private static mapTypeToSessionType(type: string): string {
    const mapping: Record<string, string> = {
      'development': 'development',
      'debugging': 'development', 
      'testing': 'test',
      'automation': 'git-hook',
      'collaboration': 'portal'
    };
    return mapping[type] || 'development';
  }
}