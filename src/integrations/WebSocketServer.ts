/**
 * WebSocket Server - TypeScript Implementation
 * Connects CJS rendering system to TypeScript daemon ecosystem
 */

import WebSocket from 'ws';
import { EventEmitter } from 'events';

export interface WebSocketMessage {
  type: string;
  data: any;
  timestamp: string;
  clientId?: string;
}

export interface CommandRequest {
  command: string;
  params: string;
  encoding?: string;
  requestId?: string;
}

export interface CommandResult {
  success: boolean;
  message?: string;
  data?: any;
  error?: string;
  processor?: string;
}

export class TypeScriptWebSocketServer extends EventEmitter {
  private server: WebSocket.Server | null = null;
  private clients = new Map<string, WebSocket>();
  private commandDaemon: any = null;
  private port: number;

  constructor(port: number = 9000) {
    super();
    this.port = port;
  }

  async start(): Promise<void> {
    console.log(`🚀 Starting TypeScript WebSocket Server on port ${this.port}`);
    
    this.server = new WebSocket.Server({ port: this.port });
    
    this.server.on('connection', (ws, req) => {
      const clientId = this.generateClientId();
      this.clients.set(clientId, ws);
      
      console.log(`📱 Client connected: ${clientId}`);
      
      ws.on('message', async (message) => {
        try {
          const data = JSON.parse(message.toString());
          await this.handleMessage(clientId, data);
        } catch (error) {
          console.error('❌ WebSocket message error:', error);
          this.sendToClient(clientId, {
            type: 'error',
            data: { error: 'Invalid message format' },
            timestamp: new Date().toISOString()
          });
        }
      });
      
      ws.on('close', () => {
        console.log(`📱 Client disconnected: ${clientId}`);
        this.clients.delete(clientId);
      });
      
      // Send connection confirmation
      this.sendToClient(clientId, {
        type: 'client_connection_confirmed',
        data: { clientId, server: 'typescript-websocket' },
        timestamp: new Date().toISOString()
      });
    });

    // Connect to TypeScript command daemon
    await this.connectToCommandDaemon();
    
    console.log(`✅ TypeScript WebSocket Server started on port ${this.port}`);
  }

  private async connectToCommandDaemon(): Promise<void> {
    try {
      // Direct import of TypeScript selftest command
      const { SelfTestCommand } = await import('../commands/development/selftest/SelfTestCommand');
      
      // Create a simple command executor that routes directly to TypeScript commands
      this.commandDaemon = {
        initialized: true,
        executeCommand: async (commandName: string, params: any, context: any) => {
          console.log(`🚀 Routing ${commandName} to TypeScript command`);
          
          if (commandName === 'selftest') {
            const result = await SelfTestCommand.execute(params, context);
            return {
              ...result,
              processor: 'typescript-daemon'
            };
          }
          
          return {
            success: false,
            error: `TypeScript command ${commandName} not found`,
            processor: 'typescript-daemon'
          };
        }
      };
      
      console.log('✅ Connected to TypeScript command daemon');
    } catch (error) {
      console.error('❌ Failed to connect to command daemon:', error.message);
    }
  }

  private async handleMessage(clientId: string, message: any): Promise<void> {
    console.log(`📨 Message from ${clientId}:`, message);

    switch (message.type) {
      case 'execute_command':
        await this.handleCommandExecution(clientId, message.data);
        break;
        
      case 'client_init':
        await this.handleClientInit(clientId, message.data);
        break;
        
      default:
        console.log(`🔄 Forwarding unknown message type: ${message.type}`);
        this.emit('message', { clientId, message });
    }
  }

  private async handleCommandExecution(clientId: string, commandData: CommandRequest): Promise<void> {
    console.log(`⚡ Executing command via TypeScript daemon: ${commandData.command}`);
    
    try {
      let result: CommandResult;
      
      if (this.commandDaemon) {
        // Use TypeScript command daemon
        const params = this.parseParams(commandData.params);
        const context = { clientId, processor: 'typescript-daemon' };
        
        result = await this.commandDaemon.executeCommand(commandData.command, params, context);
        console.log(`✅ TypeScript daemon result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
      } else {
        // No daemon available
        result = {
          success: false,
          error: 'TypeScript command daemon not available',
          processor: 'websocket-fallback'
        };
      }

      // Send result back to client
      this.sendToClient(clientId, {
        type: 'command_result',
        data: {
          command: commandData.command,
          params: commandData.params,
          result,
          requestId: commandData.requestId
        },
        timestamp: new Date().toISOString()
      });

    } catch (error) {
      console.error(`❌ Command execution error:`, error);
      
      this.sendToClient(clientId, {
        type: 'command_result',
        data: {
          command: commandData.command,
          params: commandData.params,
          result: {
            success: false,
            error: error.message,
            processor: 'websocket-error'
          },
          requestId: commandData.requestId
        },
        timestamp: new Date().toISOString()
      });
    }
  }

  private async handleClientInit(clientId: string, initData: any): Promise<void> {
    console.log(`🔧 Client initialization: ${clientId}`);
    
    this.sendToClient(clientId, {
      type: 'init_response',
      data: {
        server: 'typescript-websocket',
        daemon: this.commandDaemon ? 'connected' : 'disconnected',
        capabilities: ['command-execution', 'event-streaming']
      },
      timestamp: new Date().toISOString()
    });
  }

  private parseParams(params: string): any {
    try {
      return JSON.parse(params);
    } catch {
      return {};
    }
  }

  private sendToClient(clientId: string, message: WebSocketMessage): void {
    const client = this.clients.get(clientId);
    if (client && client.readyState === WebSocket.OPEN) {
      client.send(JSON.stringify(message));
    }
  }

  public broadcast(message: WebSocketMessage): void {
    const messageStr = JSON.stringify(message);
    this.clients.forEach((client) => {
      if (client.readyState === WebSocket.OPEN) {
        client.send(messageStr);
      }
    });
  }

  private generateClientId(): string {
    return `client_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  async stop(): Promise<void> {
    if (this.server) {
      this.server.close();
      this.clients.clear();
      console.log('🛑 TypeScript WebSocket Server stopped');
    }
  }
}

export default TypeScriptWebSocketServer;