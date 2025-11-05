// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Send To Session Handler - Session-specific message routing
 * 
 * ✅ SINGLE RESPONSIBILITY: Only handles session-to-connection routing
 * ✅ PROPER OWNERSHIP: Session logic belongs in SessionManagerDaemon
 * ✅ CLEAN INTERFACE: Implements MessageHandler for registration
 */

import { MessageHandler } from '../../../integrations/websocket/types/MessageHandler';
import { DaemonResponse } from '../../base/DaemonProtocol';

export interface SendToSessionRequest {
  sessionId: string;
  message: unknown;
}

export class SendToSessionHandler implements MessageHandler {
  public readonly priority = 100; // High priority for core functionality
  
  constructor(
    private sessionConnections: Map<string, string>, // sessionId -> connectionId
    private sendToConnection: (connectionId: string, message: unknown) => Promise<DaemonResponse>
  ) {}
  
  async handle(data: unknown): Promise<DaemonResponse> {
    try {
      const { sessionId, message } = data as SendToSessionRequest;
      
      if (!sessionId || !message) {
        return {
          success: false,
          error: 'sessionId and message are required'
        };
      }

      // Find connectionId for this sessionId
      const connectionId = this.findConnectionBySession(sessionId);
      
      if (!connectionId) {
        console.log(`❌ No connection found for session: ${sessionId}`);
        console.log(`🔍 Available mappings: ${Array.from(this.sessionConnections.entries()).map(([s, c]) => `${s}→${c}`).join(', ')}`);
        return {
          success: false,
          error: `No WebSocket connection found for session: ${sessionId}`
        };
      }

      // Delegate to connection sending
      return await this.sendToConnection(connectionId, message);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to send to session: ${errorMessage}`
      };
    }
  }
  
  /**
   * Find connection ID by session ID
   */
  private findConnectionBySession(sessionId: string): string | null {
    for (const [mappedSessionId, connectionId] of this.sessionConnections.entries()) {
      if (mappedSessionId === sessionId) {
        return connectionId;
      }
    }
    return null;
  }
}