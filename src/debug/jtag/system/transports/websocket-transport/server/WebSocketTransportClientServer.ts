/**
 * WebSocket Transport Server Client - Node.js WebSocket client implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Server role: client transport that connects to JTAG server from Node.js environment.
 * This enables CLI tools and server-side scripts to connect as WebSocket clients.
 */

import { WebSocketTransportClient, type WebSocketConfig } from '../shared/WebSocketTransportClient';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import type { ITransportHandler } from '../../shared/ITransportHandler';
import type { EventsInterface } from '../../../events';
import type { JTAGUniversalWebSocket } from '../shared/WebSocketInterface';
// No longer need JTAGWebSocketMessageData - everything is JTAGMessage
import { ServerWebSocketAdapter } from './WebSocketAdapter';

// Server client-specific WebSocket configuration with typed inheritance
export interface WebSocketServerClientConfig extends WebSocketConfig {
  url: string;
  handler: ITransportHandler; // REQUIRED transport protocol handler
  eventSystem?: EventsInterface; // REQUIRED for transport events
}

export class WebSocketTransportClientServer extends WebSocketTransportClient {
  public readonly name = 'websocket-server-client';
  
  private handler: ITransportHandler;
  
  // Promise-based correlation for request/response matching
  private pendingRequests = new Map<string, {
    resolve: (result: any) => void;
    reject: (error: Error) => void;
    timeout: NodeJS.Timeout;
  }>();

  constructor(config: WebSocketServerClientConfig) {
    super(config);
    this.handler = config.handler;
    
    // Set message handler for incoming messages (CRITICAL for message routing)
    if (config.handler && config.handler.handleTransportMessage) {
      this.setMessageHandler((message: JTAGMessage) => {
        config.handler.handleTransportMessage(message);
      });
    }
    
    // Set event system for transport events (CRITICAL for health management)
    if (config.eventSystem) {
      this.setEventSystem(config.eventSystem);
    }
  }

  /**
   * Server-specific WebSocket creation - returns JTAGUniversalWebSocket adapter
   */
  protected createWebSocket(url: string): JTAGUniversalWebSocket {
    return new ServerWebSocketAdapter(url);
  }

  /**
   * Send session handshake to server - uses shared base method
   */
  protected sendSessionHandshake(): void {
    if (this.socket && this.sessionId && this.config.sessionHandshake) {
      const handshake = this.createSessionHandshake();
      this.sendWebSocketMessage(this.socket, handshake);
      console.log(`🤝 ${this.name}: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  // connect(), send(), disconnect(), reconnect() methods inherited from base class
}