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
import type { TransportSendResult } from '../../shared';
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
import { Events } from '../../../core/shared/Events';

// Verbose logging helper (works in both browser and server)
const verbose = () => {
  if (typeof window !== 'undefined') {
    return (window as any).JTAG_VERBOSE === true;
  }
  if (typeof process !== 'undefined') {
    return process.env.JTAG_VERBOSE === 'true';
  }
  return false;
};

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
  protected reconnectAttempt = 0;
  protected reconnectTimeout?: ReturnType<typeof setTimeout>;
  protected manualDisconnect = false;
  protected isReconnecting = false;
  protected messageQueue: any[] = [];
  protected lastErrorLog = 0;
  protected hasEmittedDisconnectError = false; // Track if we've already emitted ERROR for being disconnected

  constructor(config: WebSocketConfig = {}) {
    super();
    this.config = {
      reconnectAttempts: 3, // Auto-reconnect up to 3 times (server restart, sleep recovery)
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
      verbose() && console.log(`✅ ${this.name || 'websocket'}: Connected`);
      this.connected = true;
      this.reconnectAttempt = 0; // Reset reconnection counter on successful connect
      this.manualDisconnect = false; // Reset manual disconnect flag
      this.isReconnecting = false; // No longer reconnecting
      this.hasEmittedDisconnectError = false; // Reset error state - we're connected now

      // Send session handshake after connection
      if (this.config.sessionHandshake) {
        this.sendSessionHandshake(socket);
      }

      // Flush queued messages after reconnection
      if (this.messageQueue.length > 0) {
        verbose() && console.log(`📤 ${this.name}: Flushing ${this.messageQueue.length} queued messages`);
        const queue = [...this.messageQueue];
        this.messageQueue = [];
        for (const message of queue) {
          try {
            this.sendWebSocketMessage(socket, message);
          } catch (error) {
            console.error(`❌ ${this.name}: Failed to send queued message:`, error);
          }
        }
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
      this.connected = false;

      // Only log on FIRST disconnect, not during reconnection attempts
      if (!this.isReconnecting && this.reconnectAttempt === 0) {
        if (event.code === 1006) {
          verbose() && console.log(`🔴 ${this.name || 'websocket'}: Connection lost (timeout/abnormal close)`);
        } else {
          verbose() && console.log(`🔴 ${this.name || 'websocket'}: Connection closed (code: ${event.code})`);
        }
      }

      // Emit DISCONNECTED event (only once - not on every close during reconnection)
      if (this.reconnectAttempt === 0) {
        this.emitTransportEvent('DISCONNECTED', {
          clientId,
          reason: event.code === 1006 ? 'connection_timeout' : 'connection_closed',
          code: event.code
        });
      }

      // Attempt automatic reconnection if not manually disconnected AND not already reconnecting
      if (!this.manualDisconnect && !this.isReconnecting && this.reconnectAttempt < (this.config.reconnectAttempts || 0)) {
        this.isReconnecting = true; // Mark as reconnecting to queue messages
        const delay = this.config.reconnectDelay! * Math.pow(2, this.reconnectAttempt); // Exponential backoff
        this.reconnectAttempt++;

        // Silent during reconnection - no need to log every attempt

        this.reconnectTimeout = setTimeout(async () => {
          await this.attemptReconnect();
        }, delay);
      }
      // Silent when max attempts reached - user already knows from first disconnect log
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
      verbose() && console.log(`🤝 ${this.name || 'websocket'}: Sent session handshake with sessionId: ${this.sessionId}`);
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
    verbose() && console.log(`🤝 ${this.name}: Received session handshake with sessionId: ${payload?.sessionId}`);
  }

  /**
   * Send message through WebSocket with error handling - handles both regular messages and handshakes
   */
  protected sendWebSocketMessage(socket: JTAGUniversalWebSocket, message: any): void {
    if (!socket || socket.readyState !== JTAG_WEBSOCKET_READY_STATE.OPEN) {
      // During reconnection, queue messages instead of throwing errors
      if (this.isReconnecting) {
        this.messageQueue.push(message);
        // Throttle logging to avoid spam (max once per second)
        const now = Date.now();
        if (now - this.lastErrorLog > 1000) {
          verbose() && console.log(`🔄 ${this.name}: Queueing message during reconnection (${this.messageQueue.length} queued)`);
          this.lastErrorLog = now;
        }
        return;
      }

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
   * Emits to BOTH router's eventSystem AND global Events for widget access
   */
  protected emitTransportEvent(eventType: keyof typeof TRANSPORT_EVENTS, data: any): void {
    const eventName = TRANSPORT_EVENTS[eventType];
    const eventSystemType = this.eventSystem?.constructor?.name || 'undefined';
    const eventData = {
      transportType: 'websocket' as const,
      ...data
    };

    // Emit to router's eventSystem (for router internals like ResponseCorrelator)
    if (this.eventSystem) {
      this.eventSystem.emit(eventName, eventData);
      verbose() && console.log(`✅ ${this.name}: Emitted ${eventName} to router eventSystem (${eventSystemType})`);
    }

    // ALSO emit to global Events singleton (for widgets like ContinuumEmoterWidget)
    // TODO ARCHITECTURAL DEBT: Unify event systems - router should use global Events
    Events.emit(eventName, eventData);
    verbose() && console.log(`✅ ${this.name}: Emitted ${eventName} to global Events singleton`);
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
    // Don't spam console OR events when trying to send messages while disconnected - this is NORMAL
    if (context === 'message send' && !this.connected) {
      // Only emit ERROR event ONCE when we first become disconnected
      if (!this.hasEmittedDisconnectError) {
        this.hasEmittedDisconnectError = true;
        this.emitTransportEvent('ERROR', {
          error: error.message,
          context
        });
      }
      // Silently fail subsequent attempts - application layer already knows we're disconnected
      return;
    }

    // Don't spam console during reconnection attempts - user already notified
    if (this.isReconnecting && context === 'connection') {
      // Silent during reconnection polling - no need to log every failed attempt
      return;
    }

    // For connection errors, check if it's a "server down" situation
    const isConnectionFailed = error.message.includes('Connection failed') ||
                              error.message.includes('ECONNREFUSED') ||
                              error.message.includes('connect ECONNREFUSED');

    if (isConnectionFailed && context === 'connection') {
      // Show clear, prominent message for server down instead of technical error spam
      console.error('🚨 SERVER NOT RUNNING');
      console.error('🔍 PROBLEM: No JTAG system is currently running');
      console.error('✅ IMMEDIATE ACTION: Run "npm start" and wait 60 seconds');
      console.error(''); // Empty line for separation
    } else {
      // Only show detailed errors for non-connection failures
      console.error(`❌ ${this.name}: ${context} error:`, error);
    }

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

    // Only log on first connect, not during reconnection attempts
    if (!this.isReconnecting) {
      verbose() && console.log(`🔗 ${this.name}: Connecting to ${url}`);
    }

    return new Promise((resolve, reject) => {
      try {
        this.socket = this.createWebSocket(url);
        const clientId = this.generateClientId('ws_client');
        
        // Set up consistent event handling
        this.setupWebSocketEvents(this.socket, clientId);
        
        // Add Promise resolution/rejection to the consistent event handlers
        const handleOpen = (event: JTAGWebSocketOpenEvent): void => {
          verbose() && console.log(`✅ ${this.name}: Handler compliance enforced by TypeScript`);
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
    // console.debug(`📤 ${this.name}: Sending message to server`);

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
    const wasReconnecting = this.isReconnecting; // Track if this is part of reconnection
    this.manualDisconnect = true; // Prevent auto-reconnect on manual disconnect

    // Clear any pending reconnect timeout
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = undefined;
    }

    if (this.socket) {
      // Only log on manual disconnect, not during reconnection
      if (!wasReconnecting) {
        console.debug(`🔌 ${this.name}: Disconnecting`);
      }
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

    // Mark as non-manual disconnect for reconnection
    this.manualDisconnect = false;

    // Clear any pending reconnect timeout
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = undefined;
    }

    // Disconnect first if still connected
    const wasManualDisconnect = this.manualDisconnect;
    this.manualDisconnect = true; // Temporarily set to prevent auto-reconnect during disconnect
    await this.disconnect();
    this.manualDisconnect = wasManualDisconnect; // Restore

    // Reconnect using stored URL
    await this.connect(this.lastConnectedUrl);
  }

  /**
   * Attempt reconnection with automatic retry logic
   * Called by the reconnect timeout - handles retries recursively
   */
  private async attemptReconnect(): Promise<void> {
    try {
      await this.reconnect();
      verbose() && console.log(`✅ ${this.name || 'websocket'}: Reconnection successful`);
      this.reconnectAttempt = 0; // Reset on successful reconnect
    } catch (error) {
      // Only log on FIRST failure, then stay silent during polling
      if (this.reconnectAttempt === 0) {
        verbose() && console.log(`🔴 ${this.name || 'websocket'}: Connection lost, attempting to reconnect...`);
      }

      this.isReconnecting = false; // Reset to stop queueing messages

      // Schedule another attempt
      this.isReconnecting = true; // Re-enable queueing for next attempt

      // Fast retries for first 3 attempts (1s, 2s, 4s), then slow persistent polling (10s)
      let delay: number;
      if (this.reconnectAttempt < (this.config.reconnectAttempts || 0)) {
        delay = this.config.reconnectDelay! * Math.pow(2, this.reconnectAttempt);
        this.reconnectAttempt++;
        // Silent during fast retries - user already knows we're reconnecting
      } else {
        // After fast attempts, keep trying every 10 seconds until server comes back
        delay = 10000; // 10 seconds
        // Silent during polling - no need to spam console every 10s
      }

      this.reconnectTimeout = setTimeout(async () => {
        await this.attemptReconnect(); // Recursive retry - will keep trying forever
      }, delay);
    }
  }
}