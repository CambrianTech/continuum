/**
 * WebSocket Transport Browser - Browser-specific WebSocket implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Browser role: client transport that connects to server.
 */

import { WebSocketTransportBase, type WebSocketConfig } from '../shared/WebSocketTransportBase';
import type { ITransportAdapter } from '../../shared/TransportBase';
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

export class WebSocketTransportBrowser extends WebSocketTransportBase implements ITransportAdapter {
  public readonly name = 'websocket-client';
  public readonly protocol = 'websocket';
  public readonly supportedRoles = ['client'];
  public readonly supportedEnvironments = ['browser'];
  
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
   * Browser-specific WebSocket creation
   */
  protected createWebSocket(url: string): WebSocket {
    if (typeof WebSocket === 'undefined') {
      throw new Error('WebSocket not available in this environment');
    }
    return new WebSocket(url);
  }

  /**
   * Browser doesn't create servers - not implemented
   */
  protected createWebSocketServer(port: number): Promise<never> {
    throw new Error('Browser cannot create WebSocket servers - use server environment instead');
  }

  async connect(url: string): Promise<void> {
    this.lastConnectedUrl = url; // Store for reconnection
    console.log(`🔗 ${this.name}: Connecting to ${url}`);
    
    return new Promise((resolve, reject) => {
      try {
        this.socket = this.createWebSocket(url);
        const clientId = this.generateClientId('ws_browser_client');
        
        // Use shared event setup from base class
        this.setupWebSocketEvents(this.socket, clientId);
        
        // Override onopen to add resolve callback
        const originalOnopen = this.socket.onopen;
        this.socket.onopen = (event: Event): void => {
          originalOnopen?.call(this.socket!, event);
          console.log(`✅ ${this.name}: Handler compliance enforced by TypeScript`);
          resolve();
        };
        
        // Override onerror to add reject callback
        const originalOnerror = this.socket.onerror;
        this.socket.onerror = (error): void => {
          originalOnerror?.call(this.socket!, error);
          reject(new Error(`WebSocket connection error: (error.type || 'unknown')`));
        };
        
      } catch (error) {
        reject(error);
      }
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
      console.log(`🔌 ${this.name}: Disconnecting`);
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