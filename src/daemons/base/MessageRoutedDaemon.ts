/**
 * MessageRoutedDaemon - Base class for daemons that route messages based on data.type
 * 
 * Handles the common pattern of:
 * 1. Receive message with specific type (e.g., 'browser_request')
 * 2. Route based on message.data.type (e.g., 'create', 'list', 'optimize')
 * 3. Call appropriate handler method
 */

import { BaseDaemon } from './BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from './DaemonProtocol.js';

export interface MessageRouteHandler {
  (data: any): Promise<DaemonResponse>;
}

export interface MessageRouteMap {
  [routeType: string]: MessageRouteHandler;
}

export abstract class MessageRoutedDaemon extends BaseDaemon {
  // Subclasses must define their primary message type and route map
  protected abstract readonly primaryMessageType: string;
  protected abstract getRouteMap(): MessageRouteMap;
  
  // Optional: subclasses can override for additional message types
  protected getAdditionalMessageHandlers(): { [messageType: string]: MessageRouteHandler } {
    return {};
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      // Handle built-in message types first
      if (message.type === 'set_session_log') {
        return this.handleSetSessionLog(message.data);
      }
      
      // Handle primary routed message type
      if (message.type === this.primaryMessageType) {
        return await this.handleRoutedMessage(message.data);
      }
      
      // Handle additional message types
      const additionalHandlers = this.getAdditionalMessageHandlers();
      const handler = additionalHandlers[message.type];
      if (handler) {
        return await handler(message.data);
      }
      
      // Unknown message type
      return {
        success: false,
        error: `Unknown message type: ${message.type}`
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Error handling message ${message.type}: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  private async handleRoutedMessage(data: any): Promise<DaemonResponse> {
    if (!data || !data.type) {
      return {
        success: false,
        error: `Missing route type in ${this.primaryMessageType} data`
      };
    }

    const routeMap = this.getRouteMap();
    const handler = routeMap[data.type];
    
    if (!handler) {
      const availableRoutes = Object.keys(routeMap).join(', ');
      return {
        success: false,
        error: `Unknown ${this.primaryMessageType} route: ${data.type}. Available: ${availableRoutes}`
      };
    }

    return await handler(data);
  }

  /**
   * Helper method for subclasses to get their supported message types
   */
  protected getSupportedMessageTypes(): string[] {
    const additional = Object.keys(this.getAdditionalMessageHandlers());
    return [this.primaryMessageType, ...additional];
  }

  /**
   * Helper method for subclasses to get their supported routes
   */
  protected getSupportedRoutes(): string[] {
    return Object.keys(this.getRouteMap());
  }
  
  /**
   * Handle built-in set_session_log message
   */
  private handleSetSessionLog(data: any): DaemonResponse {
    const { sessionId, logPath } = data;
    
    if (!logPath) {
      return {
        success: false,
        error: 'logPath is required for set_session_log'
      };
    }
    
    try {
      // Use the BaseDaemon method to set session log path
      this.setSessionLogPath(logPath);
      this.log(`📝 Session logging enabled for ${sessionId}: ${logPath}`);
      
      return {
        success: true,
        data: {
          sessionId,
          logPath,
          daemon: this.name,
          message: 'Session logging enabled'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to set session log: ${errorMessage}`
      };
    }
  }
}