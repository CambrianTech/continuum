/**
 * Daemon Message Utilities - Helper functions for creating consistent daemon messages
 * 
 * Ensures all daemon communication follows the same DaemonProtocol structure
 */

import { DaemonMessage } from './DaemonProtocol';

export class DaemonMessageUtils {
  /**
   * Create a properly structured daemon message
   */
  static createMessage(params: {
    id: string;
    from: string;
    to: string;
    type: string;
    data: any;
    priority?: 'low' | 'normal' | 'high' | 'critical';
    correlationId?: string;
  }): DaemonMessage {
    return {
      id: params.id,
      from: params.from,
      to: params.to,
      type: params.type,
      data: params.data,
      timestamp: new Date(),
      priority: params.priority || 'normal',
      correlationId: params.correlationId
    };
  }

  /**
   * Create a session creation message
   */
  static createSessionMessage(params: {
    id: string;
    from: string;
    sessionType: string;
    owner: string;
    options?: any;
  }): DaemonMessage {
    return this.createMessage({
      id: params.id,
      from: params.from,
      to: 'session-manager',
      type: 'create_session',
      data: {
        type: params.sessionType,
        owner: params.owner,
        options: params.options || {}
      }
    });
  }

  /**
   * Create a browser launch message
   */
  static createBrowserMessage(params: {
    id: string;
    from: string;
    sessionId: string;
    url: string;
    config?: any;
  }): DaemonMessage {
    return this.createMessage({
      id: params.id,
      from: params.from,
      to: 'browser-manager',
      type: 'create_browser',
      data: {
        sessionId: params.sessionId,
        url: params.url,
        config: params.config || {
          purpose: 'development',
          requirements: {
            devtools: true,
            isolation: 'dedicated',
            visibility: 'visible',
            persistence: 'session'
          },
          resources: {
            priority: 'high'
          }
        }
      }
    });
  }

  /**
   * Create a directory operation message
   */
  static createDirectoryMessage(params: {
    id: string;
    from: string;
    operation: string;
    data: any;
  }): DaemonMessage {
    return this.createMessage({
      id: params.id,
      from: params.from,
      to: 'continuum-directory',
      type: params.operation,
      data: params.data
    });
  }

  /**
   * Create a command execution message
   */
  static createCommandMessage(params: {
    id: string;
    from: string;
    command: string;
    parameters?: any;
    context?: any;
  }): DaemonMessage {
    return this.createMessage({
      id: params.id,
      from: params.from,
      to: 'command-processor',
      type: 'execute_command',
      data: {
        command: params.command,
        parameters: params.parameters || {},
        context: params.context
      }
    });
  }

  /**
   * Generate unique message ID
   */
  static generateMessageId(prefix = 'msg'): string {
    const timestamp = Date.now();
    const random = Math.random().toString(36).substring(2, 8);
    return `${prefix}_${timestamp}_${random}`;
  }

  /**
   * Validate daemon message structure
   */
  static validateMessage(message: any): message is DaemonMessage {
    return (
      typeof message === 'object' &&
      typeof message.id === 'string' &&
      typeof message.from === 'string' &&
      typeof message.to === 'string' &&
      typeof message.type === 'string' &&
      message.data !== undefined &&
      message.timestamp instanceof Date
    );
  }
}