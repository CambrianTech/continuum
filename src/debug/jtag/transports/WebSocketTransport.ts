/**
 * WebSocket Transport Implementation
 * 
 * Handles WebSocket communication between browser and server contexts.
 */

import { JTAGTransport } from '../shared/JTAGRouter';
import { JTAGMessage } from '../shared/JTAGTypes';
import { WebSocketServer, WebSocket as WSWebSocket } from 'ws';
import type { EventsInterface } from '../shared/JTAGRouter';
import { TRANSPORT_EVENTS } from './TransportEvents';

interface WebSocketSendResult {
  success: boolean;
  timestamp: string;
  sentCount?: number; // Only for server broadcasts
}

export class WebSocketServerTransport implements JTAGTransport {
  name = 'websocket-server';
  private server?: WebSocketServer;
  private clients = new Set<WSWebSocket>();
  private messageHandlers = new Set<(message: JTAGMessage) => void>();
  private eventSystem?: EventsInterface;
  private sessionHandshakeHandler?: (sessionId: string) => void;

  setEventSystem(eventSystem: EventsInterface): void {
    this.eventSystem = eventSystem;
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
      const WSServer = WebSocketModule.WebSocketServer || WebSocketModule.default?.WebSocketServer;
      
      this.server = new WSServer({ port });
      
      this.server!.on('connection', (ws: WSWebSocket) => {
        console.log(`🔗 WebSocket Server: New client connected`);
        
        this.clients.add(ws);
        
        // Emit CONNECTED event
        if (this.eventSystem) {
          this.eventSystem.emit(TRANSPORT_EVENTS.CONNECTED, {
            transportType: 'websocket' as const,
            clientId: `ws_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
            remoteAddress: (ws as any)._socket?.remoteAddress
          });
        }
        
        ws.on('message', (data) => {
          try {
            const messageStr = data instanceof Buffer ? data.toString() : data.toString();
            const message = JSON.parse(messageStr);
            
            // Handle session handshake messages
            if (message.type === 'session_handshake' && message.sessionId) {
              console.log(`🤝 WebSocket Server: Received session handshake with sessionId: ${message.sessionId}`);
              if (this.sessionHandshakeHandler) {
                this.sessionHandshakeHandler(message.sessionId);
              }
              return; // Don't forward handshake messages to regular handlers
            }
            
            console.log(`📨 WebSocket Server: Received message from client`);
            
            // Forward to registered message handlers
            for (const handler of this.messageHandlers) {
              handler(message);
            }
          } catch (error) {
            console.error(`❌ WebSocket Server: Message parse error:`, error);
          }
        });
        
        ws.on('close', () => {
          console.log(`🔌 WebSocket Server: Client disconnected`);
          this.clients.delete(ws);
          
          // Emit DISCONNECTED event
          if (this.eventSystem) {
            this.eventSystem.emit(TRANSPORT_EVENTS.DISCONNECTED, {
              transportType: 'websocket' as const,
              clientId: `ws_${Date.now()}`,
              reason: 'client_disconnect'
            });
          }
        });
        
        ws.on('error', (error: Error) => {
          console.error(`❌ WebSocket Server: Client error:`, error);
          this.clients.delete(ws);
        });
      });
      
      console.log(`✅ WebSocket Server: Listening on port ${port}`);
    } catch (error) {
      console.error(`❌ WebSocket Server: Failed to start:`, error);
      throw error;
    }
  }

  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandlers.add(handler);
  }

  async send(message: JTAGMessage): Promise<WebSocketSendResult> {
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
    return { success: true, timestamp: new Date().toISOString(), sentCount };
  }

  isConnected(): boolean {
    return this.server !== undefined;
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
    }
    
    this.clients.clear();
    this.messageHandlers.clear();
  }
}

export class WebSocketClientTransport implements JTAGTransport {
  name = 'websocket-client';
  private socket?: WebSocket;
  private messageHandler?: (message: JTAGMessage) => void;
  private eventSystem?: EventsInterface;
  private lastConnectedUrl?: string;
  private sessionId?: string;

  setEventSystem(eventSystem: EventsInterface): void {
    this.eventSystem = eventSystem;
  }

  /**
   * Set the session ID that will be sent during handshake
   */
  setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
  }

  /**
   * Send session handshake to server
   */
  private sendSessionHandshake(): void {
    if (this.socket && this.sessionId) {
      const handshake = {
        type: 'session_handshake',
        sessionId: this.sessionId,
        timestamp: new Date().toISOString()
      };
      
      this.socket.send(JSON.stringify(handshake));
      console.log(`🤝 WebSocket Client: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  async connect(url: string): Promise<void> {
    this.lastConnectedUrl = url; // Store for reconnection
    console.log(`🔗 WebSocket Client: Connecting to ${url}`);
    
    return new Promise((resolve, reject) => {
      if (typeof WebSocket === 'undefined') {
        reject(new Error('WebSocket not available in this environment'));
        return;
      }

      this.socket = new WebSocket(url);
      
      this.socket.onopen = () => {
        console.log(`✅ WebSocket Client: Connected to ${url}`);
        
        // Send session handshake immediately after connection
        this.sendSessionHandshake();
        
        // Emit CONNECTED event
        if (this.eventSystem) {
          this.eventSystem.emit(TRANSPORT_EVENTS.CONNECTED, {
            transportType: 'websocket' as const,
            clientId: `ws_client_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`
          });
        }
        
        resolve();
      };
      
      this.socket.onerror = (error) => {
        console.error(`❌ WebSocket Client: Connection error:`, error);
        reject(error);
      };
      
      this.socket.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data);
          if (this.messageHandler) {
            this.messageHandler(message);
          }
        } catch (error) {
          console.error(`❌ WebSocket Client: Message parse error:`, error);
        }
      };
      
      this.socket.onclose = () => {
        console.log(`🔌 WebSocket Client: Connection closed`);
        
        // Emit DISCONNECTED event
        if (this.eventSystem) {
          this.eventSystem.emit(TRANSPORT_EVENTS.DISCONNECTED, {
            transportType: 'websocket' as const,
            clientId: `ws_client_${Date.now()}`,
            reason: 'connection_closed'
          });
        }
        
        this.socket = undefined;
      };
    });
  }

  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandler = handler;
  }

  async send(message: JTAGMessage): Promise<WebSocketSendResult> {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }
    
    console.log(`📤 WebSocket Client: Sending message to server`);
    
    return new Promise<WebSocketSendResult>((resolve, reject) => {
      const messageData = JSON.stringify(message);
      
      try {
        this.socket!.send(messageData);
        resolve({ success: true, timestamp: new Date().toISOString() });
      } catch (error) {
        reject(error);
      }
    });
  }

  isConnected(): boolean {
    return this.socket?.readyState === WebSocket.OPEN;
  }

  async disconnect(): Promise<void> {
    if (this.socket) {
      console.log(`🔌 WebSocket Client: Disconnecting`);
      this.socket.close();
      this.socket = undefined;
    }
  }

  async reconnect(): Promise<void> {
    if (!this.lastConnectedUrl) {
      throw new Error('Cannot reconnect: No previous connection URL stored');
    }
    
    // Disconnect first if still connected
    await this.disconnect();
    
    // Reconnect using stored URL
    await this.connect(this.lastConnectedUrl);
  }
}