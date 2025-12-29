/**
 * Pure WebSocket Transport - Dumb pipe WebSocket implementation
 * 
 * This is a truly pure WebSocket transport that only handles networking.
 * No JTAG business logic, no message interpretation, no session handling.
 * Just a dumb pipe that sends and receives raw data.
 * 
 * ARCHITECTURE PRINCIPLE: "Transports are dumb pipes"
 */

import type { PureTransport, PureTransportConfig, PureSendResult } from '../../shared/PureTransportTypes';
import { 
  TRANSPORT_PROTOCOLS, 
  TRANSPORT_ROLES, 
  ENVIRONMENT_TYPES,
  type TransportProtocol,
  type TransportRole,
  type EnvironmentType,
  type WebSocketProtocolContract,
  type CrossEnvironmentTransport,
  isWebSocketProtocol
} from '../../shared/TransportProtocolContracts';
import type { 
  JTAGUniversalWebSocket, 
  JTAGWebSocketReadyState 
} from './WebSocketInterface';
import { JTAG_WEBSOCKET_READY_STATE } from './WebSocketInterface';

/**
 * WebSocket-specific pure configuration
 */
export interface PureWebSocketConfig extends PureTransportConfig {
  protocol: 'websocket';
  url?: string;
  host?: string;
  port?: number;
  // Pure networking parameters only
  reconnectAttempts?: number;
  reconnectDelay?: number;
  pingInterval?: number;
  // No JTAG concepts
}

/**
 * Pure WebSocket Transport - Client implementation
 * Connects to WebSocket servers as a dumb pipe
 */
export class PureWebSocketTransport implements PureTransport {
  public readonly name = 'pure-websocket';
  public readonly protocol = 'websocket' as const;
  
  private websocket?: JTAGUniversalWebSocket;
  private config: PureWebSocketConfig;
  private connected = false;
  
  // Pure callback functions - no JTAG interpretation
  private dataCallback?: (data: string | Uint8Array) => void;
  private connectCallback?: () => void;
  private disconnectCallback?: (reason?: string) => void;
  private errorCallback?: (error: Error) => void;
  
  constructor(config: PureWebSocketConfig) {
    this.config = {
      reconnectAttempts: 3,
      reconnectDelay: 1000,
      pingInterval: 30000,
      ...config
    };
  }
  
  /**
   * Connect to WebSocket server - pure networking
   */
  async connect(config?: PureTransportConfig): Promise<void> {
    const connectConfig = config ? { ...this.config, ...config } as PureWebSocketConfig : this.config;
    
    // Build WebSocket URL from configuration
    const url = connectConfig.url || `ws://${connectConfig.host || 'localhost'}:${connectConfig.port || 8080}`;
    
    return new Promise((resolve, reject) => {
      try {
        // Create WebSocket connection
        if (typeof WebSocket !== 'undefined') {
          // Browser environment
          this.websocket = new WebSocket(url) as JTAGUniversalWebSocket;
        } else {
          // Node.js environment
          const WebSocketImpl = require('ws');
          this.websocket = new WebSocketImpl(url) as JTAGUniversalWebSocket;
        }
        
        // Set up pure networking event handlers
        this.websocket.onopen = () => {
          this.connected = true;
          this.connectCallback?.();
          resolve();
        };
        
        this.websocket.onclose = (event) => {
          this.connected = false;
          const reason = event.reason || `Code: ${event.code}`;
          this.disconnectCallback?.(reason);
          // Emit immediate disconnect event for browser widgets (like WebViewWidget freeze)
          if (typeof window !== 'undefined' && (window as any).JTAGEvents) {
            console.log('🔌 PureWebSocketTransport: WebSocket closed - emitting connection:status');
            (window as any).JTAGEvents.emit('connection:status', {
              connected: false,
              state: 'disconnected',
              color: '#ff0000',
              timestamp: Date.now()
            });
          }
        };

        this.websocket.onerror = (event) => {
          const error = new Error(`WebSocket error: ${event.type}`);
          this.errorCallback?.(error);
          // Emit immediate disconnect event for browser widgets
          if (typeof window !== 'undefined' && (window as any).JTAGEvents) {
            console.log('⚠️ PureWebSocketTransport: WebSocket error - emitting connection:status');
            (window as any).JTAGEvents.emit('connection:status', {
              connected: false,
              state: 'disconnected',
              color: '#ff0000',
              timestamp: Date.now()
            });
          }
          reject(error);
        };
        
        // Raw data handler - no interpretation
        this.websocket.onmessage = (event) => {
          if (this.dataCallback) {
            // Pass raw data without interpretation
            const data = typeof event.data === 'string' ? event.data : new Uint8Array(event.data);
            this.dataCallback(data);
          }
        };
        
      } catch (error) {
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }
  
  /**
   * Send raw data - no message interpretation
   */
  async send(data: string | Uint8Array): Promise<PureSendResult> {
    if (!this.websocket || !this.connected) {
      throw new Error('WebSocket not connected');
    }
    
    try {
      // Send raw data without modification
      this.websocket.send(data);
      
      return {
        success: true,
        timestamp: new Date().toISOString(),
        bytesTransmitted: typeof data === 'string' ? new Blob([data]).size : data.length
      };
      
    } catch (error) {
      return {
        success: false,
        timestamp: new Date().toISOString()
      };
    }
  }
  
  /**
   * Check connection status - pure networking
   */
  isConnected(): boolean {
    return this.connected && 
           this.websocket?.readyState === JTAG_WEBSOCKET_READY_STATE.OPEN;
  }
  
  /**
   * Disconnect - pure networking
   */
  async disconnect(): Promise<void> {
    if (this.websocket && this.connected) {
      this.websocket.close();
      this.connected = false;
    }
  }
  
  /**
   * Set raw data callback - no JTAG interpretation
   */
  onData(callback: (data: string | Uint8Array) => void): void {
    this.dataCallback = callback;
  }
  
  /**
   * Set connection callback - pure networking event
   */
  onConnect(callback: () => void): void {
    this.connectCallback = callback;
  }
  
  /**
   * Set disconnection callback - pure networking event
   */
  onDisconnect(callback: (reason?: string) => void): void {
    this.disconnectCallback = callback;
  }
  
  /**
   * Set error callback - pure networking error
   */
  onError(callback: (error: Error) => void): void {
    this.errorCallback = callback;
  }
}

/**
 * Pure WebSocket Server Transport - Server implementation
 * Accepts WebSocket connections as a dumb pipe server
 */
export class PureWebSocketServerTransport implements PureTransport {
  public readonly name = 'pure-websocket-server';
  public readonly protocol = 'websocket' as const;
  
  private server?: any; // WebSocketServer from 'ws'
  private clients = new Set<JTAGUniversalWebSocket>();
  private config: PureWebSocketConfig;
  private connected = false;
  
  // Pure callback functions
  private dataCallback?: (data: string | Uint8Array) => void;
  private connectCallback?: () => void;
  private disconnectCallback?: (reason?: string) => void;
  private errorCallback?: (error: Error) => void;
  
  constructor(config: PureWebSocketConfig) {
    this.config = config;
  }
  
  /**
   * Start WebSocket server - pure networking
   */
  async connect(config?: PureTransportConfig): Promise<void> {
    const connectConfig = config ? { ...this.config, ...config } as PureWebSocketConfig : this.config;
    
    return new Promise((resolve, reject) => {
      try {
        // Node.js WebSocket server
        const { WebSocketServer } = require('ws');
        
        this.server = new WebSocketServer({
          port: connectConfig.port || 8080
        });
        
        this.server.on('listening', () => {
          this.connected = true;
          this.connectCallback?.();
          resolve();
        });
        
        this.server.on('error', (error: Error) => {
          this.errorCallback?.(error);
          reject(error);
        });
        
        // Handle new client connections
        this.server.on('connection', (client: JTAGUniversalWebSocket) => {
          this.clients.add(client);
          
          // Raw data from client - no interpretation
          client.on('message', (data) => {
            if (this.dataCallback) {
              const processedData = typeof data === 'string' ? data : new Uint8Array(data);
              this.dataCallback(processedData);
            }
          });
          
          // Client disconnect
          client.on('close', () => {
            this.clients.delete(client);
          });
          
          client.on('error', (error) => {
            this.clients.delete(client);
            this.errorCallback?.(error);
          });
        });
        
      } catch (error) {
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }
  
  /**
   * Send raw data to all clients - no message interpretation
   */
  async send(data: string | Uint8Array): Promise<PureSendResult> {
    if (!this.connected || this.clients.size === 0) {
      throw new Error('WebSocket server not connected or no clients');
    }
    
    let successCount = 0;
    let totalBytes = 0;
    
    // Broadcast to all connected clients
    for (const client of this.clients) {
      try {
        if (client.readyState === JTAG_WEBSOCKET_READY_STATE.OPEN) {
          client.send(data);
          successCount++;
          totalBytes += typeof data === 'string' ? new Blob([data]).size : data.length;
        }
      } catch (error) {
        // Continue sending to other clients
      }
    }
    
    return {
      success: successCount > 0,
      timestamp: new Date().toISOString(),
      bytesTransmitted: totalBytes
    };
  }
  
  /**
   * Check if server is running
   */
  isConnected(): boolean {
    return this.connected && this.server?.listening;
  }
  
  /**
   * Stop WebSocket server
   */
  async disconnect(): Promise<void> {
    if (this.server && this.connected) {
      // Close all client connections
      for (const client of this.clients) {
        client.close();
      }
      this.clients.clear();
      
      // Close server
      this.server.close();
      this.connected = false;
    }
  }
  
  /**
   * Set raw data callback
   */
  onData(callback: (data: string | Uint8Array) => void): void {
    this.dataCallback = callback;
  }
  
  /**
   * Set server start callback
   */
  onConnect(callback: () => void): void {
    this.connectCallback = callback;
  }
  
  /**
   * Set server stop callback
   */
  onDisconnect(callback: (reason?: string) => void): void {
    this.disconnectCallback = callback;
  }
  
  /**
   * Set error callback
   */
  onError(callback: (error: Error) => void): void {
    this.errorCallback = callback;
  }
}