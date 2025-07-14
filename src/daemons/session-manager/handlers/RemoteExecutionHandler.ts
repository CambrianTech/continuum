// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🚨 CROSS-CUTTING CONCERN: WebSocket request-response correlation across daemon boundaries
/**
 * Remote Execution Handler - Request-Response WebSocket communication
 * 
 * ✅ SINGLE RESPONSIBILITY: Handles remote execution request-response cycle
 * ✅ PROMISE-BASED: Returns Promise that resolves when browser responds
 * ✅ CORRELATION TRACKING: Matches responses to requests via correlation IDs
 */

import { MessageHandler } from '../../../integrations/websocket/types/MessageHandler';
import { DaemonResponse } from '../../base/DaemonProtocol';

export interface RemoteExecutionRequest {
  command: string;
  params: unknown;
  sessionId?: string;
  timeout?: number;
  correlationId?: string;
}

export interface RemoteExecutionResponse {
  success: boolean;
  data?: unknown;
  error?: string;
  correlationId?: string;
  clientMetadata?: {
    userAgent: string;
    timestamp: number;
    executionTime: number;
  };
}

export interface RemoteExecutionHandlerRequest {
  sessionId: string;
  message: {
    type: 'remote_execution_request';
    data: RemoteExecutionRequest;
    correlationId: string;
  };
}

export class RemoteExecutionHandler implements MessageHandler {
  public readonly priority = 100; // High priority for core functionality
  
  // Track pending requests by correlation ID
  private pendingRequests = new Map<string, {
    resolve: (response: RemoteExecutionResponse) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  }>();
  
  constructor(
    private sessionConnections: Map<string, string>, // sessionId -> connectionId
    private sendToConnection: (connectionId: string, message: unknown) => Promise<DaemonResponse>,
    _onResponse: (correlationId: string, response: RemoteExecutionResponse) => void // callback for external use
  ) {
    // Constructor parameters set up the handler, onResponse used externally
  }
  
  async handle(data: unknown): Promise<DaemonResponse> {
    try {
      const { sessionId, message } = data as RemoteExecutionHandlerRequest;
      
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
        return {
          success: false,
          error: `No WebSocket connection found for session: ${sessionId}`
        };
      }

      // Create Promise for the response
      const responsePromise = new Promise<RemoteExecutionResponse>((resolve, reject) => {
        const correlationId = message.correlationId;
        const timeout = setTimeout(() => {
          this.pendingRequests.delete(correlationId);
          reject(new Error(`Remote execution timeout after 30000ms for ${correlationId}`));
        }, 30000);
        
        this.pendingRequests.set(correlationId, { resolve, reject, timeout });
      });

      // Send the request to browser
      const sendResult = await this.sendToConnection(connectionId, message);
      
      if (!sendResult.success) {
        // Clean up pending request on send failure
        const correlationId = message.correlationId;
        const pending = this.pendingRequests.get(correlationId);
        if (pending) {
          clearTimeout(pending.timeout);
          this.pendingRequests.delete(correlationId);
        }
        return sendResult;
      }

      // Wait for browser response
      console.log(`⏳ Waiting for browser response with correlation ID: ${message.correlationId}`);
      console.log(`🔍 DEBUG: Request being sent to browser:`, JSON.stringify(message, null, 2));
      
      const response = await responsePromise;
      
      console.log(`✅ Received browser response for ${message.correlationId}`);
      console.log(`🔍 DEBUG: Browser response structure:`, JSON.stringify(response, null, 2));
      
      return {
        success: true,
        data: response
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Remote execution failed: ${errorMessage}`);
      return {
        success: false,
        error: `Remote execution failed: ${errorMessage}`
      };
    }
  }
  
  /**
   * Handle incoming response from browser
   */
  handleResponse(correlationId: string, response: RemoteExecutionResponse): void {
    const pending = this.pendingRequests.get(correlationId);
    if (pending) {
      clearTimeout(pending.timeout);
      this.pendingRequests.delete(correlationId);
      pending.resolve(response);
    } else {
      console.warn(`⚠️ Received response for unknown correlation ID: ${correlationId}`);
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