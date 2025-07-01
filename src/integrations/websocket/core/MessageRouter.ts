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
    console.log(`🔍 [ROUTE DEBUG] Incoming message from ${clientId}`);
    console.log(`🔍 [ROUTE DEBUG] Message type: ${message.type}`);
    console.log(`🔍 [ROUTE DEBUG] Message data:`, message.data);
    console.log(`🔍 [ROUTE DEBUG] Request ID: ${message.requestId}`);
    
    const messageType = message.type;
    const handler = this.handlers.get(messageType);
    
    console.log(`🔍 [ROUTE DEBUG] Looking for handler: ${messageType}`);
    console.log(`🔍 [ROUTE DEBUG] Available handlers:`, Array.from(this.handlers.keys()));

    if (!handler) {
      console.log(`❌ [ROUTE DEBUG] No handler found for: ${messageType}`);
      return {
        type: 'error',
        data: { 
          error: `Unknown message type: ${messageType}`,
          availableTypes: Array.from(this.handlers.keys())
        },
        timestamp: new Date().toISOString(),
        clientId
      };
    }

    console.log(`✅ [ROUTE DEBUG] Handler found, executing...`);
    try {
      const result = await handler(message.data, clientId, daemonConnector);
      console.log(`✅ [ROUTE DEBUG] Handler completed successfully`);
      console.log(`✅ [ROUTE DEBUG] Result:`, result);
      
      return {
        type: `${messageType}_response`,
        data: result,
        timestamp: new Date().toISOString(),
        clientId,
        requestId: message.requestId
      };
    } catch (error) {
      console.error(`❌ [ROUTE DEBUG] Handler error for ${messageType}:`, error);
      return {
        type: 'error',
        data: { 
          error: error instanceof Error ? error.message : String(error),
          messageType,
          stack: error instanceof Error ? error.stack : undefined
        },
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
      console.log(`🔍 [COMMAND DEBUG] Starting command execution`);
      console.log(`🔍 [COMMAND DEBUG] Command: ${data.command}`);
      console.log(`🔍 [COMMAND DEBUG] Params: ${data.params}`);
      console.log(`🔍 [COMMAND DEBUG] Client ID: ${clientId}`);
      console.log(`🔍 [COMMAND DEBUG] Request ID: ${data.requestId}`);
      
      // Check daemon connection with detailed logging
      const isConnected = daemonConnector.isConnected();
      console.log(`🔍 [COMMAND DEBUG] Daemon connected: ${isConnected}`);
      
      if (!isConnected) {
        console.log(`❌ [COMMAND DEBUG] FAILURE: TypeScript daemon not connected`);
        return {
          command: data.command,
          params: data.params,
          result: {
            success: false,
            error: 'TypeScript daemon not connected',
            processor: 'websocket-no-daemon',
            debug: {
              step: 'daemon_connection_check',
              daemonConnected: false,
              timestamp: new Date().toISOString()
            }
          }
        };
      }

      console.log(`🔍 [COMMAND DEBUG] Daemon connected, parsing params...`);
      const parsedParams = this.parseParams(data.params);
      console.log(`🔍 [COMMAND DEBUG] Parsed params:`, parsedParams);

      console.log(`🔍 [COMMAND DEBUG] Sending to daemon connector...`);
      const startTime = Date.now();
      
      try {
        const result = await daemonConnector.executeCommand(
          data.command,
          parsedParams,
          { clientId, requestId: data.requestId }
        );
        const duration = Date.now() - startTime;
        
        console.log(`✅ [COMMAND DEBUG] Command completed in ${duration}ms`);
        console.log(`✅ [COMMAND DEBUG] Result:`, result);

        return {
          command: data.command,
          params: data.params,
          result: {
            ...result,
            duration,
            processor: 'typescript-daemon',
            debug: {
              step: 'command_completed',
              duration,
              timestamp: new Date().toISOString()
            }
          },
          requestId: data.requestId
        };
      } catch (error) {
        const duration = Date.now() - startTime;
        const errorMessage = error instanceof Error ? error.message : String(error);
        
        console.log(`❌ [COMMAND DEBUG] Command failed after ${duration}ms`);
        console.log(`❌ [COMMAND DEBUG] Error:`, error);
        
        return {
          command: data.command,
          params: data.params,
          result: {
            success: false,
            error: errorMessage,
            duration,
            processor: 'typescript-daemon-error',
            debug: {
              step: 'command_error',
              duration,
              error: errorMessage,
              stack: error instanceof Error ? error.stack : undefined,
              timestamp: new Date().toISOString()
            }
          },
          requestId: data.requestId
        };
      }
    });

    // Client initialization handler with version checking
    this.registerHandler('client_init', async (data: any, clientId: string, daemonConnector: DaemonConnector) => {
      const clientVersion = data.version || 'unknown';
      
      // Import package.json to get server version
      const fs = await import('fs');
      const path = await import('path');
      let serverVersion = 'unknown';
      
      try {
        const packagePath = path.join(process.cwd(), 'package.json');
        const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
        serverVersion = packageData.version;
      } catch (error) {
        console.warn('Could not read server version from package.json');
      }
      
      console.log(`🔌 Client v${clientVersion} connecting (Server: v${serverVersion})`);
      
      // Check for version mismatch
      if (clientVersion !== 'unknown' && serverVersion !== 'unknown' && clientVersion !== serverVersion) {
        console.log(`⚠️ Version mismatch detected: Client v${clientVersion} vs Server v${serverVersion}`);
        
        // Send version mismatch message to trigger reload
        setTimeout(() => {
          this.emit('send_to_client', clientId, {
            type: 'version_mismatch',
            data: { serverVersion, clientVersion },
            timestamp: new Date().toISOString()
          });
        }, 100);
      }
      
      return {
        server: 'websocket-daemon',
        serverVersion,
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