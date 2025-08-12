/**
 * WebSocket Transport Browser - Browser-specific WebSocket implementation
 * 
 * Uses typed inheritance from shared WebSocket base class.
 * Browser role: client transport that connects to server.
 */

import { WebSocketTransportClient, type WebSocketConfig } from '../shared/WebSocketTransportClient';
import type { ITransportAdapter } from '../../shared/TransportBase';
import type { ITransportHandler } from '../../shared/ITransportHandler';
import type { EventsInterface } from '../../../events';

// Browser-specific WebSocket configuration with typed inheritance
export interface WebSocketBrowserConfig extends WebSocketConfig {
  url: string;
  handler: ITransportHandler; // REQUIRED transport protocol handler
  eventSystem?: EventsInterface; // REQUIRED for transport events
}

export class WebSocketTransportClientBrowser extends WebSocketTransportClient implements ITransportAdapter {
  public readonly name = 'websocket-client';
  public readonly protocol = 'websocket';
  public readonly supportedRoles = ['client'];
  public readonly supportedEnvironments = ['browser'];
  
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

  // connect(), send(), disconnect(), reconnect() methods inherited from base class

}