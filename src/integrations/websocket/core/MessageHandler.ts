/**
 * MessageHandler - Handles different types of WebSocket messages
 * Extracted from WebSocketDaemon to follow single responsibility principle
 */

import { EventEmitter } from 'events';
import { WebSocketManager } from './WebSocketManager';
import { DaemonRouter } from './DaemonRouter';
import { CommandRouter } from './CommandRouter';

export class MessageHandler extends EventEmitter {
  private commandRouter: CommandRouter;
  
  constructor(
    private wsManager: WebSocketManager,
    private daemonRouter: DaemonRouter
  ) {
    super();
    this.commandRouter = new CommandRouter(wsManager, daemonRouter);
    
    // Forward logs from command router
    this.commandRouter.on('log', (log) => this.emit('log', log));
  }
  
  /**
   * Handle incoming WebSocket message
   */
  async handleMessage(connectionId: string, data: any): Promise<void> {
    try {
      const message = JSON.parse(data.toString());
      
      this.emit('log', {
        level: 'debug',
        message: `WebSocket message received: type=${message.type}, connectionId=${connectionId}`
      });
      
      // Route based on message type or structure
      if (message.type === 'execute_command') {
        await this.commandRouter.routeCommand(connectionId, message);
      } else if (message.type === 'register_http_routes') {
        await this.handleRouteRegistration(connectionId, message);
      } else if (message.to) {
        await this.routeMessageToDaemon(connectionId, message);
      } else {
        this.emit('log', {
          level: 'warn',
          message: `Unknown WebSocket message type: ${message.type}`
        });
      }
    } catch (error) {
      this.emit('log', {
        level: 'error',
        message: `WebSocket message parse error: ${error}`
      });
    }
  }
  
  /**
   * Route a message to a specific daemon based on the 'to' field
   */
  private async routeMessageToDaemon(connectionId: string, message: any): Promise<void> {
    try {
      const response = await this.daemonRouter.routeMessage(message);
      
      // Send response back to WebSocket client
      const wsResponse = {
        id: message.id,
        from: message.to,
        to: message.from,
        type: response.success ? 'response' : 'error',
        success: response.success,
        data: response.data,
        error: response.error,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, wsResponse);
      
      this.emit('log', {
        level: 'info',
        message: `Message routed to ${message.to}: ${response.success ? 'success' : 'failed'}`
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      this.emit('log', {
        level: 'error',
        message: `Error routing message to daemon: ${errorMessage}`
      });
      
      // Send error response to client
      const errorResponse = {
        id: message.id,
        from: message.to,
        to: message.from,
        type: 'error',
        success: false,
        error: errorMessage,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, errorResponse);
    }
  }
  
  /**
   * Handle route registration from daemons
   */
  private async handleRouteRegistration(connectionId: string, _message: any): Promise<void> {
    // This would typically update the HTTP route table
    // For now, just acknowledge the registration
    
    const response = {
      type: 'route_registration_response',
      success: true,
      message: 'Routes registered successfully'
    };
    
    this.wsManager.sendToConnection(connectionId, response);
    
    this.emit('log', {
      level: 'info',
      message: `Route registration acknowledged for connection ${connectionId}`
    });
  }
}