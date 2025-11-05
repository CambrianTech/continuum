/**
 * CommandRouter - Handles routing commands from WebSocket to CommandProcessor
 * Extracted from WebSocketDaemon to follow single responsibility principle
 */

import { EventEmitter } from 'events';
import { WebSocketManager } from './WebSocketManager';
import { DaemonRouter } from './DaemonRouter';

export class CommandRouter extends EventEmitter {
  constructor(
    private wsManager: WebSocketManager,
    private daemonRouter: DaemonRouter
  ) {
    super();
  }
  
  /**
   * Route a command from WebSocket to the command processor daemon
   */
  async routeCommand(connectionId: string, message: any): Promise<void> {
    try {
      // Extract command info from the WebSocket message structure
      const commandData = message.data;
      const commandName = commandData.command;
      const commandParams = commandData.params;
      const requestId = commandData.requestId;
      
      // Parse parameters (they might be JSON string from browser)
      let parsedParams = {};
      if (commandParams) {
        try {
          parsedParams = typeof commandParams === 'string' ? JSON.parse(commandParams) : commandParams;
        } catch (error) {
          this.emit('log', {
            level: 'warn',
            message: `Failed to parse command parameters: ${error}`
          });
          parsedParams = commandParams;
        }
      }
      
      // Convert WebSocket message to DaemonMessage format
      const daemonMessage = {
        id: `ws-${Date.now()}`,
        from: 'websocket-server',
        to: 'command-processor',
        type: 'command.execute',
        timestamp: new Date(),
        data: {
          command: commandName,
          parameters: parsedParams,
          context: {
            connectionId: connectionId,
            websocket: true,
            requestId: requestId
          }
        }
      };
      
      this.emit('log', {
        level: 'info',
        message: `Routing command "${commandName}" to command processor`
      });
      
      // Route to command processor
      const response = await this.daemonRouter.routeMessage(daemonMessage);
      
      // Send response back to WebSocket client
      const wsResponse = {
        type: 'command_response',
        requestId: requestId,
        command: commandName,
        success: response.success,
        data: response.data,
        error: response.error,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, wsResponse);
      
      this.emit('log', {
        level: 'info',
        message: `Command "${commandName}" ${response.success ? 'completed' : 'failed'}`
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      this.emit('log', {
        level: 'error',
        message: `Error routing command: ${errorMessage}`
      });
      
      // Send error response to client
      const errorResponse = {
        type: 'command_response',
        requestId: message.data?.requestId,
        command: message.data?.command || 'unknown',
        success: false,
        error: errorMessage,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, errorResponse);
    }
  }
}