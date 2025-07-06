/**
 * Connect Command - Thin client entry point to daemon system
 * 
 * Returns session information for display by thin clients
 * Delegates to SessionManagerDaemon for actual session management
 */

import { DaemonCommand } from '../../core/daemon-command/DaemonCommand.js';
import { CommandDefinition, CommandContext } from '../../core/base-command/BaseCommand.js';

export class ConnectCommand extends DaemonCommand {
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

  protected static getTargetDaemon(): string {
    return 'session-manager';
  }
  
  protected static getMessageType(): string {
    return 'connect';
  }
  
  protected static prepareDaemonData(params: any, context?: CommandContext): any {
    const sessionType = params.sessionType || 'development';
    const owner = params.owner || 'shared';
    const requestedSessionId = params.sessionId;
    const forceNew = params.forceNew || false;
    const connectionId = context?.connectionId || 'cli';
    
    // Determine session preference
    let sessionPreference = 'current'; // default: join existing
    if (forceNew) {
      sessionPreference = 'new';
    } else if (requestedSessionId) {
      sessionPreference = requestedSessionId;
    }
    
    // Prepare capabilities based on connection type
    const capabilities = connectionId === 'cli' ? ['browser'] : [];
    
    return {
      source: 'connect-command',
      owner: owner,
      sessionPreference: sessionPreference,
      capabilities: capabilities,
      context: sessionType,
      sessionType: sessionType
    };
  }

}