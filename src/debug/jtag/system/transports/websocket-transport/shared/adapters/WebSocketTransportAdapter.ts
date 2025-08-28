/**
 * WebSocket Transport Adapter - WebSocket-specific implementation using generic base
 * 
 * Extends the generic TransportAdapterBase with WebSocket protocol-specific behavior.
 * Demonstrates proper separation of concerns and adapter pattern.
 * 
 * ARCHITECTURE:
 * - Generic base handles: callbacks, error handling, lifecycle
 * - This adapter handles: WebSocket protocol specifics, connection management
 */

import { TransportAdapterBase } from '../../../shared/adapters/TransportAdapterBase';
import type { TransportAdapterConfig, TransportSendResult } from '../../../shared/adapters/TransportAdapterBase';
import {
  TRANSPORT_PROTOCOLS,
  TRANSPORT_ROLES,
  type WebSocketProtocolContract
} from '../../../shared/TransportProtocolContracts';
import type { 
  JTAGUniversalWebSocket, 
  JTAGWebSocketReadyState 
} from '../WebSocketInterface';
import { JTAG_WEBSOCKET_READY_STATE } from '../WebSocketInterface';

/**
 * WebSocket-specific adapter configuration
 */
export type WebSocketTransportConfig = TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET>;

/**
 * WebSocket Transport Adapter - Client implementation
 * 
 * Pure WebSocket protocol handling with generic base functionality
 */
export class WebSocketClientAdapter extends TransportAdapterBase<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> {
  private websocket?: JTAGUniversalWebSocket;
  private reconnectTimer?: NodeJS.Timeout;

  constructor(config: WebSocketTransportConfig) {
    super(config);
  }

  /**
   * Connect to WebSocket server - protocol-specific implementation
   */
  public async connect(config?: WebSocketProtocolContract['config']): Promise<void> {
    const connectConfig = config || this.config.protocolConfig;
    
    // Build WebSocket URL - adapter responsibility
    const url = connectConfig.url || 
                `ws://${connectConfig.host || 'localhost'}:${connectConfig.port || 8080}`;

    return new Promise((resolve, reject) => {
      try {
        // Create WebSocket connection - environment-specific
        if (typeof WebSocket !== 'undefined') {
          // Browser environment
          this.websocket = new WebSocket(url) as JTAGUniversalWebSocket;
        } else {
          // Node.js environment
          const WebSocketImpl = require('ws');
          this.websocket = new WebSocketImpl(url) as JTAGUniversalWebSocket;
        }

        // Set up protocol-specific event handlers
        this.websocket.onopen = () => {
          this.clearReconnectTimer();
          this.emitConnect(); // Generic base handles callbacks
          resolve();
        };

        this.websocket.onclose = (event) => {
          const reason = event.reason || `WebSocket closed: ${event.code}`;
          this.handleDisconnection(reason);
        };

        this.websocket.onerror = (event) => {
          const error = this.createTransportError(
            `WebSocket connection error: ${event.type}`,
            'CONNECTION_FAILED'
          );
          this.emitError(error); // Generic base handles error callbacks
          reject(error);
        };

        // Handle incoming WebSocket messages
        this.websocket.onmessage = (event) => {
          this.handleIncomingMessage(event.data);
        };

      } catch (error) {
        const transportError = this.createTransportError(
          `Failed to create WebSocket connection: ${error instanceof Error ? error.message : String(error)}`,
          'CONNECTION_FAILED',
          error instanceof Error ? error : undefined
        );
        reject(transportError);
      }
    });
  }

  /**
   * Send data via WebSocket - protocol-specific implementation
   */
  public async send(data: string | Uint8Array): Promise<TransportSendResult> {
    if (!this.websocket || !this.isConnected()) {
      throw this.createTransportError(
        'WebSocket not connected',
        'SEND_FAILED'
      );
    }

    try {
      // Send via WebSocket protocol
      this.websocket.send(data);
      
      // Return generic send result
      return this.createSendResult(
        true,
        typeof data === 'string' ? new Blob([data]).size : data.length,
        {
          protocol: 'websocket',
          messageType: typeof data === 'string' ? 'text' : 'binary'
        }
      );

    } catch (error) {
      const transportError = this.createTransportError(
        `WebSocket send failed: ${error instanceof Error ? error.message : String(error)}`,
        'SEND_FAILED',
        error instanceof Error ? error : undefined
      );
      
      this.emitError(transportError);
      throw transportError;
    }
  }

  /**
   * Check WebSocket connection status
   */
  public isConnected(): boolean {
    return super.isConnected() && 
           this.websocket?.readyState === JTAG_WEBSOCKET_READY_STATE.OPEN;
  }

  /**
   * Disconnect WebSocket - protocol-specific cleanup
   */
  public async disconnect(): Promise<void> {
    this.clearReconnectTimer();
    
    if (this.websocket && this.websocket.readyState !== JTAG_WEBSOCKET_READY_STATE.CLOSED) {
      this.websocket.close(1000, 'Manual disconnect');
    }
    
    this.websocket = undefined;
    this.cleanup(); // Generic base cleanup
  }

  /**
   * Handle incoming WebSocket message data
   */
  private handleIncomingMessage(data: any): void {
    try {
      // Convert WebSocket message to transport data format
      const transportData = typeof data === 'string' ? data : new Uint8Array(data);
      
      // Emit via generic base
      this.emitData(transportData);
      
    } catch (error) {
      const transportError = this.createTransportError(
        `Failed to process WebSocket message: ${error instanceof Error ? error.message : String(error)}`,
        'PROTOCOL_ERROR',
        error instanceof Error ? error : undefined
      );
      
      this.emitError(transportError);
    }
  }

  /**
   * Handle WebSocket disconnection with potential reconnection
   */
  private handleDisconnection(reason: string): void {
    this.emitDisconnect(reason); // Generic base handles callbacks
    
    // Implement auto-reconnect if configured
    const autoReconnect = this.config.adapterOptions?.autoReconnect;
    const maxRetries = this.config.adapterOptions?.maxRetries || 3;
    const retryDelay = this.config.adapterOptions?.retryDelay || 1000;
    
    if (autoReconnect && this.canAttemptReconnect()) {
      this.scheduleReconnect(retryDelay);
    }
  }

  /**
   * Schedule reconnection attempt
   */
  private scheduleReconnect(delay: number): void {
    this.clearReconnectTimer();
    
    this.reconnectTimer = setTimeout(async () => {
      try {
        await this.connect();
      } catch (error) {
        // Reconnection failed - will be handled by connect() error handling
      }
    }, delay);
  }

  /**
   * Clear reconnection timer
   */
  private clearReconnectTimer(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
  }

  /**
   * Check if reconnection should be attempted
   */
  private canAttemptReconnect(): boolean {
    // Add logic for retry limits, exponential backoff, etc.
    return true; // Simplified for now
  }
}