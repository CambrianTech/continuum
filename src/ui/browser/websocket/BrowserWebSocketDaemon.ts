/**
 * BrowserWebSocketDaemon - WebSocket connection management for browser daemon system
 * 
 * Extracts WebSocket connection, message handling, and reconnection logic from 
 * monolithic continuum-browser.ts into modular daemon architecture.
 * 
 * Responsibilities:
 * - WebSocket connection establishment and management
 * - Connection state management and monitoring
 * - Message sending and receiving with queueing
 * - Automatic reconnection with exponential backoff
 * - Session initialization and establishment
 * - Error handling and connection recovery
 * 
 * Phase 3 of browser daemon migration (15% of monolithic code)
 */

import { BaseBrowserDaemon, BrowserDaemonMessage, BrowserDaemonResponse } from '../base/BaseBrowserDaemon';
import { 
  WebSocketMessageType,
  BaseMessage,
  ClientInitMessage,
  ProtocolValidator,
  PROTOCOL_DEFAULTS
} from '../../../types/shared/CommunicationProtocol';
import { WEBSOCKET_MESSAGES, MESSAGE_TYPE_ARRAYS } from '../types/BrowserDaemonConstants';

export interface WebSocketConnectionState {
  state: 'connecting' | 'connected' | 'disconnected' | 'error';
  clientId: string | null;
  sessionId: string | null;
  reconnectAttempts: number;
  lastError: string | null;
}

// Use shared protocol message type instead of local interface
export type WebSocketMessage = BaseMessage;

export interface ConnectionOptions {
  wsUrl?: string;
  maxReconnectAttempts?: number;
  reconnectDelay?: number;
  connectionTimeout?: number;
}

export class BrowserWebSocketDaemon extends BaseBrowserDaemon {
  private ws: WebSocket | null = null;
  private connectionState: WebSocketConnectionState = {
    state: 'disconnected',
    clientId: null,
    sessionId: null,
    reconnectAttempts: 0,
    lastError: null
  };
  
  private eventHandlers = new Map<string, ((data: any) => void)[]>();
  private messageQueue: WebSocketMessage[] = [];
  private pendingRequests = new Map<string, { resolve: (value: any) => void; reject: (reason: any) => void }>();
  
  // Configuration
  private readonly DEFAULT_WS_URL = 'ws://localhost:9000';
  private readonly maxReconnectAttempts: number;
  private readonly reconnectDelay: number;
  private readonly connectionTimeout: number;

  constructor(options: ConnectionOptions = {}) {
    super('BrowserWebSocketDaemon');
    
    // Use shared protocol defaults
    this.maxReconnectAttempts = options.maxReconnectAttempts ?? PROTOCOL_DEFAULTS.maxReconnectAttempts;
    this.reconnectDelay = options.reconnectDelay ?? PROTOCOL_DEFAULTS.reconnectDelay;
    this.connectionTimeout = options.connectionTimeout ?? 5000; // Not in shared defaults yet
  }

  getMessageTypes(): WebSocketMessageType[] {
    return MESSAGE_TYPE_ARRAYS.websocket as WebSocketMessageType[];
  }

  async handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    try {
      switch (message.type as WebSocketMessageType) {
        case WEBSOCKET_MESSAGES.CONNECT:
          return await this.handleConnect(message.data?.wsUrl || this.DEFAULT_WS_URL);
          
        case WEBSOCKET_MESSAGES.DISCONNECT:
          return this.handleDisconnect();
          
        case WEBSOCKET_MESSAGES.SEND:
          return this.handleSendMessage(message.data);
          
        case WEBSOCKET_MESSAGES.STATUS:
          return this.handleGetStatus();
          
        case WEBSOCKET_MESSAGES.SUBSCRIBE:
          return this.handleSubscribe(message.data?.event, message.data?.handler);
          
        case WEBSOCKET_MESSAGES.UNSUBSCRIBE:
          return this.handleUnsubscribe(message.data?.event, message.data?.handler);
          
        case WEBSOCKET_MESSAGES.EXECUTE_COMMAND:
          return await this.handleExecuteCommand(message.data?.command, message.data?.params);
          
        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`,
            timestamp: new Date().toISOString()
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Error handling message ${message.type}: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage,
        timestamp: new Date().toISOString()
      };
    }
  }

  protected async onStart(): Promise<void> {
    this.log('🚀 WebSocket daemon starting...', 'info');
    
    // Auto-connect on startup if configured
    if (this.shouldAutoConnect()) {
      try {
        await this.connect();
      } catch (error) {
        this.log(`⚠️ Auto-connect failed: ${error instanceof Error ? error.message : String(error)}`, 'warn');
      }
    }
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 WebSocket daemon stopping...', 'info');
    this.disconnect();
  }

  // Public API methods
  async connect(wsUrl: string = this.DEFAULT_WS_URL): Promise<void> {
    if (this.connectionState.state === 'connecting' || this.connectionState.state === 'connected') {
      return;
    }

    this.log(`🌐 Connecting to ${wsUrl}...`, 'info');
    this.connectionState.state = 'connecting';
    this.connectionState.lastError = null;
    this.emit('websocket:connecting', { wsUrl });

    return new Promise((resolve, reject) => {
      try {
        this.ws = new WebSocket(wsUrl);

        const connectionTimeout = setTimeout(() => {
          this.log('❌ Connection timeout', 'error');
          this.connectionState.state = 'error';
          this.connectionState.lastError = 'Connection timeout';
          this.emit('websocket:error', { error: 'Connection timeout' });
          reject(new Error('WebSocket connection timeout'));
        }, this.connectionTimeout);

        this.ws.onopen = async () => {
          clearTimeout(connectionTimeout);
          this.log('✅ WebSocket connection established', 'info');
          this.connectionState.state = 'connected';
          this.connectionState.reconnectAttempts = 0;
          
          // Send client initialization using shared protocol
          const initMessage: ClientInitMessage = {
            type: 'client_init',
            timestamp: new Date().toISOString(),
            data: {
              userAgent: navigator.userAgent,
              url: window.location.href,
              version: this.getVersion(),
              capabilities: ['console_forwarding', 'command_execution', 'health_monitoring']
            }
          };
          this.sendWebSocketMessage(initMessage);

          // Automatically establish session
          await this.establishSession();

          this.emit('websocket:connected', { clientId: this.connectionState.clientId });
          resolve();
        };

        this.ws.onmessage = (event) => {
          this.handleIncomingMessage(event);
        };

        this.ws.onclose = (event) => {
          clearTimeout(connectionTimeout);
          this.log(`🔌 Connection closed (${event.code}: ${event.reason})`, 'info');
          this.connectionState.state = 'disconnected';
          this.emit('websocket:disconnected', { code: event.code, reason: event.reason });
          
          // Attempt reconnection
          this.attemptReconnect();
        };

        this.ws.onerror = (error) => {
          clearTimeout(connectionTimeout);
          this.log(`❌ WebSocket error: ${error}`, 'error');
          this.connectionState.state = 'error';
          this.connectionState.lastError = String(error);
          this.emit('websocket:error', { error });
          reject(error);
        };

      } catch (error) {
        this.connectionState.state = 'error';
        this.connectionState.lastError = error instanceof Error ? error.message : String(error);
        this.emit('websocket:error', { error });
        reject(error);
      }
    });
  }

  disconnect(): void {
    if (this.ws) {
      this.log('🔌 Disconnecting...', 'info');
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
    this.connectionState.state = 'disconnected';
    this.connectionState.clientId = null;
    this.connectionState.sessionId = null;
  }

  isConnected(): boolean {
    return this.connectionState.state === 'connected' && this.ws?.readyState === WebSocket.OPEN;
  }

  getConnectionState(): WebSocketConnectionState {
    return { ...this.connectionState };
  }

  async executeCommand(command: string, params?: any): Promise<any> {
    return new Promise((resolve, reject) => {
      if (!this.isConnected()) {
        reject(new Error('WebSocket not connected'));
        return;
      }

      const requestId = this.generateRequestId();
      this.pendingRequests.set(requestId, { resolve, reject });

      // Set timeout for request
      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          this.pendingRequests.delete(requestId);
          reject(new Error(`Command '${command}' timed out`));
        }
      }, 30000); // 30 second timeout

      this.sendWebSocketMessage({
        type: 'execute_command',
        timestamp: new Date().toISOString(),
        data: {
          command,
          params: typeof params === 'string' ? params : JSON.stringify(params),
          requestId,
          sessionId: this.connectionState.sessionId
        }
      });
    });
  }

  // Event subscription methods
  on(event: string, handler: (data: any) => void): void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, []);
    }
    this.eventHandlers.get(event)!.push(handler);
  }

  off(event: string, handler?: (data: any) => void): void {
    if (!this.eventHandlers.has(event)) {
      return;
    }

    const handlers = this.eventHandlers.get(event)!;
    if (handler) {
      const index = handlers.indexOf(handler);
      if (index !== -1) {
        handlers.splice(index, 1);
      }
    } else {
      // Remove all handlers for this event
      this.eventHandlers.set(event, []);
    }
  }

  emit(event: string, data?: any): void {
    if (this.eventHandlers.has(event)) {
      const handlers = this.eventHandlers.get(event)!;
      handlers.forEach(handler => {
        try {
          handler(data);
        } catch (error) {
          this.log(`❌ Error in event handler for ${event}: ${error}`, 'error');
        }
      });
    }
  }

  // Override sendMessage to match base class signature
  async sendMessage(_targetDaemon: string, messageType: string, _data: any): Promise<BrowserDaemonResponse> {
    // Route to WebSocket if it's a WebSocket message type
    if (messageType.startsWith('websocket:')) {
      this.sendWebSocketMessage({ type: messageType, timestamp: new Date().toISOString(), data: _data });
      return { success: true, timestamp: new Date().toISOString(), data: { sent: true } };
    }
    
    // Default fallback
    return { success: false, timestamp: new Date().toISOString(), data: { error: 'Unsupported message type' } };
  }

  // Private helper for WebSocket-specific messages
  private sendWebSocketMessage(message: WebSocketMessage): void {
    const fullMessage = {
      ...message,
      timestamp: new Date().toISOString(),
      clientId: this.connectionState.clientId,
      sessionId: this.connectionState.sessionId
    };

    if (this.isConnected() && this.ws) {
      this.ws.send(JSON.stringify(fullMessage));
    } else {
      // Queue message for when connection is ready
      this.messageQueue.push(fullMessage);
    }
  }

  private handleIncomingMessage(event: MessageEvent): void {
    try {
      const rawMessage = JSON.parse(event.data);
      
      // Validate message structure using shared protocol
      if (!ProtocolValidator.isValidBaseMessage(rawMessage)) {
        this.log(`❌ Invalid message structure received`, 'error');
        return;
      }

      const message = rawMessage as BaseMessage;
      
      // Handle connection confirmation
      if (message.type === 'connection_confirmed') {
        this.connectionState.clientId = (message.data as any)?.clientId;
        this.log(`🆔 Client ID assigned: ${this.connectionState.clientId}`, 'info');
        
        // Send queued messages
        this.flushMessageQueue();
        return;
      }

      // Handle session_ready message using shared protocol
      if (message.type === 'session_ready') {
        this.connectionState.sessionId = (message.data as any)?.sessionId;
        this.log(`🎯 Session ready: ${this.connectionState.sessionId}`, 'info');
        this.emit('session:ready', message.data);
        return;
      }

      // Handle command responses using shared protocol
      if (message.type === 'command_response' && message.requestId) {
        const requestId = message.requestId;
        if (this.pendingRequests.has(requestId)) {
          const { resolve } = this.pendingRequests.get(requestId)!;
          this.pendingRequests.delete(requestId);
          resolve(message.data);
          return;
        }
      }

      // Handle command errors using shared protocol
      if (message.type === 'command_error' && message.requestId) {
        const requestId = message.requestId;
        if (this.pendingRequests.has(requestId)) {
          const { reject } = this.pendingRequests.get(requestId)!;
          this.pendingRequests.delete(requestId);
          reject(new Error((message as any).error || 'Command execution failed'));
          return;
        }
      }

      // Emit message for other handlers
      this.emit('message', message);
      this.emit(`message:${message.type}`, message.data);

    } catch (error) {
      this.log(`❌ Error parsing incoming message: ${error}`, 'error');
    }
  }

  private flushMessageQueue(): void {
    while (this.messageQueue.length > 0) {
      const message = this.messageQueue.shift();
      if (message && this.isConnected()) {
        this.sendWebSocketMessage(message);
      }
    }
  }

  private attemptReconnect(): void {
    if (this.connectionState.reconnectAttempts >= this.maxReconnectAttempts) {
      this.log(`❌ Max reconnection attempts reached (${this.maxReconnectAttempts})`, 'error');
      this.connectionState.state = 'error';
      this.connectionState.lastError = 'Max reconnection attempts reached';
      this.emit('websocket:max_reconnect_attempts');
      return;
    }

    const delay = this.reconnectDelay * Math.pow(2, this.connectionState.reconnectAttempts);
    this.connectionState.reconnectAttempts++;
    
    this.log(`🔄 Reconnecting in ${delay}ms (attempt ${this.connectionState.reconnectAttempts}/${this.maxReconnectAttempts})`, 'info');
    
    setTimeout(() => {
      if (this.connectionState.state === 'disconnected') {
        this.connect().catch(error => {
          this.log(`❌ Reconnection failed: ${error instanceof Error ? error.message : String(error)}`, 'error');
        });
      }
    }, delay);
  }

  private async establishSession(): Promise<void> {
    try {
      this.log('🔌 Establishing session...', 'info');
      await this.executeCommand('connect', {
        sessionType: 'development',
        owner: 'shared',
        forceNew: false
      });
      this.log('✅ Session established', 'info');
    } catch (error) {
      this.log(`❌ Failed to establish session: ${error instanceof Error ? error.message : String(error)}`, 'error');
    }
  }

  private generateRequestId(): string {
    return `req_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private shouldAutoConnect(): boolean {
    // Auto-connect in development environment
    return window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1';
  }

  private getVersion(): string {
    // Get version from package.json embedded in build
    return (window as any).__CONTINUUM_VERSION__ || '0.0.0';
  }

  // Message handler methods for daemon interface
  private async handleConnect(wsUrl: string): Promise<BrowserDaemonResponse> {
    try {
      await this.connect(wsUrl);
      return {
        success: true,
        data: {
          connectionState: this.getConnectionState(),
          wsUrl
        },
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }

  private handleDisconnect(): BrowserDaemonResponse {
    this.disconnect();
    return {
      success: true,
      data: {
        connectionState: this.getConnectionState()
      },
      timestamp: new Date().toISOString()
    };
  }

  private handleSendMessage(messageData: any): BrowserDaemonResponse {
    try {
      this.sendWebSocketMessage(messageData);
      return {
        success: true,
        data: { sent: true },
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }

  private handleGetStatus(): BrowserDaemonResponse {
    return {
      success: true,
      data: {
        connectionState: this.getConnectionState(),
        isConnected: this.isConnected(),
        queuedMessages: this.messageQueue.length,
        pendingRequests: this.pendingRequests.size,
        eventHandlers: Array.from(this.eventHandlers.keys())
      },
      timestamp: new Date().toISOString()
    };
  }

  private handleSubscribe(event: string, handler: (data: any) => void): BrowserDaemonResponse {
    try {
      this.on(event, handler);
      return {
        success: true,
        data: { event, subscribed: true },
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }

  private handleUnsubscribe(event: string, handler?: (data: any) => void): BrowserDaemonResponse {
    try {
      this.off(event, handler);
      return {
        success: true,
        data: { event, unsubscribed: true },
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }

  private async handleExecuteCommand(command: string, params?: any): Promise<BrowserDaemonResponse> {
    try {
      const result = await this.executeCommand(command, params);
      return {
        success: true,
        data: { result },
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString()
      };
    }
  }
}