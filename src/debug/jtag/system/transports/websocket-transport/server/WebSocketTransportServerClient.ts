/**
 * WebSocket Transport Server Client - Node.js WebSocket client implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Server role: client transport that connects to JTAG server from Node.js environment.
 * This enables CLI tools and server-side scripts to connect as WebSocket clients.
 */

import { WebSocketTransportBase, type WebSocketConfig } from '../shared/WebSocketTransportBase';
import type { JTAGMessage } from '@shared/JTAGTypes';
import type { TransportSendResult } from '../../shared/TransportTypes';
import type { ITransportHandler } from '../../shared/ITransportHandler';
import type { EventsInterface } from '@systemEvents';

// Server client-specific WebSocket configuration with typed inheritance
export interface WebSocketServerClientConfig extends WebSocketConfig {
  url: string;
  handler: ITransportHandler; // REQUIRED transport protocol handler
  eventSystem?: EventsInterface; // REQUIRED for transport events
}

export class WebSocketTransportServerClient extends WebSocketTransportBase {
  public readonly name = 'websocket-server-client';
  
  private client?: any; // ws.WebSocket type
  private lastConnectedUrl?: string;
  private handler: ITransportHandler;

  constructor(config: WebSocketServerClientConfig) {
    super(config);
    this.handler = config.handler;
    
    // Set event system for transport events (CRITICAL for health management)
    if (config.eventSystem) {
      this.setEventSystem(config.eventSystem);
    }
  }

  /**
   * Send session handshake to server - uses shared base method
   */
  private sendSessionHandshake(): void {
    if (this.client && this.sessionId && this.config.sessionHandshake) {
      const handshake = this.createSessionHandshake();
      this.sendWebSocketMessage(this.client, handshake);
      console.log(`🤝 ${this.name}: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  async connect(url: string): Promise<void> {
    this.lastConnectedUrl = url; // Store for reconnection
    console.log(`🔗 ${this.name}: Connecting to ${url}`);
    
    return new Promise(async (resolve, reject) => {
      try {
        // Dynamic import to handle WebSocket client availability
        const WebSocketModule = await eval('import("ws")');
        const WSClient = WebSocketModule.default || WebSocketModule.WebSocket;
        
        this.client = new WSClient(url);
        const clientId = this.generateClientId('ws_server_client');
        
        this.client.on('open', () => {
          console.log(`✅ ${this.name}: Connected to ${url}`);
          this.connected = true;
          
          // TypeScript guarantees handler implements ITransportHandler
          console.log(`✅ ${this.name}: Handler compliance enforced by TypeScript`);
          
          // Send session handshake after validation
          this.sendSessionHandshake();
          
          // Emit CONNECTED event using shared method
          this.emitTransportEvent('CONNECTED', { clientId });
          resolve();
        });

        this.client.on('error', (error: Error) => {
          this.connected = false;
          const errorObj = new Error(`WebSocket connection error: ${error.message}`);
          this.handleWebSocketError(errorObj, 'connection');
          reject(errorObj);
        });
        
        this.client.on('message', (data: any) => {
          try {
            // Convert Buffer to string if needed
            const rawData = typeof data === 'string' ? data : data.toString();
            const message = this.parseWebSocketMessage(rawData);
            
            // Handle session handshake messages
            if (this.isSessionHandshake(message)) {
              this.handleSessionHandshake(message);
              return;
            }
            
            // Forward regular messages to base class handler
            this.handleIncomingMessage(message);
          } catch (error) {
            this.handleWebSocketError(error as Error, 'message parsing');
          }
        });
        
        this.client.on('close', () => {
          console.log(`🔌 ${this.name}: Connection closed`);
          this.connected = false;
          
          // Emit DISCONNECTED event using shared method
          this.emitTransportEvent('DISCONNECTED', {
            clientId,
            reason: 'connection_closed'
          });
          
          this.client = undefined;
        });
        
      } catch (error) {
        reject(new Error(`Failed to create WebSocket client: ${error}`));
      }
    });
  }

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    console.log(`📤 ${this.name}: Sending message to server`);
    
    try {
      this.sendWebSocketMessage(this.client, message);
      return this.createResult(true);
    } catch (error) {
      this.handleWebSocketError(error as Error, 'message send');
      throw error;
    }
  }

  async disconnect(): Promise<void> {
    if (this.client) {
      console.log(`🔌 WebSocket Server Client: Disconnecting`);
      this.client.close();
      this.client = undefined;
      this.connected = false;
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