/**
 * WebSocket Transport Base - Shared logic for WebSocket client and server
 * 
 * Extracts common WebSocket functionality to eliminate duplication between
 * client and server implementations.
 */

import { TransportBase } from '../../shared/TransportBase';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import { TRANSPORT_EVENTS } from '../../shared/TransportEvents';

// WebSocket specific configuration shared between client and server
export interface WebSocketConfig {
  reconnectAttempts?: number;
  reconnectDelay?: number;
  pingInterval?: number;
  sessionHandshake?: boolean;
}

export abstract class WebSocketTransportClient extends TransportBase {
  protected config: WebSocketConfig;
  protected sessionId?: string;
  protected connected = false;
  
  constructor(config: WebSocketConfig = {}) {
    super();
    this.config = {
      reconnectAttempts: 5,
      reconnectDelay: 1000,
      pingInterval: 30000,
      sessionHandshake: true,
      ...config
    };
  }

  /**
   * Abstract method for creating WebSocket - environment-specific
   */
  protected abstract createWebSocket(url: string): any;

  /**
   * Abstract method for setting up WebSocket server - server-specific
   */
  protected abstract createWebSocketServer?(port: number): Promise<any>;

  /**
   * Common WebSocket connection setup - shared between client and server
   */
  protected setupWebSocketEvents(socket: any, clientId: string): void {
    // Connection opened
    socket.onopen = () => {
      console.log(`✅ ${this.name || 'websocket'}: Connected`);
      this.connected = true;
      
      // Send session handshake after connection
      if (this.config.sessionHandshake) {
        this.sendSessionHandshake(socket);
      }
      
      // Emit CONNECTED event
      this.emitTransportEvent('CONNECTED', { clientId });
    };

    // Message received  
    socket.onmessage = (event: any) => {
      try {
        const message = this.parseWebSocketMessage(event.data);
        
        // Handle session handshake messages
        if (this.isSessionHandshake(message)) {
          this.handleSessionHandshake(message);
          return;
        }
        
        // Forward regular messages to handler
        this.handleIncomingMessage(message);
      } catch (error) {
        this.handleWebSocketError(error as Error, 'message parsing');
      }
    };

    // Connection closed
    socket.onclose = () => {
      console.log(`🔌 ${this.name || 'websocket'}: Connection closed`);
      this.connected = false;
      
      // Emit DISCONNECTED event
      this.emitTransportEvent('DISCONNECTED', {
        clientId,
        reason: 'connection_closed'
      });
    };

    // Connection error
    socket.onerror = (error: any) => {
      this.connected = false;
      const errorObj = new Error(`WebSocket error: ${error.type || 'unknown'}`);
      this.handleWebSocketError(errorObj, 'connection');
    };
  }

  /**
   * Send session handshake to peer
   */
  protected sendSessionHandshake(socket: any): void {
    if (this.sessionId && this.config.sessionHandshake) {
      const handshake = this.createSessionHandshake();
      this.sendWebSocketMessage(socket, handshake);
      console.log(`🤝 ${this.name || 'websocket'}: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  /**
   * Set the session ID for handshake
   */
  setSessionId(sessionId: string): void {
    this.sessionId = sessionId;
  }

  /**
   * Create session handshake message - shared format
   */
  protected createSessionHandshake(): object {
    return {
      type: 'session_handshake',
      sessionId: this.sessionId,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Check if message is a session handshake
   */
  protected isSessionHandshake(message: any): boolean {
    return message.type === 'session_handshake' && message.sessionId;
  }

  /**
   * Handle session handshake message - can be overridden by implementations
   */
  protected handleSessionHandshake(message: any): void {
    console.log(`🤝 ${this.name}: Received session handshake with sessionId: ${message.sessionId}`);
  }

  /**
   * Send message through WebSocket with error handling
   */
  protected sendWebSocketMessage(socket: any, message: JTAGMessage | object): void {
    if (!socket || socket.readyState !== 1) { // WebSocket.OPEN = 1
      throw new Error(`WebSocket not ready (readyState: ${socket?.readyState})`);
    }

    const messageData = JSON.stringify(message);
    socket.send(messageData);
  }

  /**
   * Parse incoming WebSocket message with cross-platform compatibility
   */
  protected parseWebSocketMessage(data: any): any {
    try {
      let messageStr: string;
      
      // Handle different data types across environments
      if (typeof data === 'string') {
        messageStr = data;
      } else if (data instanceof ArrayBuffer) {
        // Browser environment - convert ArrayBuffer to string
        messageStr = new TextDecoder().decode(data);
      } else if (typeof Buffer !== 'undefined' && data instanceof Buffer) {
        // Node.js environment - convert Buffer to string
        messageStr = data.toString();
      } else if (data && typeof data.toString === 'function') {
        // Fallback - try toString method
        messageStr = data.toString();
      } else {
        throw new Error(`Unsupported message data type: ${typeof data}`);
      }
      
      return JSON.parse(messageStr);
    } catch (error) {
      console.error(`❌ ${this.name}: Message parse error:`, error);
      throw error;
    }
  }

  /**
   * Emit transport events with consistent format
   */
  protected emitTransportEvent(eventType: keyof typeof TRANSPORT_EVENTS, data: any): void {
    if (this.eventSystem) {
      this.eventSystem.emit(TRANSPORT_EVENTS[eventType], {
        transportType: 'websocket' as const,
        ...data
      });
    }
  }

  /**
   * Generate unique client ID
   */
  protected generateClientId(prefix = 'ws'): string {
    return `${prefix}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
  }

  /**
   * Handle WebSocket error with consistent logging
   */
  protected handleWebSocketError(error: Error, context = 'connection'): void {
    console.error(`❌ ${this.name}: ${context} error:`, error);
    this.emitTransportEvent('ERROR', {
      error: error.message,
      context
    });
  }

  // ===== SHARED CLIENT METHODS (from working browser implementation) =====

  protected socket?: any; // WebSocket instance
  protected lastConnectedUrl?: string; // For reconnection
  
  /**
   * Connect to WebSocket server - shared client implementation
   */
  async connect(url: string): Promise<void> {
    this.lastConnectedUrl = url; // Store for reconnection
    console.log(`🔗 ${this.name}: Connecting to ${url}`);
    
    return new Promise((resolve, reject) => {
      try {
        this.socket = this.createWebSocket(url);
        const clientId = this.generateClientId('ws_client');
        
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
        this.socket.onerror = (error: any): void => {
          originalOnerror?.call(this.socket!, error);
          reject(new Error(`WebSocket connection error: ${error.type || 'unknown'}`));
        };
        
      } catch (error) {
        reject(error);
      }
    });
  }

  /**
   * Send message via WebSocket - shared client implementation
   */
  async send(message: any): Promise<any> {
    console.log(`📤 ${this.name}: Sending message to server`);
    
    try {
      this.sendWebSocketMessage(this.socket, message);
      return this.createResult(true);
    } catch (error) {
      this.handleWebSocketError(error as Error, 'message send');
      throw error;
    }
  }

  /**
   * Disconnect WebSocket - shared client implementation
   */
  async disconnect(): Promise<void> {
    if (this.socket) {
      console.log(`🔌 ${this.name}: Disconnecting`);
      this.socket.close();
      this.socket = undefined;
      this.connected = false;
    }
  }

  /**
   * Reconnect using stored URL - shared client implementation
   */
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