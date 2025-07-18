/**
 * WebSocket Routing Client - Client Implementation
 * 
 * EXTRACTED CROSS-CUTTING CONCERN: Client-side WebSocket routing and communication
 * 
 * This client handles WebSocket operations from the browser side, following the same
 * patterns as the server-side WebSocket routing service but adapted for browser context.
 */

import { 
  IWebSocketRoutingService, 
  WebSocketRoutingConfig, 
  WebSocketConnectionMetadata,
  WebSocketRoutingStatus 
} from '../shared/WebSocketRoutingTypes';

export class WebSocketRoutingClient implements IWebSocketRoutingService {
  private config: WebSocketRoutingConfig;
  private ws: WebSocket | null = null;
  private isInitialized = false;
  private connectionId: string | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 3;
  private reconnectDelay = 1000;

  constructor(config: WebSocketRoutingConfig) {
    this.config = config;
  }

  /**
   * Initialize WebSocket routing client
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      return;
    }

    this.config.logger.log('🔌 Initializing WebSocket routing client...');
    
    // Initialize WebSocket connection
    await this.initializeWebSocket();

    this.isInitialized = true;
    this.config.logger.log('✅ WebSocket routing client initialized');
  }

  /**
   * Initialize WebSocket connection
   */
  private async initializeWebSocket(): Promise<void> {
    try {
      this.ws = new WebSocket('ws://localhost:9000');
      
      this.ws.onopen = () => {
        this.config.logger.log('🔌 WebSocket connected');
        this.reconnectAttempts = 0;
        
        // Send client init
        this.sendClientInit();
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event);
      };

      this.ws.onclose = () => {
        this.config.logger.log('🔌 WebSocket disconnected');
        this.handleDisconnection();
      };

      this.ws.onerror = (error) => {
        this.config.logger.log(`❌ WebSocket error: ${error}`, 'error');
        this.handleDisconnection();
      };

    } catch (error) {
      this.config.logger.log(`❌ Failed to initialize WebSocket: ${error}`, 'error');
      throw error;
    }
  }

  /**
   * Send client initialization message
   */
  private sendClientInit(): void {
    const clientInitData = {
      userAgent: navigator.userAgent,
      url: window.location.href,
      timestamp: new Date().toISOString(),
      version: this.config.context.version || '1.0.0',
      mode: 'join_existing'
    };

    this.sendMessage({
      type: 'client_init',
      data: clientInitData,
      timestamp: new Date().toISOString()
    });
  }

  /**
   * Handle incoming WebSocket messages
   */
  private async handleMessage(event: MessageEvent): Promise<void> {
    try {
      const message = JSON.parse(event.data);
      
      if (message.type === 'connection_confirmed') {
        this.connectionId = message.data?.clientId;
        if (this.connectionId) {
          this.config.logger.log(`🔌 Client ID received: ${this.connectionId}`);
        }
        return;
      }

      if (message.type === 'session_ready') {
        const sessionId = message.data?.sessionId;
        if (sessionId) {
          this.config.logger.log(`🎯 Session ready: ${sessionId}`);
        }
        return;
      }

      // Handle other message types
      this.config.logger.log(`📨 Received message: ${message.type}`);
      
    } catch (error) {
      this.config.logger.log(`❌ Error handling message: ${error}`, 'error');
    }
  }

  /**
   * Handle WebSocket disconnection
   */
  private handleDisconnection(): void {
    this.connectionId = null;
    
    if (this.reconnectAttempts < this.maxReconnectAttempts) {
      this.reconnectAttempts++;
      this.config.logger.log(`🔄 Attempting reconnect (${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
      
      setTimeout(() => {
        this.initializeWebSocket().catch(error => {
          this.config.logger.log(`❌ Reconnection failed: ${error}`, 'error');
        });
      }, this.reconnectDelay * this.reconnectAttempts);
    } else {
      this.config.logger.log('❌ Max reconnection attempts reached', 'error');
    }
  }

  /**
   * Send message through WebSocket
   */
  private sendMessage(message: any): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else {
      this.config.logger.log('❌ Cannot send message - WebSocket not connected', 'error');
    }
  }

  /**
   * Register with WebSocket daemon (client-side implementation)
   */
  async registerWithWebSocketDaemon(_webSocketDaemon: any): Promise<void> {
    // Client-side doesn't register directly with daemon
    // This is handled through the WebSocket connection
    this.config.logger.log('📋 Client-side WebSocket registration handled through connection');
  }

  /**
   * Send message to connection (client-side sends to server)
   */
  async sendToConnection(connectionId: string, message: any): Promise<void> {
    // Client sends messages to server, not to specific connections
    this.sendMessage({
      type: 'client_message',
      targetConnection: connectionId,
      data: message,
      timestamp: new Date().toISOString()
    });
  }

  /**
   * Determine session type (client-side implementation)
   */
  determineSessionType(_metadata: WebSocketConnectionMetadata): string {
    // Client-side session type determination
    if (window.location.search.includes('git-hook')) {
      return 'validation';
    }
    if (window.location.pathname.includes('portal')) {
      return 'portal';
    }
    return 'user';
  }

  /**
   * Get client status
   */
  getStatus(): WebSocketRoutingStatus {
    return {
      initialized: this.isInitialized,
      webSocketDaemonConnected: this.ws?.readyState === WebSocket.OPEN,
      handlersRegistered: 0, // Client doesn't register handlers
      connectionsActive: this.connectionId ? 1 : 0
    };
  }

  /**
   * Get current connection ID
   */
  getConnectionId(): string | null {
    return this.connectionId;
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN && this.connectionId !== null;
  }

  /**
   * Cleanup WebSocket routing client
   */
  async cleanup(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    
    this.connectionId = null;
    this.reconnectAttempts = 0;
    this.isInitialized = false;
    this.config.logger.log('🧹 WebSocket routing client cleaned up');
  }
}