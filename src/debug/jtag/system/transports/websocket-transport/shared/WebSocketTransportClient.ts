/**
 * WebSocket Transport Base - Shared logic for WebSocket client and server
 * 
 * Extracts common WebSocket functionality to eliminate duplication between
 * client and server implementations.
 */

import { TransportBase } from '../../shared/TransportBase';
import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { EventsInterface } from '../../../events';
import { TRANSPORT_EVENTS } from '../../shared/TransportEvents';
import type { TransportSendResult } from '@system/transports/shared';
import type { 
  JTAGUniversalWebSocket, 
  JTAGWebSocketOpenEvent, 
  JTAGWebSocketMessageEvent, 
  JTAGWebSocketCloseEvent, 
  JTAGWebSocketErrorEvent,
  JTAGWebSocketReadyState 
} from './WebSocketInterface';
import { JTAG_WEBSOCKET_READY_STATE, JTAG_WEBSOCKET_EVENTS } from './WebSocketInterface';
import { isJTAGSessionHandshake, isJTAGMessage } from './JTAGWebSocketTypes';

// WebSocket specific configuration shared between client and server
export interface WebSocketConfig {
  reconnectAttempts?: number;
  reconnectDelay?: number;
  pingInterval?: number;
  sessionHandshake?: boolean;
}

export abstract class WebSocketTransportClient extends TransportBase {
  protected config: WebSocketConfig;
  protected sessionId?: UUID;
  protected connected = false;
  protected messageHandler?: (message: JTAGMessage) => void;
  
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
   * Must return a JTAG-compatible WebSocket object
   */
  protected abstract createWebSocket(url: string): JTAGUniversalWebSocket;

  /**
   * Common WebSocket connection setup - type-safe and consistent
   */
  protected setupWebSocketEvents(socket: JTAGUniversalWebSocket, clientId: string): void {
    // Connection opened - consistent addEventListener pattern
    socket.addEventListener('open', (event: JTAGWebSocketOpenEvent) => {
      console.log(`✅ ${this.name || 'websocket'}: Connected`);
      this.connected = true;
      
      // Send session handshake after connection
      if (this.config.sessionHandshake) {
        this.sendSessionHandshake(socket);
      }
      
      // Emit CONNECTED event
      this.emitTransportEvent('CONNECTED', { clientId });
    });

    // Message received - consistent addEventListener pattern
    socket.addEventListener('message', (event: JTAGWebSocketMessageEvent) => {
      try {
        const message = this.parseWebSocketMessage(event.data);
        
        // Handle session handshake messages
        if (this.isSessionHandshake(message)) {
          this.handleSessionHandshake(message);
          return;
        }
        
        // Forward regular messages to handler
        if (this.messageHandler) {
          this.messageHandler(message);
        } else {
          console.warn(`${this.name}: Received message but no handler set:`, message);
        }
      } catch (error) {
        this.handleWebSocketError(error as Error, 'message parsing');
      }
    });

    // Connection closed - consistent addEventListener pattern
    socket.addEventListener('close', (event: JTAGWebSocketCloseEvent) => {
      console.log(`🔌 ${this.name || 'websocket'}: Connection closed (code: ${event.code})`);
      this.connected = false;
      
      // Emit DISCONNECTED event
      this.emitTransportEvent('DISCONNECTED', {
        clientId,
        reason: 'connection_closed',
        code: event.code
      });
    });

    // Connection error - consistent addEventListener pattern
    socket.addEventListener('error', (event: JTAGWebSocketErrorEvent) => {
      this.connected = false;
      const errorMsg = event.error?.message || event.message || 'Unknown WebSocket error';
      const errorObj = new Error(`WebSocket error: ${errorMsg}`);
      this.handleWebSocketError(errorObj, 'connection');
    });
  }

  /**
   * Send session handshake to peer
   */
  protected sendSessionHandshake(socket: JTAGUniversalWebSocket): void {
    if (this.sessionId && this.config.sessionHandshake) {
      const handshake = this.createSessionHandshake();
      this.sendWebSocketMessage(socket, handshake);
      console.log(`🤝 ${this.name || 'websocket'}: Sent session handshake with sessionId: ${this.sessionId}`);
    }
  }

  /**
   * Set the session ID for handshake
   */
  setSessionId(sessionId: UUID): void {
    this.sessionId = sessionId;
  }

  /**
   * Set message handler for incoming messages
   */
  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandler = handler;
  }

  /**
   * Set event system for transport events
   */
  setEventSystem(eventSystem: EventsInterface): void {
    this.eventSystem = eventSystem;
  }

  /**
   * Create session handshake message - simplified for transport layer
   */
  protected createSessionHandshake(): any {
    // Simplified handshake - will be properly structured when JTAG message factory is available
    return {
      messageType: 'event',
      endpoint: 'session/handshake',
      payload: {
        sessionId: this.sessionId!,
        timestamp: new Date().toISOString()
      }
    };
  }

  /**
   * Check if message is a session handshake - deprecated, use type guard
   */
  protected isSessionHandshake(message: unknown): boolean {
    return isJTAGSessionHandshake(message);
  }

  /**
   * Handle session handshake message - can be overridden by implementations
   */
  protected handleSessionHandshake(message: any): void {
    const payload = message.payload as Record<string, unknown> | undefined;
    console.log(`🤝 ${this.name}: Received session handshake with sessionId: ${payload?.sessionId}`);
  }

  /**
   * Send message through WebSocket with error handling - handles both regular messages and handshakes
   */
  protected sendWebSocketMessage(socket: JTAGUniversalWebSocket, message: any): void {
    if (!socket || socket.readyState !== JTAG_WEBSOCKET_READY_STATE.OPEN) {
      throw new Error(`WebSocket not ready (readyState: ${socket?.readyState})`);
    }

    const messageData = JSON.stringify(message);
    socket.send(messageData);
  }

  /**
   * Parse incoming WebSocket message with cross-platform compatibility
   */
  protected parseWebSocketMessage(data: unknown): any {
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

  protected socket?: JTAGUniversalWebSocket; // WebSocket instance
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
        
        // Set up consistent event handling
        this.setupWebSocketEvents(this.socket, clientId);
        
        // Add Promise resolution/rejection to the consistent event handlers
        const handleOpen = (event: JTAGWebSocketOpenEvent): void => {
          console.log(`✅ ${this.name}: Handler compliance enforced by TypeScript`);
          this.socket!.removeEventListener('error', handleError);
          resolve();
        };
        
        const handleError = (event: JTAGWebSocketErrorEvent): void => {
          const errorMsg = event.error?.message || event.message || 'Connection failed';
          this.socket!.removeEventListener('open', handleOpen);
          reject(new Error(`WebSocket connection error: ${errorMsg}`));
        };
        
        // Add temporary listeners for Promise resolution
        this.socket.addEventListener('open', handleOpen);
        this.socket.addEventListener('error', handleError);
        
      } catch (error) {
        reject(error);
      }
    });
  }

  /**
   * Send message via WebSocket - shared client implementation
   */
  async send(message: JTAGMessage): Promise<TransportSendResult> {
    console.log(`📤 ${this.name}: Sending message to server`);
    
    if (!this.socket) {
      throw new Error('WebSocket not connected');
    }
    
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