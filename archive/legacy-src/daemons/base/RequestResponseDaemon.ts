/**
 * RequestResponseDaemon - Base class for daemons with simple message → handler mapping
 * 
 * Handles the common pattern of:
 * 1. Receive message with specific type
 * 2. Call corresponding handler method directly
 * 3. Return response
 */

import { BaseDaemon } from './BaseDaemon';
import type { DaemonMessage, DaemonResponse } from './DaemonProtocol';

export interface RequestHandler<T = unknown> {
  (data: T): Promise<DaemonResponse>;
}

export interface RequestHandlerMap {
  [messageType: string]: RequestHandler<unknown>;
}

export abstract class RequestResponseDaemon extends BaseDaemon {
  // Subclasses must define their message handlers
  protected abstract getRequestHandlers(): RequestHandlerMap;

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      const handlers = this.getRequestHandlers();
      const handler = handlers[message.type];
      
      if (!handler) {
        const availableTypes = Object.keys(handlers).join(', ');
        return {
          success: false,
          error: `Unknown message type: ${message.type}. Available: ${availableTypes}`
        };
      }

      return await handler(message.data);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Error handling message ${message.type}: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Helper method for subclasses to get their supported message types
   */
  protected getSupportedMessageTypes(): string[] {
    return Object.keys(this.getRequestHandlers());
  }

  /**
   * Helper method to create a standard success response
   */
  protected createSuccessResponse(data?: unknown): DaemonResponse {
    return {
      success: true,
      data
    };
  }

  /**
   * Helper method to create a standard error response
   */
  protected createErrorResponse(error: string): DaemonResponse {
    return {
      success: false,
      error
    };
  }
}