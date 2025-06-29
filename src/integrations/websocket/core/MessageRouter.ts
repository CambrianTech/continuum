/**
 * Message Router - Routes WebSocket messages to appropriate handlers
 */

import { EventEmitter } from 'events';
import { WebSocketMessage, CommandRequest } from '../types';
import { DaemonConnector } from './DaemonConnector';

export class MessageRouter extends EventEmitter {
  private handlers = new Map<string, MessageHandler>();

  constructor() {
    super();
    this.setupDefaultHandlers();
  }

  async routeMessage(
    message: any,
    clientId: string,
    daemonConnector: DaemonConnector
  ): Promise<WebSocketMessage | null> {
    const messageType = message.type;
    const handler = this.handlers.get(messageType);

    if (!handler) {
      console.log(`🔄 Unknown message type: ${messageType}`);
      return {
        type: 'error',
        data: { error: `Unknown message type: ${messageType}` },
        timestamp: new Date().toISOString(),
        clientId
      };
    }

    try {
      const result = await handler(message.data, clientId, daemonConnector);
      return {
        type: `${messageType}_response`,
        data: result,
        timestamp: new Date().toISOString(),
        clientId,
        requestId: message.requestId
      };
    } catch (error) {
      console.error(`❌ Handler error for ${messageType}:`, error);
      return {
        type: 'error',
        data: { error: error instanceof Error ? error.message : String(error) },
        timestamp: new Date().toISOString(),
        clientId,
        requestId: message.requestId
      };
    }
  }

  registerHandler(messageType: string, handler: MessageHandler): void {
    this.handlers.set(messageType, handler);
    console.log(`📝 Registered handler for: ${messageType}`);
  }

  unregisterHandler(messageType: string): boolean {
    return this.handlers.delete(messageType);
  }

  getRegisteredHandlers(): string[] {
    return Array.from(this.handlers.keys());
  }

  private setupDefaultHandlers(): void {
    // Daemon stats handler
    this.registerHandler('get_stats', async (_data: any, clientId: string, daemonConnector: DaemonConnector) => {
      console.log(`📊 Getting daemon stats for client: ${clientId}`);
      return {
        server: 'websocket-server',
        version: '1.0.0',
        uptime: process.uptime(),
        memory: process.memoryUsage(),
        connections: 1, // Current client
        daemon_connected: daemonConnector.isConnected()
      };
    });

    // Client list handler  
    this.registerHandler('get_clients', async (_data: any, clientId: string, _daemonConnector: DaemonConnector) => {
      console.log(`👥 Getting client list for: ${clientId}`);
      return {
        clients: [{ id: clientId, connected: true, timestamp: new Date().toISOString() }],
        total: 1
      };
    });

    // Command execution handler
    this.registerHandler('execute_command', async (data: CommandRequest, clientId: string, daemonConnector: DaemonConnector) => {
      console.log(`⚡ Executing command: ${data.command}`);
      
      if (!daemonConnector.isConnected()) {
        return {
          command: data.command,
          params: data.params,
          result: {
            success: false,
            error: 'TypeScript daemon not connected',
            processor: 'websocket-no-daemon'
          }
        };
      }

      const startTime = Date.now();
      const result = await daemonConnector.executeCommand(
        data.command,
        this.parseParams(data.params),
        { clientId, requestId: data.requestId }
      );
      const duration = Date.now() - startTime;

      return {
        command: data.command,
        params: data.params,
        result: {
          ...result,
          duration,
          processor: 'typescript-daemon'
        },
        requestId: data.requestId
      };
    });

    // Client initialization handler
    this.registerHandler('client_init', async (_data: any, _clientId: string, daemonConnector: DaemonConnector) => {
      return {
        server: 'websocket-daemon',
        daemon: daemonConnector.isConnected() ? 'connected' : 'disconnected',
        capabilities: [
          'command-execution',
          'event-streaming',
          'real-time-updates'
        ],
        availableCommands: daemonConnector.getAvailableCommands()
      };
    });

    // Event subscription handler
    this.registerHandler('subscribe_events', async (data: { events: string[] }, clientId: string) => {
      // TODO: Implement event subscription
      return {
        subscribed: data.events,
        clientId
      };
    });

    // Ping/pong handler
    this.registerHandler('ping', async (_data: any, clientId: string) => {
      return {
        pong: true,
        timestamp: new Date().toISOString(),
        clientId
      };
    });

    // Chat message handler - routes to chat command with room_id
    this.registerHandler('message', async (data: any, clientId: string, daemonConnector: DaemonConnector) => {
      console.log(`💬 Chat message from ${clientId} to room: ${data.room || 'general'}`);
      
      // Route to chat command - let it handle the room routing
      if (daemonConnector.isConnected()) {
        try {
          const result = await daemonConnector.executeCommand(
            'chat',
            { 
              message: data.content || data.message || data,
              room: data.room || 'general',
              history: data.history || []
            },
            { clientId }
          );
          return result;
        } catch (error) {
          console.error('❌ Error routing to chat command:', error);
          return {
            success: false,
            error: `Chat command failed: ${error instanceof Error ? error.message : String(error)}`
          };
        }
      }
      
      // No daemon connected - still echo for debugging
      return {
        success: false,
        error: 'Chat daemon not connected',
        echo: {
          message: data.content || data.message || data,
          room: data.room || 'general',
          clientId
        }
      };
    });
  }

  private parseParams(params: string): any {
    try {
      return JSON.parse(params);
    } catch {
      return {};
    }
  }
}

type MessageHandler = (
  data: any,
  clientId: string,
  daemonConnector: DaemonConnector
) => Promise<any>;