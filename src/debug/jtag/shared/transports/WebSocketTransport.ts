/**
 * WebSocket Transport Implementation
 * Full-featured WebSocket transport with connection lifecycle
 */

import { BaseJTAGTransport } from './BaseTransport';
import { JTAGConfig, JTAGWebSocketMessage, JTAGTransportResponse, JTAG_STATUS, JTAG_TRANSPORT, JTAGTransportType, JTAGStatus } from '../JTAGTypes';

export class JTAGWebSocketTransportImpl extends BaseJTAGTransport {
  private ws: WebSocket | any = null;
  private endpoint: string = '';
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 3;
  private messageHandler?: (message: JTAGWebSocketMessage) => void;
  private disconnectHandler?: () => void;

  constructor() {
    super('websocket-transport');
  }

  getTransportType(): JTAGTransportType {
    return JTAG_TRANSPORT.WEBSOCKET;
  }

  protected getEndpoint(): string { return this.endpoint; }
  protected getProtocol(): string { return 'websocket'; }
  protected getMetadata(): Record<string, any> { 
    return {
      readyState: this.ws?.readyState,
      reconnectAttempts: this.reconnectAttempts,
      maxReconnectAttempts: this.maxReconnectAttempts
    };
  }

  async initialize(config: JTAGConfig): Promise<boolean> {
    this.endpoint = `ws://localhost:${config.jtagPort}`;
    
    // Emit CONNECTING
    this.emitStatus(JTAG_STATUS.CONNECTING, { 
      wsState: 0, // WebSocket.CONNECTING
      reason: 'websocket_initialization',
      endpoint: this.endpoint
    });

    return this.connect();
  }

  private async connect(): Promise<boolean> {
    return new Promise((resolve) => {
      try {
        // Use browser WebSocket or Node.js ws
        const WebSocketConstructor = typeof WebSocket !== 'undefined' ? WebSocket : require('ws');
        this.ws = new WebSocketConstructor(this.endpoint);

        const timeout = setTimeout(() => {
          this.emitStatus(JTAG_STATUS.ERROR, {
            error: 'Connection timeout',
            wsState: this.ws.readyState,
            reason: 'websocket_timeout'
          });
          resolve(false);
        }, 5000);

        this.ws.onopen = () => {
          clearTimeout(timeout);
          this.connected = true;
          this.reconnectAttempts = 0;
          
          this.emitStatus(JTAG_STATUS.READY, {
            wsState: 1, // WebSocket.OPEN
            reason: 'websocket_connected',
            endpoint: this.endpoint,
            connectionId: this.connectionId
          });
          
          resolve(true);
        };

        this.ws.onerror = (error: any) => {
          clearTimeout(timeout);
          this.emitStatus(JTAG_STATUS.ERROR, {
            error: error.message || 'WebSocket error',
            wsState: this.ws.readyState,
            reason: 'websocket_error'
          });
          resolve(false);
        };

        this.ws.onclose = () => {
          this.connected = false;
          this.emitStatus(JTAG_STATUS.DISCONNECTED, {
            wsState: 3, // WebSocket.CLOSED
            reason: 'websocket_closed',
            reconnectAttempts: this.reconnectAttempts
          });
          this.disconnectHandler?.();
          
          // Auto-reconnect logic
          if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.attemptReconnect();
          } else {
            this.emitStatus(JTAG_STATUS.TERMINATED, {
              reason: 'max_reconnect_attempts_reached',
              reconnectAttempts: this.reconnectAttempts
            });
          }
        };

        this.ws.onmessage = (event: any) => {
          try {
            const message = JSON.parse(event.data);
            this.messageHandler?.(message);
          } catch (error: any) {
            console.error('WebSocket message parse error:', error);
          }
        };

      } catch (error: any) {
        this.emitStatus(JTAG_STATUS.ERROR, {
          error: error.message,
          reason: 'websocket_constructor_failed'
        });
        resolve(false);
      }
    });
  }

  private async attemptReconnect(): Promise<void> {
    this.reconnectAttempts++;
    
    this.emitStatus(JTAG_STATUS.CONNECTING, {
      reason: 'websocket_reconnecting',
      retryCount: this.reconnectAttempts
    });

    // Exponential backoff
    const delay = Math.pow(2, this.reconnectAttempts) * 1000;
    
    setTimeout(() => {
      this.connect();
    }, delay);
  }

  async send<T>(message: JTAGWebSocketMessage): Promise<JTAGTransportResponse<T>> {
    const startTime = Date.now();
    
    if (!this.connected || this.ws.readyState !== 1) { // WebSocket.OPEN
      return {
        success: false,
        error: 'WebSocket not connected',
        timestamp: new Date().toISOString(),
        transportMeta: {
          transport: 'websocket',
          duration: Date.now() - startTime,
          retries: 0
        }
      };
    }

    return new Promise((resolve) => {
      try {
        this.ws.send(JSON.stringify(message));
        
        resolve({
          success: true,
          timestamp: new Date().toISOString(),
          transportMeta: {
            transport: 'websocket',
            duration: Date.now() - startTime,
            retries: 0
          }
        });
      } catch (error: any) {
        resolve({
          success: false,
          error: error.message,
          timestamp: new Date().toISOString(),
          transportMeta: {
            transport: 'websocket',
            duration: Date.now() - startTime,
            retries: 0
          }
        });
      }
    });
  }

  isConnected(): boolean {
    return this.connected && this.ws?.readyState === 1; // WebSocket.OPEN
  }

  async disconnect(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.connected = false;
    
    // Always emit DISCONNECTED when disconnect is called explicitly
    this.emitStatus(JTAG_STATUS.DISCONNECTED, {
      reason: 'explicit_disconnect',
      wsState: 3 // WebSocket.CLOSED
    });
  }

  onMessage(handler: (message: JTAGWebSocketMessage) => void): void {
    this.messageHandler = handler;
  }

  onDisconnect(handler: () => void): void {
    this.disconnectHandler = handler;
  }

  onStatusChange(handler: (status: JTAGStatus, details?: any) => void): void {
    this.statusHandler = handler;
  }
}