/**
 * WebSocket Transport Server - Server-specific WebSocket implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Server role: server transport that accepts client connections.
 */

import { WebSocketTransportClient, type WebSocketConfig } from '../shared/WebSocketTransportClient';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import { JTAGMessageTypes } from '../../../core/types/JTAGTypes';
import { WebSocketServer, type WebSocket as WSWebSocket } from 'ws';
import type { TransportSendResult } from '../../shared/TransportTypes';
import type { ITransportAdapter } from '../../shared/TransportBase';
import { WebSocketResponseRouter } from './WebSocketResponseRouter';

// Server-specific WebSocket configuration with typed inheritance
export interface WebSocketServerConfig extends WebSocketConfig {
  port: number;
}

export class WebSocketTransportServer extends WebSocketTransportClient implements ITransportAdapter {
  public readonly name = 'websocket-server';
  public readonly protocol = 'websocket';
  public readonly supportedRoles = ['server'];
  public readonly supportedEnvironments = ['server'];
  
  private server?: WebSocketServer;
  private clients = new Set<WSWebSocket>();
  private messageHandlers = new Set<(message: JTAGMessage) => void>();
  private sessionHandshakeHandler?: (sessionId: string) => void;
  private serverConfig: WebSocketServerConfig;
  private responseRouter = new WebSocketResponseRouter();

  constructor(config: WebSocketServerConfig) {
    super(config);
    this.serverConfig = config;
  }

  /**
   * Server-specific WebSocket creation - not used for server
   */
  protected createWebSocket(url: string): never {
    throw new Error('Server does not create WebSocket clients - use createWebSocketServer instead');
  }

  /**
   * Server-specific WebSocket server creation
   */
  protected async createWebSocketServer(port: number): Promise<WebSocketServer> {
    return new WebSocketServer({ port });
  }

  /**
   * Set handler for session handshake messages
   */
  setSessionHandshakeHandler(handler: (sessionId: string) => void): void {
    this.sessionHandshakeHandler = handler;
  }

  /**
   * ITransportAdapter interface compliance - delegates to start()
   * @param url - ignored for server (uses port from config)
   */
  async connect(url?: string): Promise<void> {
    // Use typed config port
    return this.start(this.serverConfig.port);
  }

  async start(port: number): Promise<void> {
    console.log(`🔗 WebSocket Server: Starting on port ${port}`);
    
    try {
      // Create server using clean abstracted method
      const server = await this.createWebSocketServer(port);
      
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
      
      this.server.on('connection', (ws: WSWebSocket) => {
        console.log(`🔗 ${this.name}: New client connected`);
        
        this.clients.add(ws);
        const clientId = this.generateClientId('ws');
        
        // Register client with response router
        this.responseRouter.registerClient(ws, clientId);
        
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
            
            // Register correlation for request messages so we can route responses back
            if (JTAGMessageTypes.isRequest(message)) {
              const correlationId = (message as any).correlationId;
              if (correlationId) {
                this.responseRouter.registerCorrelation(correlationId, clientId);
                console.log(`🔗 WebSocketTransportServer: Registered correlation ${correlationId} for client ${clientId}`);
              }
            }
            
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
          
          // Unregister client from response router
          this.responseRouter.unregisterClient(clientId);
          
          // Emit DISCONNECTED event using shared method
          this.emitTransportEvent('DISCONNECTED', {
            clientId,
            reason: 'client_disconnect'
          });
        });
        
        ws.on('error', (error: Error) => {
          this.clients.delete(ws);
          
          // Unregister client from response router
          this.responseRouter.unregisterClient(clientId);
          
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
    // Check if this is a response that should be routed to a specific client
    if (JTAGMessageTypes.isResponse(message)) {
      const correlationId = message.correlationId;
      
      // First check: Does this correlation exist in our router?
      if (correlationId && this.responseRouter.hasCorrelation(correlationId)) {
        console.log(`🎯 WebSocket Server: Routing response ${correlationId} to specific client`);
        const success = await this.responseRouter.sendResponse(message);
        return this.createResult(success, success ? 1 : 0);
      }
      
      // Second check: Is this a server client response that we should try to route?
      // Server client responses have endpoint "server" but should be sent via WebSocket
      if (correlationId?.startsWith('client_')) {
        console.log(`🔍 WebSocket Server: Attempting to route server client response ${correlationId}`);
        // Try to send anyway in case correlation was briefly removed
        const success = await this.responseRouter.sendResponse(message);
        if (success) {
          console.log(`✅ WebSocket Server: Successfully sent late response ${correlationId}`);
          return this.createResult(true, 1);
        } else {
          console.log(`⚠️ WebSocket Server: Failed to send response ${correlationId} - client may have disconnected`);
        }
      }
    }
    
    // Fallback to broadcast for non-responses or responses without correlation
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