/**
 * WebSocket Transport Browser - Browser-specific WebSocket implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Browser role: client transport that connects to server.
 */

import { WebSocketTransportBase, type WebSocketConfig } from '../shared/WebSocketTransportBase';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import type { TransportSendResult } from '../../shared/TransportTypes';
import type { ITransportHandler } from '../../shared/ITransportHandler';
import type { EventsInterface } from '../../../events';

// Browser-specific WebSocket configuration with typed inheritance
export interface WebSocketBrowserConfig extends WebSocketConfig {
  url: string;
  handler: ITransportHandler; // REQUIRED transport protocol handler
  eventSystem?: EventsInterface; // REQUIRED for transport events
}

export class WebSocketTransportBrowser extends WebSocketTransportBase {
  public readonly name = 'websocket-client';
  
  private socket?: WebSocket;
  private lastConnectedUrl?: string;
  private handler: ITransportHandler;

  constructor(config: WebSocketBrowserConfig) {
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
    if (this.socket && this.sessionId && this.config.sessionHandshake) {
      const handshake = this.createSessionHandshake();
      this.sendWebSocketMessage(this.socket, handshake);
      console.log(`🤝 ${this.name}: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  async connect(url: string): Promise<void> {
    this.lastConnectedUrl = url; // Store for reconnection
    console.log(`🔗 ${this.name}: Connecting to ${url}`);
    
    return new Promise((resolve, reject) => {
      if (typeof WebSocket === 'undefined') {
        reject(new Error('WebSocket not available in this environment'));
        return;
      }

      this.socket = new WebSocket(url);
      const clientId = this.generateClientId('ws_client');
      
      this.socket.onopen = async () => {
        console.log(`✅ ${this.name}: Connected to ${url}`);
        this.connected = true;
        
        // TypeScript guarantees handler implements ITransportHandler
        console.log(`✅ ${this.name}: Handler compliance enforced by TypeScript`);
        
        // Send session handshake after validation
        this.sendSessionHandshake();
        
        // Emit CONNECTED event using shared method
        this.emitTransportEvent('CONNECTED', { clientId });
        resolve();
      };

      this.socket.onerror = (error): void => {
        this.connected = false;
        const errorObj = new Error(`WebSocket connection error: ${error.type || 'unknown'}`);
        this.handleWebSocketError(errorObj, 'connection');
        reject(errorObj);
      };
      
      this.socket.onmessage = (event): void => {
        try {
          const message = this.parseWebSocketMessage(event.data);
          
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
      };
      
      this.socket.onclose = (): void => {
        console.log(`🔌 ${this.name}: Connection closed`);
        this.connected = false;
        
        // Emit DISCONNECTED event using shared method
        this.emitTransportEvent('DISCONNECTED', {
          clientId,
          reason: 'connection_closed'
        });
        
        this.socket = undefined;
      };
    });
  }

  async send(message: JTAGMessage): Promise<TransportSendResult> {
    console.log(`📤 ${this.name}: Sending message to server`);
    
    try {
      this.sendWebSocketMessage(this.socket, message);
      return this.createResult(true);
    } catch (error) {
      this.handleWebSocketError(error as Error, 'message send');
      throw error;
    }
  }

  async disconnect(): Promise<void> {
    if (this.socket) {
      console.log(`🔌 WebSocket Client: Disconnecting`);
      this.socket.close();
      this.socket = undefined;
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