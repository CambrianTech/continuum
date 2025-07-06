/**
 * ConnectionManager - Manages WebSocket connection lifecycle and sessions
 * Extracted from WebSocketDaemon to follow single responsibility principle
 */

import { EventEmitter } from 'events';
import { DaemonRouter } from './DaemonRouter';

export interface ConnectionType {
  type: string;
  needsDevTools: boolean;
  context: string;
}

export class ConnectionManager extends EventEmitter {
  constructor(
    private daemonRouter: DaemonRouter,
    private port: number
  ) {
    super();
  }
  
  /**
   * Handle new WebSocket connection
   */
  async handleNewConnection(connectionId: string, connection: any): Promise<void> {
    try {
      // Identify connection type from metadata
      const connectionType = this.identifyConnectionType(connection);
      
      this.emit('log', {
        level: 'info',
        message: `New ${connectionType.type} connection: ${connectionId}`
      });
      
      // Check if this connection needs a browser session
      if (this.shouldCreateBrowserSession(connectionType)) {
        await this.ensureBrowserSession(connectionId, connectionType);
      }
      
      // Handle DevTools mode for git hooks and portal connections
      if (connectionType.needsDevTools) {
        await this.setupDevToolsMode(connectionId, connectionType);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.emit('log', {
        level: 'error',
        message: `Failed to handle new connection ${connectionId}: ${errorMessage}`
      });
    }
  }
  
  /**
   * Handle connection closure
   */
  async handleConnectionClosed(connectionId: string): Promise<void> {
    try {
      this.emit('log', {
        level: 'info',
        message: `Connection ${connectionId} closed - session cleanup handled by SessionManager`
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.emit('log', {
        level: 'warn',
        message: `Error handling connection closure for ${connectionId}: ${errorMessage}`
      });
    }
  }
  
  /**
   * Identify connection type from metadata
   */
  private identifyConnectionType(connection: any): ConnectionType {
    const userAgent = connection.metadata?.userAgent || '';
    const url = connection.metadata?.url || '';
    
    // Git hook connections
    if (userAgent.includes('git') || url.includes('hook') || url.includes('ci')) {
      return { type: 'git-hook', needsDevTools: true, context: 'automation' };
    }
    
    // Portal connections (development interface)
    if (userAgent.includes('Portal') || url.includes('portal') || url.includes('dev')) {
      return { type: 'portal', needsDevTools: true, context: 'development' };
    }
    
    // Browser-based user connections
    if (userAgent.includes('Chrome') || userAgent.includes('Firefox') || userAgent.includes('Safari')) {
      return { type: 'user-browser', needsDevTools: false, context: 'user' };
    }
    
    // Default to user connection
    return { type: 'user', needsDevTools: false, context: 'user' };
  }
  
  /**
   * Determine if connection type needs a browser session
   */
  private shouldCreateBrowserSession(connectionType: ConnectionType): boolean {
    return ['user', 'user-browser', 'portal'].includes(connectionType.type);
  }
  
  /**
   * Ensure browser session exists for this connection
   */
  private async ensureBrowserSession(connectionId: string, connectionType: ConnectionType): Promise<void> {
    const sessionManager = this.daemonRouter.hasDaemon('session-manager');
    const browserManager = this.daemonRouter.hasDaemon('browser-manager');
    
    if (!sessionManager || !browserManager) {
      this.emit('log', {
        level: 'warn',
        message: `Session or Browser manager not available for ${connectionId}`
      });
      return;
    }
    
    // Create session for this connection
    const sessionRequest = {
      id: `conn-${Date.now()}`,
      from: 'connection-manager',
      to: 'session-manager',
      type: 'create_session',
      timestamp: new Date(),
      data: {
        sessionType: connectionType.context,
        owner: connectionId,
        connectionType: connectionType.type,
        options: {
          autoCleanup: true,
          devtools: connectionType.needsDevTools,
          context: connectionType.context
        }
      }
    };
    
    const sessionResponse = await this.daemonRouter.routeMessage(sessionRequest);
    
    if (sessionResponse.success) {
      const sessionData = sessionResponse.data;
      
      // Request smart browser management
      const browserRequest = {
        id: `browser-${Date.now()}`,
        from: 'connection-manager',
        to: 'browser-manager',
        type: 'create_browser',
        timestamp: new Date(),
        data: {
          sessionId: (sessionData as any).sessionId, // TODO: Add proper session data type
          url: `http://localhost:${this.port}`,
          config: {
            purpose: connectionType.context,
            requirements: {
              devtools: connectionType.needsDevTools,
              isolation: 'dedicated',
              visibility: 'visible',
              persistence: 'session'
            }
          }
        }
      };
      
      await this.daemonRouter.routeMessage(browserRequest);
      
      this.emit('log', {
        level: 'info',
        message: `Browser session created for ${connectionId}`
      });
    }
  }
  
  /**
   * Setup DevTools mode for development connections
   */
  private async setupDevToolsMode(connectionId: string, connectionType: ConnectionType): Promise<void> {
    this.emit('log', {
      level: 'info',
      message: `DevTools mode enabled for ${connectionType.type} connection ${connectionId}`
    });
  }
}