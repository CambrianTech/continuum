/**
 * WebSocket Daemon - Pure Content-Agnostic Router
 * ONLY handles routing and connections - knows NOTHING about content
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol';
import { DaemonType } from '../../daemons/base/DaemonTypes';
import { SystemEventType } from '../../daemons/base/EventTypes';
import { RouteManager } from './core/RouteManager';
import { WebSocketManager } from './core/WebSocketManager';
import { PortManager } from './core/PortManager';
import { createServer } from 'http';

export interface ServerConfig {
  port?: number;
  host?: string;
  maxClients?: number;
}

export class WebSocketDaemon extends BaseDaemon {
  public readonly name = 'websocket-server';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.WEBSOCKET_SERVER;

  private routeManager: RouteManager;
  private wsManager: WebSocketManager;
  private portManager: PortManager;
  private httpServer: any = null;
  private config: Required<ServerConfig>;
  private registeredDaemons = new Map<string, any>();
  // Track connection to session mapping for command context
  private connectionSessions = new Map<string, string>(); // connectionId -> sessionId

  constructor(config: ServerConfig = {}) {
    super();
    
    this.config = {
      port: config.port ?? 9000,
      host: config.host ?? 'localhost',
      maxClients: config.maxClients ?? 100
    };

    this.routeManager = new RouteManager((daemonName, message) => this.sendMessageToDaemon(daemonName, message));
    this.wsManager = new WebSocketManager();
    this.portManager = new PortManager(this.config.port, this.config.host);
    
    // Set up WebSocket event handling
    this.wsManager.onMessage = (connectionId: string, data: any) => {
      this.handleWebSocketMessage(connectionId, data);
    };
    
    this.wsManager.onConnection = (connectionId: string, connection: any) => {
      this.handleNewConnection(connectionId, connection);
    };
    
    this.wsManager.onDisconnection = (connectionId: string) => {
      this.handleConnectionClosed(connectionId);
    };
  }

  protected async onStart(): Promise<void> {
    this.log(`🌐 Starting pure router on ${this.config.host}:${this.config.port}`);
    
    // Listen for session assignments from SessionManagerDaemon
    const { DAEMON_EVENT_BUS } = await import('../../daemons/base/DaemonEventBus');
    DAEMON_EVENT_BUS.onEvent(SystemEventType.SESSION_CREATED, (event) => {
      // We'll track sessions when they send messages
      this.log(`📋 Session created: ${event.sessionId}`);
    });
    
    // Check for existing healthy instance
    if (await this.portManager.isPortHealthy()) {
      this.log(`✅ Healthy instance already running on port ${this.config.port}`);
      return; // Let the existing instance handle everything
    }
    
    // Start HTTP server for routing
    this.httpServer = createServer(async (req, res) => {
      await this.handleHttpRequest(req, res);
    });

    // Start HTTP server with intelligent port conflict handling
    return new Promise<void>((resolve, reject) => {
      this.httpServer.listen(this.config.port, this.config.host, () => {
        this.log(`✅ Pure router ready on ${this.config.host}:${this.config.port}`);
        resolve();
      });
      
      this.httpServer.on('error', async (error: any) => {
        if (error.code === 'EADDRINUSE') {
          this.log(`🔄 Port conflict detected - attempting resolution...`);
          
          if (await this.portManager.resolvePortConflict()) {
            // Retry after cleanup
            setTimeout(() => {
              this.httpServer.listen(this.config.port, this.config.host, () => {
                this.log(`✅ Pure router ready on ${this.config.host}:${this.config.port} (after cleanup)`);
                resolve();
              });
            }, 1000);
          } else {
            this.log(`❌ Could not resolve port conflict`, 'error');
            reject(error);
          }
        } else {
          this.log(`❌ HTTP server failed to start: ${error.message}`, 'error');
          reject(error);
        }
      });
    }).then(async () => {
      // Attach WebSocket server to the HTTP server
      await this.wsManager.start(this.httpServer);
    });
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping router...');
    
    if (this.httpServer) {
      this.httpServer.close();
    }
    
    await this.wsManager.stop();
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'register_daemon':
        return this.handleDaemonRegistration(message.data);
      
      case 'register_http_routes':
        return this.handleRouteRegistration(message.data);
      
      case 'get_status':
        return {
          success: true,
          data: {
            routes: this.routeManager.getRegisteredRoutes(),
            connections: this.wsManager.getConnectionCount(),
            registered_daemons: Array.from(this.registeredDaemons.keys())
          }
        };

      case 'send_to_connection':
        return this.handleSendToConnection(message.data);

      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }

  /**
   * Register daemon with route handlers - CONTENT AGNOSTIC
   */
  public registerDaemon(daemon: any): void {
    this.registeredDaemons.set(daemon.name, daemon);
    
    // Let daemon register its own routes
    if (daemon.registerWithWebSocketDaemon) {
      daemon.registerWithWebSocketDaemon(this);
    }
    
    this.log(`🔌 Registered daemon: ${daemon.name}`);
  }


  private async handleHttpRequest(req: any, res: any): Promise<void> {
    const url = new URL(req.url!, `http://${this.config.host}:${this.config.port}`);
    const pathname = url.pathname;

    try {
      // EARLY ROUTING: Simple endpoints bypass complex command system
      if (pathname === '/api/health') {
        // Direct health check - no command routing needed (supports GET and POST)
        const healthData = {
          status: 'healthy',
          timestamp: new Date().toISOString(),
          daemons: Array.from(this.registeredDaemons.keys()),
          connections: this.wsManager.getConnectionCount()
        };
        
        if (req.method === 'POST') {
          // Handle POST from browser health validation
          let body = '';
          req.on('data', (chunk: any) => body += chunk);
          req.on('end', () => {
            try {
              const clientData = JSON.parse(body);
              this.log(`🏥 Client health report received:`, clientData.source || 'unknown');
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({ ...healthData, clientReport: 'received' }));
            } catch (error) {
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify(healthData));
            }
          });
        } else {
          // Handle GET requests
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify(healthData));
        }
        
        this.log(`✅ Direct health check served (${req.method})`);
        return;
      } else if (pathname === '/api/status') {
        // Direct status check - comprehensive system info
        const statusData = {
          status: 'running',
          timestamp: new Date().toISOString(),
          daemons: Array.from(this.registeredDaemons.keys()).map(name => ({
            name,
            status: 'running' // TODO: Get actual daemon status
          })),
          routes: this.routeManager.getRegisteredRoutes(),
          connections: this.wsManager.getConnectionCount(),
          version: '2.0.0'
        };
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(statusData));
        this.log(`✅ Direct status check served (${req.method})`);
        return;
      } else if (pathname.startsWith('/api/')) {
        // Route other API calls to command daemons
        const handled = await this.routeManager.handleRequest(pathname, req, res);
        if (!handled) {
          res.writeHead(404, { 'Content-Type': 'text/plain' });
          res.end('API endpoint not found');
          this.log(`❌ No API route for: ${pathname}`);
        } else {
          this.log(`✅ API routed: ${pathname}`);
        }
      } else {
        // Route all other requests (files, HTML, etc.) to appropriate daemons
        const handled = await this.routeManager.handleRequest(pathname, req, res);
        if (!handled) {
          res.writeHead(404, { 'Content-Type': 'text/plain' });
          res.end('Not found');
          this.log(`❌ No route handler for: ${pathname}`);
        } else {
          this.log(`✅ Request routed: ${pathname}`);
        }
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : 'No stack trace';
      this.log(`🚨🚨🚨 REQUEST CRASH for ${pathname}:`, 'error');
      this.log(`Error: ${errorMessage}`, 'error');
      this.log(`Stack: ${errorStack}`, 'error');
      
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Internal Server Error');
    }
  }




  private handleWebSocketMessage(connectionId: string, data: any): void {
    try {
      const message = JSON.parse(data.toString());
      
      // 🔍 SESSION DEBUG: Log full incoming message structure
      this.log(`🔍 [SESSION_DEBUG] WebSocket message received:`);
      this.log(`🔍 [SESSION_DEBUG]   type: ${message.type}`);
      this.log(`🔍 [SESSION_DEBUG]   connectionId: ${connectionId}`);
      this.log(`🔍 [SESSION_DEBUG]   message.data: ${JSON.stringify(message.data || {}, null, 2)}`);
      this.log(`🔍 [SESSION_DEBUG]   message.sessionId: ${message.sessionId || 'NOT_FOUND'}`);
      this.log(`🔍 [SESSION_DEBUG]   message.data.sessionId: ${message.data?.sessionId || 'NOT_FOUND'}`);
      this.log(`🔍 [SESSION_DEBUG]   full message keys: ${Object.keys(message).join(', ')}`);
      
      // Route WebSocket messages to appropriate daemon
      // This is pure message routing, no content knowledge
      
      if (message.type === 'execute_command') {
        this.routeCommandToProcessor(connectionId, message).catch(error => {
          this.log(`❌ Failed to route command: ${error}`, 'error');
        });
      } else if (message.type === 'console_log' || message.type === 'browser_console') {
        // Handle browser console messages and write to session browser.log
        this.handleBrowserConsoleMessage(connectionId, message).catch(error => {
          this.log(`❌ Failed to handle console message: ${error}`, 'error');
        });
      } else if (message.type === 'register_http_routes') {
        // Handle route registration from daemons via WebSocket
        const response = this.handleRouteRegistration(message);
        // TODO: Send response back to daemon
        this.log(`📨 Route registration: ${response.success ? 'success' : 'failed'}`);
      } else if (message.to) {
        // Route message to specific daemon if 'to' field is present
        this.log(`📨 Routing message to daemon: ${message.to}`);
        this.routeMessageToDaemon(connectionId, message).catch(error => {
          this.log(`❌ Failed to route message to daemon: ${error}`, 'error');
        });
      } else {
        this.log(`📨 Unknown WebSocket message type: ${message.type}`);
      }
    } catch (error) {
      this.log(`❌ WebSocket message parse error: ${error}`, 'error');
    }
  }

  private async routeCommandToProcessor(connectionId: string, message: any): Promise<void> {
    // Pure routing - find command processor daemon and forward
    const commandProcessor = this.registeredDaemons.get('command-processor');
    
    if (commandProcessor) {
      try {
        // Extract command info from the WebSocket message structure
        const commandData = message.data;
        const commandName = commandData.command;
        const commandParams = commandData.params;
        const requestId = commandData.requestId;
        
        // Get sessionId from connection mapping
        const sessionId = this.connectionSessions.get(connectionId);
        
        // 🔍 SESSION DEBUG: Log sessionId extraction process
        this.log(`🔍 [SESSION_DEBUG] routeCommandToProcessor extraction:`);
        this.log(`🔍 [SESSION_DEBUG]   connectionId: ${connectionId}`);
        this.log(`🔍 [SESSION_DEBUG]   sessionId from mapping: ${sessionId || 'NOT_FOUND'}`);
        this.log(`🔍 [SESSION_DEBUG]   commandName: ${commandName}`);
        
        // Parse parameters (they might be JSON string from browser)
        let parsedParams = {};
        if (commandParams) {
          try {
            parsedParams = typeof commandParams === 'string' ? JSON.parse(commandParams) : commandParams;
          } catch (error) {
            this.log(`⚠️ Failed to parse command parameters: ${error}`, 'warn');
            parsedParams = commandParams;
          }
        }
        
        // Convert WebSocket message to DaemonMessage format
        const daemonMessage = {
          id: `ws-${Date.now()}`,
          from: 'websocket-server',
          to: 'command-processor',
          type: 'command.execute',
          timestamp: new Date(),
          data: {
            command: commandName,
            parameters: parsedParams,
            context: {
              connectionId: connectionId,
              sessionId: sessionId, // Pass session ID to command context
              websocket: {
                registeredDaemons: this.registeredDaemons
              },
              requestId: requestId
            }
          }
        };
        
        // 🔍 SESSION DEBUG: Log final message being sent to CommandProcessor  
        this.log(`🔍 [SESSION_DEBUG] Final daemonMessage to CommandProcessor:`);
        this.log(`🔍 [SESSION_DEBUG]   command: ${commandName}`);
        this.log(`🔍 [SESSION_DEBUG]   context.sessionId: ${daemonMessage.data.context.sessionId || 'NOT_FOUND'}`);
        this.log(`🔍 [SESSION_DEBUG]   context.connectionId: ${daemonMessage.data.context.connectionId}`);
        this.log(`🔄 Routing command "${commandName}" to command processor`);
        
        // Forward to command processor using standard daemon protocol
        const response = await commandProcessor.handleMessage(daemonMessage);
        
        // Send response back to WebSocket client
        const wsResponse = {
          type: 'execute_command_response',
          requestId: requestId,
          command: commandName,
          success: response.success,
          data: response.data,
          error: response.error,
          timestamp: new Date().toISOString()
        };
        
        this.wsManager.sendToConnection(connectionId, wsResponse);
        
        // Special handling for connect command - send session_ready message
        if (commandName === 'connect' && response.success && response.data?.session?.sessionId) {
          const sessionReadyMessage = {
            type: 'session_ready',
            data: {
              sessionId: response.data.session.sessionId,
              sessionType: response.data.session.sessionType || 'development',
              devtools: false,
              logPaths: response.data.session.logs,
              directories: response.data.session.directories
            },
            timestamp: new Date().toISOString()
          };
          
          this.log(`📤 Sending session_ready message with sessionId: ${response.data.session.sessionId}`);
          this.wsManager.sendToConnection(connectionId, sessionReadyMessage);
        }
        
        this.log(`✅ Command "${commandName}" ${response.success ? 'completed' : 'failed'}`);
        
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log(`❌ Error routing command to processor: ${errorMessage}`, 'error');
        
        // Send error response to client
        const errorResponse = {
          type: 'execute_command_response',
          requestId: message.data?.requestId,
          command: message.data?.command || 'unknown',
          success: false,
          error: errorMessage,
          timestamp: new Date().toISOString()
        };
        
        this.wsManager.sendToConnection(connectionId, errorResponse);
      }
    } else {
      this.log('❌ No command processor registered', 'error');
      
      // Send error response to client
      const errorResponse = {
        type: 'execute_command_response',
        requestId: message.data?.requestId,
        command: message.data?.command || 'unknown',
        success: false,
        error: 'Command processor daemon not available',
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, errorResponse);
    }
  }

  private handleDaemonRegistration(_data: any): DaemonResponse {
    // Handle external daemon registration
    return {
      success: true,
      data: { registered: true }
    };
  }

  private handleRouteRegistration(data: any): DaemonResponse {
    const { daemon, routes } = data;
    
    try {
      // Register each route with the daemon handler using new message-based format
      for (const route of routes) {
        this.routeManager.registerRoute(route.pattern, daemon, route.handler);
      }
      
      this.log(`🔌 Registered ${routes.length} routes for daemon: ${daemon}`);
      
      return {
        success: true,
        data: { routes_registered: routes.length }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Route registration failed: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Register a route handler - called from main.ts
   */
  registerRouteHandler(path: string, daemonName: string, handler: string): void {
    this.routeManager.registerRoute(path, daemonName, handler);
    this.log(`🔗 Registered route: ${path} → ${daemonName}::${handler}`);
  }

  private async sendMessageToDaemon(daemonName: string, message: any): Promise<any> {
    // In a proper implementation, this would connect to the daemon via WebSocket
    // For now, we'll use a simple direct call if the daemon is registered
    const daemon = this.registeredDaemons.get(daemonName);
    
    this.log(`🔍 DEBUG: Looking for daemon '${daemonName}', found: ${!!daemon}, hasHandleMessage: ${daemon?.handleMessage ? 'yes' : 'no'}`);
    this.log(`🔍 DEBUG: Registered daemons: ${Array.from(this.registeredDaemons.keys()).join(', ')}`);
    
    if (daemon && daemon.handleMessage) {
      // Inject websocket context for daemon access
      const messageWithContext = {
        ...message,
        context: {
          ...message.context,
          websocket: {
            registeredDaemons: this.registeredDaemons
          }
        }
      };
      
      this.log(`🔍 DEBUG: Calling handleMessage with: ${JSON.stringify(messageWithContext)}`, 'debug');
      try {
        const result = await daemon.handleMessage(messageWithContext);
        this.log(`🔍 DEBUG: Got response: ${JSON.stringify(result)}`, 'debug');
        return result;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log(`❌ DEBUG: handleMessage threw error: ${errorMessage}`, 'error');
        throw error;
      }
    } else {
      this.log(`❌ DEBUG: Daemon ${daemonName} not found or doesn't support messaging`, 'error');
      throw new Error(`Daemon ${daemonName} not found or doesn't support messaging`);
    }
  }

  /**
   * Route a WebSocket message to a specific daemon
   */
  private async routeMessageToDaemon(connectionId: string, message: any): Promise<void> {
    try {
      const targetDaemon = message.to;
      const daemon = this.registeredDaemons.get(targetDaemon);
      
      if (!daemon) {
        this.log(`❌ Target daemon not found: ${targetDaemon}`, 'error');
        
        // Send error response back to client
        const errorResponse = {
          id: message.id,
          from: targetDaemon,
          to: message.from,
          type: 'error',
          success: false,
          error: `Daemon ${targetDaemon} not found`,
          timestamp: new Date().toISOString()
        };
        
        this.wsManager.sendToConnection(connectionId, errorResponse);
        return;
      }
      
      // Forward message to daemon
      const response = await daemon.handleMessage(message);
      
      // Send response back to WebSocket client
      const wsResponse = {
        id: message.id,
        from: targetDaemon,
        to: message.from,
        type: response.success ? 'response' : 'error',
        success: response.success,
        data: response.data,
        error: response.error,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, wsResponse);
      
      this.log(`✅ Message routed to ${targetDaemon}: ${response.success ? 'success' : 'failed'}`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Error routing message to daemon: ${errorMessage}`, 'error');
      
      // Send error response to client
      const errorResponse = {
        id: message.id,
        from: message.to,
        to: message.from,
        type: 'error',
        success: false,
        error: errorMessage,
        timestamp: new Date().toISOString()
      };
      
      this.wsManager.sendToConnection(connectionId, errorResponse);
    }
  }

  /**
   * Handle new WebSocket connection - PURE ROUTING ONLY
   */
  private async handleNewConnection(connectionId: string, connection: any): Promise<void> {
    try {
      // PURE ROUTER - no session management, just log the connection
      this.log(`🔌 New WebSocket connection: ${connectionId}`);
      
      // Extract connection metadata
      const metadata = {
        userAgent: connection.metadata?.userAgent || 'unknown',
        url: connection.metadata?.url || '/',
        headers: connection.metadata?.headers || {}
      };
      
      // Emit event for other daemons (especially SessionManagerDaemon)
      const { DAEMON_EVENT_BUS } = await import('../../daemons/base/DaemonEventBus');
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.WEBSOCKET_CONNECTION_ESTABLISHED, {
        timestamp: new Date(),
        source: this.name,
        connectionId,
        metadata
      });
      
      // Connection is ready for message routing
      this.log(`✅ Connection ready for routing - event emitted`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to handle new connection ${connectionId}: ${errorMessage}`, 'error');
    }
  }

  // SESSION MANAGEMENT REMOVED - WebSocketDaemon is now a pure router
  // All session management is handled by SessionManagerDaemon

  // REMOVED: _createUniversalSession - disabled during session system cleanup
  /*
  private async _createUniversalSession(connectionId: string, connectionType: any): Promise<any> {
    try {
      const sessionManager = this.registeredDaemons.get('session-manager');
      if (!sessionManager) {
        this.log('⚠️ Session manager not available for universal session creation', 'warn');
        return null;
      }

      // Use the same shared session logic as ConnectCommand
      // This ensures we reuse existing shared sessions instead of creating new ones
      const sessionParams = this.getSessionParamsForConnection(connectionType, connectionId);
      
      // Use handleConnect to get shared session behavior
      const connectRequest = {
        source: 'websocket-daemon',
        sessionPreference: 'current', // Use shared session by default
        sessionType: sessionParams.type,
        owner: sessionParams.owner,
        capabilities: ['browser', 'commands', 'screenshots'],
        context: 'development'
      };

      const response = await sessionManager.handleConnect(connectRequest);
      
      if (!response.success) {
        this.log(`❌ Universal session creation failed: ${response.error}`, 'error');
        return null;
      }

      const sessionData = response.data;
      this.log(`✅ Universal session ${sessionData.action}: ${sessionData.sessionId} for ${connectionType.type} connection`);
      
      // Enable session logging for ALL daemons - this is critical for JTAG observability
      const serverLogPath = sessionData.logs.server;
      this.enableSessionLoggingForAllDaemons(serverLogPath, sessionData.sessionId);
      
      // Store the connection-to-session mapping for browser console logging
      // TODO: Re-enable session tracking when session system is fixed
      // this.connectionSessions.set(connectionId, sessionData.sessionId);
      
      return {
        sessionId: sessionData.sessionId,
        type: 'development', // from handleConnect response
        owner: 'shared', // from handleConnect response
        logPaths: {
          server: serverLogPath,
          browser: sessionData.logs.browser
        },
        directories: {
          screenshots: sessionData.screenshots,
          files: sessionData.screenshots.replace('screenshots', 'files') // derive files path
        }
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Universal session creation error: ${errorMessage}`, 'error');
      return null;
    }
  }

  /**
   * Enable session logging for all registered daemons
   * This ensures JTAG observability - all daemon activity goes to session logs
   */
  // TODO: Re-enable when session system is restored
  /*
  private _enableSessionLoggingForAllDaemons(serverLogPath: string, sessionId: string): void {
    this.log(`📝 Enabling session logging for all daemons: ${serverLogPath}`);
    
    // Enable logging for all registered daemons
    for (const [name, daemon] of this.registeredDaemons) {
      try {
        if (daemon && typeof daemon.setSessionLogPath === 'function') {
          daemon.setSessionLogPath(serverLogPath);
          this.log(`✅ Session logging enabled for ${name} daemon`);
          
          // Log daemon startup to session log for JTAG observability
          daemon.log(`✅ ${name} daemon session logging enabled for session ${sessionId}`, 'info');
        } else {
          this.log(`⚠️ Daemon ${name} does not support session logging`, 'warn');
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log(`⚠️ Failed to enable logging for ${name}: ${errorMessage}`, 'warn');
      }
    }
    
    // Also enable logging for this daemon (WebSocketDaemon)
    try {
      this.setSessionLogPath(serverLogPath);
      this.log(`✅ WebSocket daemon session logging enabled for session ${sessionId}`, 'info');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️ Failed to enable WebSocket daemon logging: ${errorMessage}`, 'warn');
    }
  }
  */

  /**
   * Get session parameters based on connection type
   */
  // TODO: Re-enable when session system is restored  
  /*
  private _getSessionParamsForConnection(connectionType: any, connectionId: string): any {
    const baseParams = {
      type: 'development', // Default type
      owner: 'shared', // Default to shared sessions for all connections
      options: {
        autoCleanup: true,
        cleanupAfterMs: 2 * 60 * 60 * 1000, // 2 hours
        connectionId: connectionId
      }
    };

    // Customize based on connection type
    switch (connectionType.type) {
      case 'user-browser':
        return { ...baseParams, type: 'development', owner: 'shared' }; // Use shared session for browser connections
      case 'portal':
        return { ...baseParams, type: 'portal', owner: 'ai-assistant' };
      case 'git-hook':
        return { ...baseParams, type: 'git-hook', owner: 'automation' };
      case 'api-client':
        return { ...baseParams, type: 'development', owner: 'api' };
      default:
        // Default to shared for any unrecognized connection type
        this.log(`🔍 Unknown connection type: ${connectionType.type}, defaulting to shared session`, 'warn');
        return baseParams; // baseParams now has owner: 'shared'
    }
  }
  */

  /**
   * Handle browser console messages and write them to session browser.log
   * This enables browser console capture via WebSocket instead of DevTools
   */
  private async handleBrowserConsoleMessage(connectionId: string, message: any): Promise<void> {
    try {
      // Get the session ID for this connection
      // TODO: Re-enable session ID lookup when session system is fixed
      // const sessionId = this.getSessionIdForConnection(connectionId);
      const sessionId = null;
      if (!sessionId) {
        this.log(`⚠️ No session found for connection ${connectionId} - cannot log console message`, 'warn');
        return;
      }

      // Get session manager to find browser log path
      const sessionManager = this.registeredDaemons.get('session-manager');
      if (!sessionManager) {
        this.log(`⚠️ Session manager not available for console logging`, 'warn');
        return;
      }

      // Get session info to find browser log path
      const sessionInfo = await sessionManager.handleMessage({
        type: 'get_session_info',
        data: { sessionId: sessionId }
      });

      if (!sessionInfo.success) {
        this.log(`⚠️ Could not get session info for console logging: ${sessionInfo.error}`, 'warn');
        return;
      }

      const browserLogPath = sessionInfo.data?.artifacts?.logs?.client?.[0];
      if (!browserLogPath) {
        this.log(`⚠️ No browser log path found for session ${sessionId}`, 'warn');
        return;
      }

      // Format the console message for logging
      const timestamp = new Date().toISOString();
      const logEntry = `[${timestamp}] Console Message from ${connectionId}:\n   Type: ${message.data?.level || 'log'}\n   Message: ${message.data?.message || JSON.stringify(message.data)}\n   Data: ${JSON.stringify(message.data)}\n`;

      // Write to browser log file
      await this.writeToBrowserLog(browserLogPath, logEntry);
      
      this.log(`📝 Browser console message logged to ${browserLogPath}`);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to handle browser console message: ${errorMessage}`, 'error');
    }
  }

  /**
   * Write console message to browser log file
   */
  private async writeToBrowserLog(logPath: string, content: string): Promise<void> {
    try {
      const fs = await import('fs/promises');
      await fs.appendFile(logPath, content);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to write to browser log ${logPath}: ${errorMessage}`, 'error');
    }
  }

  // SESSION MAPPING REMOVED - pure router doesn't track session connections

  /**
   * Handle send_to_connection message from other daemons
   */
  private async handleSendToConnection(data: unknown): Promise<DaemonResponse> {
    try {
      const { connectionId, message } = data as { connectionId: string; message: unknown };
      
      if (!connectionId || !message) {
        return {
          success: false,
          error: 'connectionId and message are required'
        };
      }
      
      // Send message to specific connection
      const sent = this.wsManager.sendToConnection(connectionId, message);
      
      if (sent) {
        this.log(`📤 Sent message to connection ${connectionId}`);
        
        // Track session assignment if this is a session_ready message
        const msg = message as any;
        if (msg.type === 'session_ready' && msg.data?.sessionId) {
          this.connectionSessions.set(connectionId, msg.data.sessionId);
          this.log(`🔗 Mapped connection ${connectionId} to session ${msg.data.sessionId}`);
        }
        
        return {
          success: true,
          data: { sent: true }
        };
      } else {
        return {
          success: false,
          error: `Connection ${connectionId} not found`
        };
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to send to connection: ${errorMessage}`
      };
    }
  }

  /**
   * Handle connection closure - PURE ROUTER CLEANUP
   */
  private async handleConnectionClosed(connectionId: string): Promise<void> {
    try {
      // Pure router - just log the disconnection
      this.log(`🔌 Connection ${connectionId} closed`);
      
      // Clean up session mapping
      this.connectionSessions.delete(connectionId);
      
      // Emit event for other daemons
      const { DAEMON_EVENT_BUS } = await import('../../daemons/base/DaemonEventBus');
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.WEBSOCKET_CONNECTION_CLOSED, {
        timestamp: new Date(),
        source: this.name,
        connectionId
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️  Error handling connection closure for ${connectionId}: ${errorMessage}`, 'error');
    }
  }
}

// REMOVED: All session management methods - disabled during session system cleanup

// Main execution (direct execution detection)
if (process.argv[1] && process.argv[1].endsWith('WebSocketDaemon.ts')) {
  const daemon = new WebSocketDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });

  daemon.start().catch(error => {
    console.error('❌ Failed to start WebSocket Daemon:', error);
    process.exit(1);
  });
}