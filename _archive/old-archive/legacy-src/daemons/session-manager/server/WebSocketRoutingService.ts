/**
 * WebSocket Routing Service - Server Implementation
 * 
 * EXTRACTED CROSS-CUTTING CONCERN: WebSocket message routing and connection management
 * 
 * This service handles all WebSocket-related operations that were previously
 * embedded in the SessionManagerDaemon. It provides clean separation of concerns
 * between session management and WebSocket communication.
 */

import { SystemEventType } from '../../base/EventTypes';
import { DAEMON_EVENT_BUS } from '../../base/DaemonEventBus';
import { MESSAGE_HANDLER_REGISTRY } from '../../../integrations/websocket/core/MessageHandlerRegistry';
import { SendToSessionHandler } from '../handlers/SendToSessionHandler';
import { RemoteExecutionHandler } from '../handlers/RemoteExecutionHandler';
import { DAEMON_REGISTRY } from '../../base/DaemonRegistry';
import { 
  IWebSocketRoutingService, 
  WebSocketRoutingConfig, 
  WebSocketConnectionMetadata,
  WebSocketRoutingStatus 
} from '../shared/WebSocketRoutingTypes';

export class WebSocketRoutingService implements IWebSocketRoutingService {
  private config: WebSocketRoutingConfig;
  private remoteExecutionHandler?: RemoteExecutionHandler;
  private isInitialized = false;
  private handlerCount = 0;
  private activeConnections = 0;

  constructor(config: WebSocketRoutingConfig) {
    this.config = config;
  }

  /**
   * Initialize WebSocket routing service
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      return;
    }

    this.config.logger.log('🔌 Initializing WebSocket routing service...');

    // Register WebSocket connection event handlers
    await this.registerConnectionEventHandlers();

    // Register message handlers for session-related WebSocket messages
    await this.registerMessageHandlers();

    this.isInitialized = true;
    this.config.logger.log('✅ WebSocket routing service initialized');
  }

  /**
   * Register WebSocket connection and disconnection event handlers
   */
  private async registerConnectionEventHandlers(): Promise<void> {
    // Listen for WebSocket connections with proper typing
    DAEMON_EVENT_BUS.onEvent(SystemEventType.WEBSOCKET_CONNECTION_ESTABLISHED, async (event) => {
      this.activeConnections++;
      await this.handleWebSocketConnection(event.connectionId, event.metadata);
    });

    DAEMON_EVENT_BUS.onEvent(SystemEventType.WEBSOCKET_CONNECTION_CLOSED, async (event) => {
      this.activeConnections = Math.max(0, this.activeConnections - 1);
      await this.handleWebSocketDisconnection(event.connectionId, event.reason);
    });

    this.config.logger.log('🔌 WebSocket connection event handlers registered');
  }

  /**
   * Register with WebSocket daemon - called by startup system
   */
  async registerWithWebSocketDaemon(webSocketDaemon: any): Promise<void> {
    this.config.logger.log('📋 Registering session handlers with WebSocket daemon');

    try {
      // Create session message handler with access to connection mapping
      const sendToSessionHandler = new SendToSessionHandler(
        webSocketDaemon.getConnectionSessions(),
        webSocketDaemon.sendToConnectionById.bind(webSocketDaemon)
      );

      // Create remote execution handler for request-response WebSocket communication
      this.remoteExecutionHandler = new RemoteExecutionHandler(
        webSocketDaemon.getConnectionSessions(),
        webSocketDaemon.sendToConnectionById.bind(webSocketDaemon),
        (correlationId: string, _response: any) => {
          // This will be called when responses arrive
          this.config.logger.log(`📨 Remote execution response received: ${correlationId}`);
        }
      );

      // Register handlers for session-related messages
      MESSAGE_HANDLER_REGISTRY.registerHandler('send_to_session', sendToSessionHandler, 'websocket-routing-service');
      MESSAGE_HANDLER_REGISTRY.registerHandler('remote_execution', this.remoteExecutionHandler, 'websocket-routing-service');

      // Register handler for remote execution responses from browser
      MESSAGE_HANDLER_REGISTRY.registerHandler('remote_execution_response', {
        priority: 100,
        handle: async (data: any) => {
          if (this.remoteExecutionHandler && data.correlationId) {
            this.remoteExecutionHandler.handleResponse(data.correlationId, data);
            return { success: true };
          }
          return { success: false, error: 'No remote execution handler or correlation ID' };
        }
      }, 'websocket-routing-service');

      this.handlerCount = 3; // Track registered handlers
      this.config.logger.log('✅ Registered session message handlers with WebSocket daemon');
    } catch (error) {
      this.config.logger.log(`❌ Failed to register message handlers: ${error}`, 'error');
      throw error;
    }
  }

  /**
   * Register message handlers with WebSocket daemon (legacy compatibility)
   */
  private async registerMessageHandlers(): Promise<void> {
    try {
      // Get WebSocket daemon to access connection methods
      const webSocketDaemon = await this.getWebSocketDaemon();

      if (webSocketDaemon) {
        // Create session message handler with access to connection mapping
        const sendToSessionHandler = new SendToSessionHandler(
          webSocketDaemon.getConnectionSessions(),
          webSocketDaemon.sendToConnectionById.bind(webSocketDaemon)
        );

        // Register handlers for session-related messages
        MESSAGE_HANDLER_REGISTRY.registerHandler('send_to_session', sendToSessionHandler, 'websocket-routing-service');
        this.handlerCount++;

        this.config.logger.log('📋 Registered session message handlers with WebSocket daemon');
      } else {
        this.config.logger.log('⚠️ WebSocket daemon not found - session handlers not registered');
      }
    } catch (error) {
      this.config.logger.log(`❌ Failed to register message handlers: ${error}`, 'error');
      throw error;
    }
  }

  /**
   * Get WebSocket daemon instance using daemon discovery
   */
  private async getWebSocketDaemon(): Promise<any> {
    try {
      // Debug: List all registered daemons
      const allDaemons = DAEMON_REGISTRY.getAllDaemons();
      this.config.logger.log(`🔍 Looking for WebSocket daemon among ${allDaemons.length} daemons`);

      const webSocketDaemon = DAEMON_REGISTRY.findDaemon('websocket-server');

      if (webSocketDaemon) {
        this.config.logger.log('✅ Found WebSocket daemon via registry');
        return webSocketDaemon;
      } else {
        // Try waiting for it to register (in case of startup ordering)
        this.config.logger.log('⏳ Waiting for WebSocket daemon to register...');
        const daemon = await DAEMON_REGISTRY.waitForDaemon('websocket-server', 3000);

        if (daemon) {
          this.config.logger.log('✅ WebSocket daemon registered after waiting');
          return daemon;
        } else {
          this.config.logger.log('❌ WebSocket daemon not found after timeout');
          // Debug: Check the old registry too
          const { DAEMON_REGISTRY: OLD_REGISTRY } = await import('../../base/BaseDaemon');
          const webSocketDaemonOld = OLD_REGISTRY.get('websocket-server');
          if (webSocketDaemonOld) {
            this.config.logger.log('✅ Found WebSocket daemon in old registry, using it');
            return webSocketDaemonOld;
          } else {
            this.config.logger.log('❌ WebSocket daemon not in old registry either');
            this.config.logger.log(`🔍 Old registry has: ${Array.from(OLD_REGISTRY.keys()).join(', ')}`);
          }
          return null;
        }
      }
    } catch (error) {
      this.config.logger.log(`❌ Error finding WebSocket daemon: ${error}`, 'error');
      return null;
    }
  }

  /**
   * Handle new WebSocket connection
   */
  private async handleWebSocketConnection(
    connectionId: string,
    metadata: WebSocketConnectionMetadata
  ): Promise<void> {
    try {
      this.config.logger.log(`🔌 Handling WebSocket connection: ${connectionId}`);
      this.config.logger.log(`🚨🚨🚨 SCREENSHOT DEBUG VARIANT-B: About to handle WebSocket connection ${connectionId} 🚨🚨🚨`);

      // Determine session type based on connection metadata
      const sessionType = this.determineSessionType(metadata);

      // TODO: Delegate session creation to SessionManagerDaemon
      // For now, just log the connection
      this.config.logger.log(`📋 Connection ${connectionId} identified as ${sessionType} session`);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.config.logger.log(`❌ Failed to handle WebSocket connection: ${errorMessage}`, 'error');
    }
  }

  /**
   * Handle WebSocket disconnection
   */
  private async handleWebSocketDisconnection(connectionId: string, reason?: string): Promise<void> {
    this.config.logger.log(`🔌 WebSocket disconnected: ${connectionId}${reason ? ` (${reason})` : ''}`);
    // Sessions persist beyond connections, so we just log for now
    // Could implement connection tracking per session if needed
  }

  /**
   * Determine session type based on connection metadata
   */
  determineSessionType(metadata: WebSocketConnectionMetadata): string {
    // This is simplified logic - in practice, this would need to analyze the metadata
    // to determine if it's a portal, validation, user, or persona session
    if (metadata.userAgent.includes('git-hook')) {
      return 'validation';
    }
    if (metadata.url.includes('portal')) {
      return 'portal';
    }
    return 'user';
  }

  /**
   * Send message to connection (delegated from SessionManagerDaemon)
   */
  async sendToConnection(connectionId: string, message: any): Promise<void> {
    try {
      const webSocketDaemon = await this.getWebSocketDaemon();
      if (webSocketDaemon && webSocketDaemon.sendToConnectionById) {
        await webSocketDaemon.sendToConnectionById(connectionId, message);
      } else {
        this.config.logger.log(`❌ Cannot send message to connection ${connectionId} - WebSocket daemon not available`, 'error');
      }
    } catch (error) {
      this.config.logger.log(`❌ Failed to send message to connection ${connectionId}: ${error}`, 'error');
      throw error;
    }
  }

  /**
   * Get service status
   */
  getStatus(): WebSocketRoutingStatus {
    return {
      initialized: this.isInitialized,
      webSocketDaemonConnected: this.handlerCount > 0,
      handlersRegistered: this.handlerCount,
      connectionsActive: this.activeConnections
    };
  }

  /**
   * Cleanup WebSocket routing service
   */
  async cleanup(): Promise<void> {
    if (this.remoteExecutionHandler) {
      // Cleanup remote execution handler if needed
      // Use delete to avoid exactOptionalPropertyTypes error
      delete this.remoteExecutionHandler;
    }

    this.handlerCount = 0;
    this.activeConnections = 0;
    this.isInitialized = false;
    this.config.logger.log('🧹 WebSocket routing service cleaned up');
  }
}