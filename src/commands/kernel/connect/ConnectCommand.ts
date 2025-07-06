/**
 * Connect Command - Thin client entry point to daemon system
 * 
 * Returns session information for display by thin clients
 * Delegates to SessionManagerDaemon for actual session management
 */

import { DaemonCommand } from '../../core/daemon-command/DaemonCommand';
import { CommandDefinition, CommandContext } from '../../core/base-command/BaseCommand';

export class ConnectCommand extends DaemonCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'connect',
      category: 'kernel',
      icon: '🔌',
      description: 'Connect to daemon system with session management',
      parameters: {
        sessionId: {
          type: 'string' as const,
          description: 'Specific session ID to connect to',
          required: false
        },
        sessionType: {
          type: 'string' as const,
          description: 'Type of session (development, persona, etc.)',
          required: false,
          default: 'development'
        },
        owner: {
          type: 'string' as const,
          description: 'Session owner (user, system, etc.)',
          required: false,
          default: 'user'
        },
        forceNew: {
          type: 'boolean' as const,
          description: 'Force create new session instead of reusing existing',
          required: false,
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

  /**
   * Specify which daemon handles this command
   */
  protected static getTargetDaemon(): string {
    return 'session-manager';
  }
  
  /**
   * Specify the message type for the daemon
   */
  protected static getMessageType(): string {
    return 'session.connect';
  }
  
  /**
   * Prepare the data to send to SessionManagerDaemon
   */
  protected static prepareDaemonData(params: any, context?: CommandContext): any {
    return {
      sessionType: params.sessionType || 'development',
      owner: params.owner || 'shared',
      sessionId: params.sessionId,
      forceNew: params.forceNew || false,
      connectionId: context?.connectionId || 'cli',
      source: (context as any)?.source || 'cli'
    };
  }

}