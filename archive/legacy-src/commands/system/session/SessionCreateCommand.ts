/**
 * Session Create Command - Create new sessions with rich identity
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../core/base-command/types';

export class SessionCreateCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'session-create',
      description: 'Create a new session with custom identity',
      category: 'system',
      parameters: {
        starter: {
          type: 'string' as const,
          description: 'Who/what is starting this session (cli, portal, persona, git-hook, api, test)',
          required: true
        },
        name: {
          type: 'string' as const,
          description: 'Session name (e.g., user name, project name)',
          required: true
        },
        type: {
          type: 'string' as const,
          description: 'Session type (development, debugging, testing, automation, collaboration)',
          required: false
        },
        user: {
          type: 'string',
          description: 'User identifier',
          required: false
        },
        project: {
          type: 'string',
          description: 'Project name',
          required: false
        },
        branch: {
          type: 'string',
          description: 'Git branch',
          required: false
        },
        task: {
          type: 'string',
          description: 'Task description',
          required: false
        },
        description: {
          type: 'string',
          description: 'Session description',
          required: false
        },
        autoCleanup: {
          type: 'boolean',
          description: 'Enable automatic cleanup',
          required: false
        },
        cleanupHours: {
          type: 'number',
          description: 'Hours before auto-cleanup',
          required: false
        }
      },
      examples: [
        {
          description: 'Create development session',
          command: 'session-create --starter=system --name=dev-session'
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

      const {
        starter,
        name,
        type = 'development',
        user,
        project,
        branch,
        task,
        description,
        autoCleanup = true,
        cleanupHours = 2
      } = params;

      // Build session creation request
      const sessionType = this.mapTypeToSessionType(type);
      const options: any = {
        autoCleanup,
        cleanupAfterMs: cleanupHours * 60 * 60 * 1000
      };

      // Add metadata context
      if (project || branch || task || description) {
        options.sessionContext = [task, branch, project].filter(Boolean).join('-');
      }

      const message = {
        id: `create-${Date.now()}`,
        from: 'session-create-command',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: sessionType,
          owner: name,
          options
        }
      };

      const response = await sessionManager.handleMessage(message);
      
      if (!response.success) {
        return {
          success: false,
          error: response.error || 'Failed to create session'
        };
      }

      const session = response.data.session;

      return {
        success: true,
        data: {
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
            user: user || name,
            type,
            metadata: {
              project,
              branch,
              task,
              description
            }
          }
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