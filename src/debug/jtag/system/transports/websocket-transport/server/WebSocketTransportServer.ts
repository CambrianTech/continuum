/**
 * WebSocket Transport Server - Server-specific WebSocket implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Server role: server transport that accepts client connections.
 */

import { WebSocketTransportBase, type WebSocketConfig } from '../shared/WebSocketTransportBase';
import type { JTAGMessage } from '@shared/JTAGTypes';
import type { WebSocketServer, WebSocket as WSWebSocket } from 'ws';
import type { TransportSendResult } from '../../shared/TransportTypes';

// Server-specific WebSocket configuration with typed inheritance
export interface WebSocketServerConfig extends WebSocketConfig {
  port: number;
}

export class WebSocketTransportServer extends WebSocketTransportBase {
  public readonly name = 'websocket-server';
  
  private server?: WebSocketServer;
  private clients = new Set<WSWebSocket>();
  private messageHandlers = new Set<(message: JTAGMessage) => void>();
  private sessionHandshakeHandler?: (sessionId: string) => void;

  constructor(config: WebSocketServerConfig) {
    super(config);
  }

  /**
   * Set handler for session handshake messages
   */
  setSessionHandshakeHandler(handler: (sessionId: string) => void): void {
    this.sessionHandshakeHandler = handler;
  }

  async start(port: number): Promise<void> {
    console.log(`🔗 WebSocket Server: Starting on port ${port}`);
    
    try {
      // Dynamic import to handle WebSocket availability
      const WebSocketModule = await eval('import("ws")');
      const WSServer = WebSocketModule.WebSocketServer ?? WebSocketModule.default?.WebSocketServer;
      
      // Create server with proper error handling for port conflicts
      const server = new WSServer({ port });
      
      // Wait for server to successfully start or fail
      await new Promise<void>((resolve, reject) => {
        server.once('error', (error: Error) => {
          if (error.message.includes('EADDRINUSE')) {
            reject(new Error(`WebSocket server port ${port} already in use - another JTAG server may be running`));
          } else {
            reject(error);
          }
        });
        
        server.once('listening', () => {
          resolve();
        });
      });
      
      this.server = server;
      this.connected = true;
      
      this.server!.on('connection', (ws: WSWebSocket) => {
        console.log(`🔗 ${this.name}: New client connected`);
        
        this.clients.add(ws);
        const clientId = this.generateClientId('ws');
        
        // Emit CONNECTED event using shared method
        this.emitTransportEvent('CONNECTED', {
          clientId,
          remoteAddress: (ws as any)._socket?.remoteAddress
        });
        
        ws.on('message', (data) => {
          try {
            const message = this.parseWebSocketMessage(data);
            
            // Handle session handshake messages
            if (this.isSessionHandshake(message)) {
              this.handleSessionHandshake(message);
              if (this.sessionHandshakeHandler) {
                this.sessionHandshakeHandler(message.sessionId);
              }
              return; // Don't forward handshake messages to regular handlers
            }
            
            console.log(`📨 ${this.name}: Received message from client`);
            
            // Forward to registered message handlers (router handles all processing)
            for (const handler of this.messageHandlers) {
              handler(message);
            }
            
            // Base class handler not needed - messageHandlers include router which does all processing
          } catch (error) {
            this.handleWebSocketError(error as Error, 'message processing');
          }
        });
        
        ws.on('close', () => {
          console.log(`🔌 ${this.name}: Client disconnected`);
          this.clients.delete(ws);
          
          // Emit DISCONNECTED event using shared method
          this.emitTransportEvent('DISCONNECTED', {
            clientId,
            reason: 'client_disconnect'
          });
        });
        
        ws.on('error', (error: Error) => {
          this.clients.delete(ws);
          this.handleWebSocketError(error, 'client connection');
        });
      });
      
      console.log(`✅ WebSocket Server: Listening on port ${port}`);
    } catch (error) {
      console.error(`❌ WebSocket Server: Failed to start:`, error);
      this.connected = false;
      throw error;
    }
  }

  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandlers.add(handler);
    super.setMessageHandler(handler);
  }

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    console.log(`📤 WebSocket Server: Broadcasting message to ${this.clients.size} clients`);
    
    const messageData = JSON.stringify(message);
    let sentCount = 0;
    
    for (const client of this.clients) {
      try {
        if (client.readyState === 1) { // WebSocket.OPEN
          client.send(messageData);
          sentCount++;
        }
      } catch (error) {
        console.error(`❌ WebSocket Server: Failed to send to client:`, error);
        this.clients.delete(client);
      }
    }
    
    console.log(`📤 WebSocket Server: Message sent to ${sentCount} clients`);
    return this.createResult(true, sentCount);
  }

  async disconnect(): Promise<void> {
    console.log(`🔌 WebSocket Server: Shutting down`);
    
    if (this.server) {
      // Close all client connections
      for (const client of this.clients) {
        try {
          client.close();
        } catch (error) {
          console.error(`❌ WebSocket Server: Error closing client:`, error);
        }
      }
      
      // Close server
      this.server.close();
      this.server = undefined;
      this.connected = false;
    }
    
    this.clients.clear();
    this.messageHandlers.clear();
  }

  async reconnect(): Promise<void> {
    throw new Error('WebSocket server transport does not support reconnection - clients should reconnect to server');
  }
}