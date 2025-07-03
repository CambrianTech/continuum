/**
 * WebSocket Daemon - Pure Content-Agnostic Router
 * ONLY handles routing and connections - knows NOTHING about content
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol';
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

  private routeManager: RouteManager;
  private wsManager: WebSocketManager;
  private portManager: PortManager;
  private httpServer: any = null;
  private config: Required<ServerConfig>;
  private registeredDaemons = new Map<string, any>();

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
      
      // Log all incoming WebSocket messages for debugging
      this.log(`🔍 WebSocket message received: type=${message.type}, connectionId=${connectionId}`);
      
      // Route WebSocket messages to appropriate daemon
      // This is pure message routing, no content knowledge
      
      if (message.type === 'execute_command') {
        this.routeCommandToProcessor(connectionId, message).catch(error => {
          this.log(`❌ Failed to route command: ${error}`, 'error');
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
        const sessionId = commandData.sessionId; // Get sessionId from browser
        
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
              websocket: true,
              requestId: requestId,
              sessionId: sessionId // Include sessionId in context
            }
          }
        };
        
        this.log(`🔄 Routing command "${commandName}" to command processor`);
        
        // Forward to command processor using standard daemon protocol
        const response = await commandProcessor.handleMessage(daemonMessage);
        
        // Send response back to WebSocket client
        const wsResponse = {
          type: 'command_response',
          requestId: requestId,
          command: commandName,
          success: response.success,
          data: response.data,
          error: response.error,
          timestamp: new Date().toISOString()
        };
        
        this.wsManager.sendToConnection(connectionId, wsResponse);
        
        this.log(`✅ Command "${commandName}" ${response.success ? 'completed' : 'failed'}`);
        
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        this.log(`❌ Error routing command to processor: ${errorMessage}`, 'error');
        
        // Send error response to client
        const errorResponse = {
          type: 'command_response',
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
        type: 'command_response',
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
      this.log(`🔍 DEBUG: Calling handleMessage with: ${JSON.stringify(message)}`, 'debug');
      try {
        const result = await daemon.handleMessage(message);
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
   * Handle new WebSocket connection - ensure browser session exists
   */
  private async handleNewConnection(connectionId: string, connection: any): Promise<void> {
    try {
      // 1. Identify connection type from user-agent or URL patterns
      const connectionType = this.identifyConnectionType(connection);
      
      this.log(`🔌 New ${connectionType.type} connection: ${connectionId}`);
      
      // 2. Check if this connection needs a browser session
      if (this.shouldCreateBrowserSession(connectionType)) {
        await this.ensureBrowserSession(connectionId, connectionType);
      }
      
      // 3. Handle DevTools mode for git hooks and portal connections
      if (connectionType.needsDevTools) {
        await this.setupDevToolsMode(connectionId, connectionType);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to handle new connection ${connectionId}: ${errorMessage}`, 'error');
    }
  }

  /**
   * Handle connection closure - cleanup sessions if needed
   */
  private async handleConnectionClosed(connectionId: string): Promise<void> {
    try {
      // Could add session cleanup logic here if needed
      this.log(`🔌 Connection ${connectionId} closed - session cleanup handled by SessionManager`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️  Error handling connection closure for ${connectionId}: ${errorMessage}`, 'error');
    }
  }

  /**
   * Identify connection type from metadata
   */
  private identifyConnectionType(connection: any): { type: string; needsDevTools: boolean; context: string } {
    const userAgent = connection.metadata.userAgent || '';
    const url = connection.metadata.url || '';
    
    // Git hook connections (often have specific user agents or URL patterns)
    if (userAgent.includes('git') || url.includes('hook') || url.includes('ci')) {
      return { type: 'git-hook', needsDevTools: true, context: 'automation' };
    }
    
    // Portal connections (your development interface)
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
  private shouldCreateBrowserSession(connectionType: any): boolean {
    // All user connections need browser sessions
    // Git hooks and portal connections might need them for testing
    return ['user', 'user-browser', 'portal'].includes(connectionType.type);
  }

  /**
   * Ensure browser session exists for this connection
   */
  private async ensureBrowserSession(connectionId: string, connectionType: any): Promise<void> {
    try {
      const sessionManager = this.registeredDaemons.get('session-manager');
      const browserManager = this.registeredDaemons.get('browser-manager');
      
      if (!sessionManager || !browserManager) {
        this.log(`⚠️  Session or Browser manager not available for ${connectionId}`);
        return;
      }

      // Create session for this connection
      const sessionRequest = {
        type: 'create_session',
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

      const sessionResponse = await sessionManager.handleMessage(sessionRequest);
      
      if (sessionResponse.success) {
        const sessionData = sessionResponse.data;
        
        // Request smart browser management
        const browserRequest = {
          type: 'create_browser',
          data: {
            sessionId: sessionData.sessionId,
            url: `http://localhost:${this.config.port}`,
            config: {
              purpose: connectionType.context,
              requirements: {
                devtools: connectionType.needsDevTools,
                isolation: 'dedicated',
                visibility: 'visible',
                persistence: 'session'
              },
              resources: {
                priority: connectionType.needsDevTools ? 'high' : 'normal'
              }
            }
          }
        };

        const browserResponse = await browserManager.handleMessage(browserRequest);
        
        if (browserResponse.success) {
          this.log(`✅ Browser session ready for ${connectionId}: ${browserResponse.data.action}`);
          
          // Send session info to client
          this.wsManager.sendToConnection(connectionId, {
            type: 'session_ready',
            data: {
              sessionId: sessionData.sessionId,
              browserId: browserResponse.data.browserId,
              devtools: connectionType.needsDevTools,
              action: browserResponse.data.action
            }
          });
        } else {
          this.log(`❌ Browser session failed for ${connectionId}: ${browserResponse.error}`);
        }
      } else {
        this.log(`❌ Session creation failed for ${connectionId}: ${sessionResponse.error}`);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Error ensuring browser session for ${connectionId}: ${errorMessage}`, 'error');
    }
  }

  /**
   * Setup DevTools mode with extra control port
   */
  private async setupDevToolsMode(connectionId: string, connectionType: any): Promise<void> {
    // For git hooks and portal connections, set up debug mode with extra port
    this.log(`🛠️  Setting up DevTools mode for ${connectionType.type} connection ${connectionId}`);
    
    // In real implementation:
    // 1. Allocate debug port (e.g., 9001, 9002)
    // 2. Launch browser with --remote-debugging-port
    // 3. Send debug port info to client
    
    this.wsManager.sendToConnection(connectionId, {
      type: 'devtools_ready',
      data: {
        debugPort: 9001, // Mock debug port
        connectionType: connectionType.type,
        capabilities: ['console', 'network', 'sources', 'performance']
      }
    });
  }
}

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