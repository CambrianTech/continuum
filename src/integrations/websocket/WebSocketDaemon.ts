/**
 * WebSocket Daemon - Clean Router Implementation
 * 
 * 🎫 ISSUE TICKETS (Priority: HIGH → LOW):
 * 
 * #004 [HIGH] Remove temporary route handlers (HACKED FIXES)
 *   - PROBLEM: Lines 975-1067 contain hardcoded route handlers violating separation of concerns
 *   - SOLUTION: Move to RendererDaemon via proper registration system
 *   - FILES: WebSocketDaemon.ts:975-1067, RendererDaemon.ts
 *   - BLOCKED_BY: #002 (daemon registration)
 * 
 * #005 [HIGH] Separate static file serving  
 *   - PROBLEM: handleStaticFiles() shouldn't be in WebSocket daemon
 *   - SOLUTION: Create StaticFileService, register with RendererDaemon
 *   - FILES: WebSocketDaemon.ts:1037-1067, new StaticFileService.ts
 *   - BLOCKED_BY: #004
 * 
 * #006 [MED] Replace manual route registration with discovery
 *   - PROBLEM: Manual registerRouteHandler() calls, not modular
 *   - SOLUTION: Auto-discover route handlers via metadata/decorators
 *   - FILES: WebSocketDaemon.ts:323-334
 *   - BLOCKED_BY: #004, #005
 * 
 * RESPONSIBILITIES (SHOULD DO):
 * - WebSocket server management
 * - HTTP request routing to other daemons  
 * - Connection management
 * - Message routing
 * 
 * NOT RESPONSIBLE FOR (DELEGATE TO OTHER DAEMONS):
 * - HTML generation (RendererDaemon)
 * - Static file serving (RendererDaemon)  
 * - Command execution (CommandProcessorDaemon)
 * - UI data generation (RendererDaemon)
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol';
import { WebSocketServer, WebSocket } from 'ws';
import { ConnectionManager } from './core/ConnectionManager';
import { DynamicMessageRouter } from './core/DynamicMessageRouter';
import { DaemonConnector } from './core/DaemonConnector';
import { BrowserManager } from '../../core/BrowserManager';
import { ServerConfig, WebSocketMessage } from './types';
import { createServer } from 'http';

export class WebSocketDaemon extends BaseDaemon {
  public readonly name = 'websocket-server';
  public readonly version = '1.0.0';

  private server: WebSocketServer | null = null;
  private httpServer: any = null;
  private connectionManager: ConnectionManager;
  private messageRouter: DynamicMessageRouter;
  private daemonConnector: DaemonConnector;
  private browserManager: BrowserManager;
  private registeredDaemons = new Map<string, any>();
  private routeHandlers = new Map<string, { daemon: any; handler: Function }>();
  private apiHandlers = new Map<string, { daemon: any; handler: Function }>();
  private config: Required<ServerConfig>;

  constructor(config: ServerConfig = {}) {
    super();
    
    this.config = {
      port: config.port ?? 9000,
      host: config.host ?? 'localhost',
      maxClients: config.maxClients ?? 100,
      enableHeartbeat: config.enableHeartbeat ?? true,
      enableAuth: config.enableAuth ?? false,
      daemonConfig: {
        autoConnect: false, // Disable until module paths fixed
        enableFallback: false,
        retryAttempts: 3,
        retryInterval: 5000,
        ...config.daemonConfig
      }
    };

    // Initialize modular components
    this.connectionManager = new ConnectionManager({
      maxClients: this.config.maxClients,
      enableHeartbeat: this.config.enableHeartbeat,
      enableAuth: this.config.enableAuth
    });

    this.messageRouter = new DynamicMessageRouter();
    this.daemonConnector = new DaemonConnector(this.config.daemonConfig);
    this.browserManager = new BrowserManager(this.config.port);

    this.setupEventHandlers();
  }

  protected async onStart(): Promise<void> {
    this.log(`Starting WebSocket server on ${this.config.host}:${this.config.port}`);
    
    // Register self as a daemon in the router
    await this.messageRouter.registerDaemon(this.name, this);
    this.log(`✅ Registered self (${this.name}) with dynamic router`);
    
    // Start daemon connector first
    if (this.config.daemonConfig.autoConnect) {
      await this.daemonConnector.connect();
    }

    // Start HTTP server first, then attach WebSocket
    try {
      // Create HTTP server
      this.httpServer = createServer((req, res) => {
        this.handleHttpRequest(req, res);
      });

      // Create WebSocket server attached to HTTP server
      this.server = new WebSocketServer({
        server: this.httpServer
      });

      this.server.on('connection', (socket, request) => {
        this.handleConnection(socket, request);
      });

      this.server.on('error', (error) => {
        this.log(`❌ WebSocket server error: ${error.message}`, 'error');
        this.emit('error', error);
        throw error;
      });

      // Start HTTP server listening
      this.httpServer.listen(this.config.port, this.config.host, () => {
        this.log(`✅ HTTP server listening on http://${this.config.host}:${this.config.port}`);
        this.log(`✅ WebSocket server ACTUALLY listening on ws://${this.config.host}:${this.config.port}`);
      });

      // Wait for server to actually start listening
      await new Promise<void>((resolve, reject) => {
        this.httpServer.on('listening', resolve);
        this.httpServer.on('error', reject);
        
        // Timeout after 5 seconds
        setTimeout(() => reject(new Error('HTTP/WebSocket server failed to start listening')), 5000);
      });

      this.log(`✅ WebSocket server started and verified listening on ws://${this.config.host}:${this.config.port}`);
      
      // TEMP FIX: Register essential routes until daemon registration is working
      await this.registerEssentialRoutes();
      
    } catch (error) {
      this.log(`❌ Failed to start WebSocket server: ${error.message}`, 'error');
      throw error;
    }
  }

  protected async onStop(): Promise<void> {
    this.log('Stopping WebSocket server...');

    // Stop daemon connector
    await this.daemonConnector.disconnect();

    // Cleanup browser manager
    await this.browserManager.cleanup();

    // Shutdown connection manager
    this.connectionManager.shutdown();

    // Close WebSocket server
    if (this.server) {
      await new Promise<void>((resolve) => {
        this.server!.close(() => {
          this.log('✅ WebSocket server stopped');
          resolve();
        });
      });
      this.server = null;
    }

    // Close HTTP server
    if (this.httpServer) {
      await new Promise<void>((resolve) => {
        this.httpServer.close(() => {
          this.log('✅ HTTP server stopped');
          resolve();
        });
      });
      this.httpServer = null;
    }
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'get_stats':
        return {
          success: true,
          data: this.getStats()
        };
        
      case 'get_clients':
        return {
          success: true,
          data: this.connectionManager.getAllClients().map(client => ({
            id: client.id,
            connected: client.connected,
            connectTime: client.connectTime,
            lastActivity: client.lastActivity,
            metadata: client.metadata
          }))
        };
        
      case 'send_message':
        return this.handleSendMessage(message.data);
        
      case 'broadcast_message':
        return this.handleBroadcastMessage(message.data);

      case 'tabRegister':
        return this.handleTabRegister(message.data);

      case 'get_component_css':
        return this.handleGetComponentCSS(message.data);

      case 'get_capabilities':
        return {
          success: true,
          data: {
            capabilities: [
              'websocket-server',
              'client-management', 
              'api-endpoints',
              'css-service'
            ]
          }
        };
        
      case 'ping':
        return { 
          success: true, 
          data: { 
            type: 'pong', 
            timestamp: new Date().toISOString(),
            server: 'websocket-daemon'
          } 
        };
        
      case 'pong':
        return { 
          success: true, 
          data: { 
            received: true,
            timestamp: new Date().toISOString()
          } 
        };
        
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }

  private handleConnection(socket: WebSocket, request: any): void {
    try {
      const clientMetadata = {
        userAgent: request.headers['user-agent'],
        origin: request.headers.origin,
        remoteAddress: request.socket.remoteAddress,
        url: request.url
      };

      const clientId = this.connectionManager.addClient(socket, clientMetadata);
      
      // Register with browser manager
      this.browserManager.registerClient({
        clientId,
        ...clientMetadata,
        capabilities: []
      });

      socket.on('message', (data) => {
        this.handleClientMessage(clientId, data);
        // Update activity in browser manager
        this.browserManager.updateClientActivity(clientId);
      });

      socket.on('close', () => {
        this.browserManager.removeClient(clientId);
      });

      // Send connection confirmation
      this.sendToClient(clientId, {
        type: 'connection_confirmed',
        data: {
          clientId,
          server: this.name,
          version: this.version,
          daemon: this.daemonConnector.isConnected(),
          browserState: this.browserManager.getBrowserState()
        },
        timestamp: new Date().toISOString()
      });

    } catch (error) {
      this.log(`Failed to handle connection: ${error.message}`, 'error');
      socket.close(1011, 'Server error');
    }
  }

  private async handleHttpRequest(req: any, res: any): Promise<void> {
    const url = new URL(req.url, `http://${this.config.host}:${this.config.port}`);
    
    // Check for registered route handlers first (including root '/')
    if (req.method === 'GET') {
      const routeHandler = this.findRouteHandler(url.pathname);
      if (routeHandler) {
        await routeHandler.handler(url.pathname, req, res);
        return;
      }
    }
    
    if (req.method === 'GET' && url.pathname === '/health') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: 'healthy', server: this.name, version: this.version }));
    } else if (req.method === 'GET' && url.pathname === '/status') {
      // Serve status page with simple UI and system info
      const statusPage = await this.generateStatusPage();
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(statusPage);
    } else if (req.method === 'GET' && url.pathname.startsWith('/api/')) {
      // Check for registered API handlers first
      const apiHandler = this.apiHandlers.get(url.pathname);
      if (apiHandler) {
        await apiHandler.handler(url.pathname, req, res);
      } else {
        // Fallback to built-in API endpoints
        await this.handleApiRequest(url.pathname, req, res);
      }
    } else {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  }

  /**
   * Generic route registration - any daemon can register route handlers
   */
  public registerRouteHandler(pattern: string, daemon: any, handler: Function): void {
    this.routeHandlers.set(pattern, { daemon, handler });
    this.log(`🔌 Route registered: ${pattern} → ${daemon.name || 'unknown'}`);
    this.log(`📊 Total routes now: ${this.routeHandlers.size}`);
    this.log(`📋 All routes: ${[...this.routeHandlers.keys()].join(', ')}`);
  }

  /**
   * Generic API endpoint registration - any daemon can register API handlers
   */
  public registerApiHandler(endpoint: string, daemon: any, handler: Function): void {
    this.apiHandlers.set(endpoint, { daemon, handler });
    this.log(`🔌 API registered: ${endpoint} → ${daemon.name || 'unknown'}`);
  }

  /**
   * Find the best matching route handler for a path
   */
  private findRouteHandler(pathname: string): { daemon: any; handler: Function } | null {
    this.log(`🔍 Finding route for: ${pathname}`, 'debug');
    this.log(`📋 Available routes: ${[...this.routeHandlers.keys()].join(', ')}`, 'debug');
    this.log(`🔌 Available APIs: ${[...this.apiHandlers.keys()].join(', ')}`, 'debug');
    
    // Check API handlers first (exact match for /api/* paths)
    if (this.apiHandlers.has(pathname)) {
      this.log(`✅ API match found: ${pathname}`, 'debug');
      return this.apiHandlers.get(pathname)!;
    }
    
    // Exact match in route handlers
    if (this.routeHandlers.has(pathname)) {
      this.log(`✅ Route exact match found: ${pathname}`, 'debug');
      return this.routeHandlers.get(pathname)!;
    }
    
    // Pattern match (e.g., /src/* matches /src/anything)
    for (const [pattern, handler] of this.routeHandlers) {
      if (pattern.endsWith('*') && pathname.startsWith(pattern.slice(0, -1))) {
        this.log(`✅ Route pattern match: ${pathname} matches ${pattern}`, 'debug');
        return handler;
      }
      if (pattern.includes('*') && this.matchesPattern(pathname, pattern)) {
        this.log(`✅ Route complex pattern match: ${pathname} matches ${pattern}`, 'debug');
        return handler;
      }
    }
    
    this.log(`❌ No handler found for: ${pathname} (checked ${this.routeHandlers.size} routes, ${this.apiHandlers.size} APIs)`, 'debug');
    return null;
  }

  private matchesPattern(path: string, pattern: string): boolean {
    const regex = new RegExp('^' + pattern.replace(/\*/g, '.*') + '$');
    return regex.test(path);
  }



  private async handleApiRequest(pathname: string, req: any, res: any): Promise<void> {
    this.log(`🔌 API request: ${pathname}`);
    
    try {
      let responseData;
      
      switch (pathname) {
        case '/api/agents':
          responseData = await this.getAgentsData();
          break;
          
        case '/api/personas':
          responseData = await this.getPersonasData();
          break;
          
        case '/api/system':
          responseData = this.getSystemStatus();
          break;
          
        case '/api/daemons':
          responseData = {
            daemons: Array.from(this.registeredDaemons.keys()),
            router: this.messageRouter.getSystemStatus()
          };
          break;
          
        case '/api/version':
          responseData = await this.getCurrentVersion();
          break;
          
        case '/api/register':
          responseData = await this.handleClientRegistration(req);
          break;
          
        case '/api/commands':
          responseData = await this.getAvailableCommands();
          break;
          
        case '/api/message':
          responseData = await this.handleApiMessage(req);
          break;
          
        case '/api/disconnect':
          responseData = await this.handleClientDisconnect(req);
          break;
          
        case '/api/browser/state':
          responseData = this.browserManager.getBrowserState();
          break;
          
        case '/api/browser/ensure':
          responseData = await this.handleEnsureBrowser(req);
          break;
          
        case '/api/browser/devtools':
          responseData = await this.handleLaunchDevTools(req);
          break;
          
        default:
          res.writeHead(404, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'API endpoint not found' }));
          return;
      }
      
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(responseData));
      
    } catch (error) {
      this.log(`❌ API error for ${pathname}: ${error.message}`, 'error');
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Internal server error' }));
    }
  }

  // TODO: MODULARIZATION NOTES
  // These hardcoded data methods were removed because they belong in:
  // 1. src/data/agents.ts - Agent configuration data
  // 2. src/data/personas.ts - Persona configuration data
  // 3. RendererDaemon should handle all /api/agents and /api/personas requests
  // 4. Data should be dynamic, not hardcoded arrays

  /**
   * Register a daemon instance for direct communication
   */
  public registerDaemon(daemonName: string, daemon: any): void {
    this.registeredDaemons.set(daemonName, daemon);
    this.log(`🔌 Registered daemon: ${daemonName}`);
  }

  private async requestFromDaemon(daemonName: string, message: any): Promise<any> {
    const daemon = this.registeredDaemons.get(daemonName);
    if (daemon && daemon.handleMessage) {
      return await daemon.handleMessage(message);
    }
    throw new Error(`Daemon ${daemonName} not found or no handleMessage method`);
  }

  // TODO: Move to src/data/agents.ts - Agent data should not be in router
  // TODO: Move to src/data/personas.ts - Persona data should not be in router
  // TODO: RendererDaemon should handle all status page and UI generation

  // ============================================================================
  // BROWSER MANAGEMENT API HANDLERS
  // ============================================================================

  /**
   * Handle browser management requests
   */
  private async handleEnsureBrowser(req: any): Promise<any> {
    try {
      const body = await this.getRequestBody(req);
      const options = body ? JSON.parse(body) : { mode: 'default' };
      
      this.log(`🌐 Ensuring browser connection (mode: ${options.mode})`);
      
      const browserState = await this.browserManager.ensureBrowserConnection(options);
      
      return {
        success: true,
        browserState,
        action: 'browser_ensured',
        connections: browserState.connectedClients.length
      };
      
    } catch (error) {
      this.log(`❌ Browser ensure failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
      };
    }
  }

  private async handleLaunchDevTools(req: any): Promise<any> {
    try {
      const body = await this.getRequestBody(req);
      const options = body ? JSON.parse(body) : {};
      
      this.log(`🛠️ Launching DevTools browser`);
      
      const browserState = await this.browserManager.ensureBrowserConnection({
        mode: 'devtools',
        debugPort: options.debugPort || 9222,
        url: options.url || `http://localhost:${this.config.port}`
      });
      
      return {
        success: true,
        browserState,
        action: 'devtools_launched',
        debugPort: browserState.devToolsPort,
        devToolsApi: `http://localhost:${browserState.devToolsPort}/json`
      };
      
    } catch (error) {
      this.log(`❌ DevTools launch failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
      };
    }
  }

  // ============================================================================
  // THIN CLIENT API HANDLERS
  // ============================================================================

  /**
   * Handle thin client registration
   */
  private async handleClientRegistration(req: any): Promise<any> {
    try {
      const body = await this.getRequestBody(req);
      const registrationData = JSON.parse(body);
      
      this.log(`📡 Registering thin client: ${registrationData.data.clientId}`);
      
      // Store client info (could be expanded to persist in database)
      const clientInfo = {
        ...registrationData.data,
        registeredAt: new Date().toISOString(),
        lastSeen: new Date().toISOString()
      };
      
      // Register with browser manager
      this.browserManager.registerClient(clientInfo);
      
      return {
        success: true,
        clientId: registrationData.data.clientId,
        serverVersion: this.version,
        registeredAt: clientInfo.registeredAt,
        availableCommands: await this.getCommandCount()
      };
      
    } catch (error) {
      this.log(`❌ Client registration failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Get available commands from daemon system
   */
  private async getAvailableCommands(): Promise<any> {
    try {
      // Get commands from registered daemons
      const commands: any[] = [];
      
      for (const [daemonName, daemonInfo] of this.registeredDaemons) {
        const daemonCommands = daemonInfo.messageTypes.map(type => ({
          name: type,
          daemon: daemonName,
          type: 'daemon-command',
          capabilities: daemonInfo.capabilities,
          clientExecution: this.getClientExecutionInfo(type)
        }));
        
        commands.push(...daemonCommands);
      }
      
      return {
        success: true,
        commands,
        totalCommands: commands.length,
        daemons: Array.from(this.registeredDaemons.keys())
      };
      
    } catch (error) {
      this.log(`❌ Command discovery failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message,
        commands: []
      };
    }
  }

  /**
   * Handle API messages from thin clients
   */
  private async handleApiMessage(req: any): Promise<any> {
    try {
      const body = await this.getRequestBody(req);
      const message = JSON.parse(body);
      
      this.log(`📨 API message: ${message.type}`);
      
      // Route through message router like WebSocket messages
      const response = await this.messageRouter.routeMessage(
        message,
        'api-client',
        this.daemonConnector
      );
      
      return response || { success: true, processed: true };
      
    } catch (error) {
      this.log(`❌ API message failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Handle client disconnect
   */
  private async handleClientDisconnect(req: any): Promise<any> {
    try {
      const body = await this.getRequestBody(req);
      const disconnectData = JSON.parse(body);
      
      this.log(`📱 Client disconnected: ${disconnectData.data.clientId}`);
      
      return {
        success: true,
        message: 'Client disconnect recorded'
      };
      
    } catch (error) {
      this.log(`❌ Client disconnect handling failed: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
      };
    }
  }

  /**
   * Utility: Get request body
   */
  private async getRequestBody(req: any): Promise<string> {
    return new Promise((resolve, reject) => {
      let body = '';
      req.on('data', (chunk: any) => {
        body += chunk.toString();
      });
      req.on('end', () => {
        resolve(body);
      });
      req.on('error', reject);
    });
  }

  /**
   * Utility: Get client execution info for a command
   */
  private getClientExecutionInfo(commandType: string): any {
    // Define which commands need client-side execution
    const clientSideCommands = [
      'execute_js', 'reload', 'navigate', 'inject_css', 
      'screenshot', 'ping', 'update_ui'
    ];
    
    if (clientSideCommands.includes(commandType)) {
      return {
        required: true,
        reason: 'Browser context required'
      };
    }
    
    return {
      required: false,
      reason: 'Server-side execution'
    };
  }

  /**
   * Utility: Get command count
   */
  private async getCommandCount(): Promise<number> {
    let count = 0;
    for (const [, daemonInfo] of this.registeredDaemons) {
      count += daemonInfo.messageTypes.length;
    }
    return count;
  }

  private async handleClientMessage(clientId: string, data: WebSocket.Data): Promise<void> {
    try {
      const message = JSON.parse(data.toString());
      
      // Handle client console logs directly (JTAG methodology)
      if (message.type === 'client_console_log') {
        this.log(`📱 CLIENT ${clientId} [${message.level.toUpperCase()}]: ${message.message}`, message.level === 'error' ? 'error' : 'info');
        return; // Don't route these, just log them
      }
      
      this.log(`📨 Message from ${clientId}: ${message.type}`);

      const response = await this.messageRouter.routeMessage(
        message,
        clientId,
        this.daemonConnector
      );

      if (response) {
        this.sendToClient(clientId, response);
      }

    } catch (error) {
      this.log(`Error handling message from ${clientId}: ${error.message}`, 'error');
      
      // Get current version for error response
      const versionInfo = await this.getCurrentVersion();
      
      this.sendToClient(clientId, {
        type: 'error',
        data: { 
          error: 'Invalid message format',
          serverVersion: versionInfo.version,
          serverBuild: versionInfo.build,
          daemon: this.name
        },
        timestamp: new Date().toISOString()
      });
    }
  }

  private async handleSendMessage(data: any): Promise<DaemonResponse> {
    const { clientId, message } = data;
    const success = this.sendToClient(clientId, message);
    
    return {
      success,
      data: { sent: success }
    };
  }

  private async handleBroadcastMessage(data: any): Promise<DaemonResponse> {
    const { message, excludeClientId } = data;
    const sentCount = this.connectionManager.broadcast(message, excludeClientId);
    
    return {
      success: true,
      data: { sentCount }
    };
  }

  private async handleTabRegister(data: any): Promise<DaemonResponse> {
    const tabId = data?.tabId || `tab_${Date.now()}`;
    this.log(`📱 Tab registered: ${tabId}`);
    
    return {
      success: true,
      data: {
        registered: true,
        tabId: tabId,
        serverVersion: this.version,
        timestamp: new Date().toISOString()
      }
    };
  }

  private async handleGetComponentCSS(data: any): Promise<DaemonResponse> {
    // Log the full data object to debug
    this.log(`🔍 CSS request data: ${JSON.stringify(data)}`);
    
    const component = data?.component || 'unknown';
    const cssPath = data?.path;
    this.log(`🎨 Component CSS requested: ${component} (path: ${cssPath})`);
    
    try {
      // Try to read the actual CSS file first
      let componentCSS;
      if (cssPath) {
        componentCSS = await this.readComponentCSSFile(cssPath);
      } else {
        // Fallback to hardcoded CSS
        componentCSS = this.getComponentCSS(component);
      }
      
      return {
        success: true,
        data: {
          component: component,
          css: componentCSS,
          timestamp: new Date().toISOString()
        }
      };
    } catch (error) {
      this.log(`❌ Failed to load CSS for ${component}: ${error.message}`, 'error');
      
      // Return fallback CSS
      return {
        success: true,
        data: {
          component: component,
          css: this.getComponentCSS(component),
          timestamp: new Date().toISOString()
        }
      };
    }
  }

  // TODO: Move CSS handling to RendererDaemon - not router responsibility

  private sendToClient(clientId: string, message: WebSocketMessage): boolean {
    return this.connectionManager.sendToClient(clientId, message);
  }

  private setupEventHandlers(): void {
    this.connectionManager.on('client:connected', (client) => {
      this.log(`Client connected: ${client.id}`);
      this.emit('client:connected', client);
    });

    this.connectionManager.on('client:disconnected', (client) => {
      this.log(`Client disconnected: ${client.id}`);
      this.emit('client:disconnected', client);
    });

    this.connectionManager.on('heartbeat:cleanup', (data) => {
      this.log(`Heartbeat cleanup: ${data.removedCount} stale clients removed`);
    });

    this.daemonConnector.on('connected', () => {
      this.log('✅ Connected to TypeScript command daemon');
    });

    this.daemonConnector.on('disconnected', () => {
      this.log('❌ Disconnected from TypeScript command daemon');
    });

    this.daemonConnector.on('error', (error) => {
      this.log(`Daemon connector error: ${error.message}`, 'error');
    });
  }

  /**
   * Register an external daemon with the dynamic router
   */
  async registerExternalDaemon(name: string, daemon: any): Promise<void> {
    await this.messageRouter.registerDaemon(name, daemon);
    this.registeredDaemons.set(name, daemon);
    
    // If daemon has route registration method, call it for modular integration
    this.log(`🔍 Checking if ${name} has registerWithWebSocketDaemon method...`);
    this.log(`📊 Method exists: ${!!daemon.registerWithWebSocketDaemon}`);
    this.log(`📊 Method type: ${typeof daemon.registerWithWebSocketDaemon}`);
    
    if (daemon.registerWithWebSocketDaemon && typeof daemon.registerWithWebSocketDaemon === 'function') {
      this.log(`🎯 Calling registerWithWebSocketDaemon for ${name}...`);
      daemon.registerWithWebSocketDaemon(this);
      this.log(`🔌 Daemon ${name} registered its routes`);
    } else {
      this.log(`⚠️ Daemon ${name} does not have registerWithWebSocketDaemon method`);
    }
    
    this.log(`✅ Registered external daemon: ${name}`);
  }

  /**
   * Get comprehensive system status including all registered daemons
   */
  getSystemStatus(): any {
    const browserState = this.browserManager.getBrowserState();
    
    return {
      server: {
        name: this.name,
        version: this.version,
        status: this.getStatus(),
        uptime: this.getUptime(),
        port: this.config.port,
        host: this.config.host
      },
      connections: this.connectionManager.getStats(),
      browserConnections: {
        hasActiveConnections: browserState.hasActiveConnections,
        connectedClients: browserState.connectedClients.length,
        debugMode: browserState.debugMode,
        devToolsPort: browserState.devToolsPort,
        clients: browserState.connectedClients.map(client => ({
          type: client.type,
          lastSeen: client.lastSeen,
          capabilities: client.capabilities.length
        }))
      },
      daemonConnector: {
        connected: this.daemonConnector.isConnected(),
        commandsAvailable: this.daemonConnector.getAvailableCommands().length
      },
      dynamicRouter: this.messageRouter.getSystemStatus(),
      registeredDaemons: Array.from(this.registeredDaemons.keys())
    };
  }

  private getStats() {
    return this.getSystemStatus();
  }

  private async getCurrentVersion(): Promise<{ version: string; build: string; timestamp: string }> {
    try {
      const { readFileSync } = await import('fs');
      const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
      return {
        version: packageData.version,
        build: 'TypeScript Daemon System',
        timestamp: new Date().toISOString()
      };
    } catch (error) {
      this.log(`Failed to read version: ${error.message}`, 'error');
      return {
        version: '0.2.UNKNOWN',
        build: 'TypeScript Daemon System',
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * TEMP FIX: Register essential routes until daemon registration is working
   */
  private async registerEssentialRoutes(): Promise<void> {
    this.log('🔧 Registering essential routes (temporary fix)...');
    
    // Register main UI route
    this.registerRouteHandler('/', this, this.handleMainUI.bind(this));
    
    // Register static file routes (DEBUG: these need proper file serving)
    this.registerRouteHandler('/src/*', this, this.handleStaticFiles.bind(this));
    this.registerRouteHandler('/dist/*', this, this.handleStaticFiles.bind(this));
    
    // Register API routes
    this.registerApiHandler('/api/projects', this, this.handleProjectsAPI.bind(this));
    this.registerApiHandler('/api/agents', this, this.handleAgentsAPI.bind(this));
    this.registerApiHandler('/api/personas', this, this.handlePersonasAPI.bind(this));
    this.registerApiHandler('/api/version', this, this.handleVersionAPI.bind(this));
    
    this.log('✅ Essential routes registered successfully');
    this.log(`📊 DEBUG: Total routes: ${this.routeHandlers.size}, APIs: ${this.apiHandlers.size}`, 'debug');
  }

  private async handleMainUI(pathname: string, req: any, res: any): Promise<void> {
    const html = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum v0.2.2205 - TypeScript Client</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🟢</text></svg>">
    <style>* { margin: 0; padding: 0; box-sizing: border-box; }</style>
</head>
<body>
    <div id="app">Loading Continuum...</div>
    <script type="module" src="/src/ui/continuum.js?v=0.2.2205&bust=${Date.now()}"></script>
</body>
</html>`;
    
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(html);
  }

  private async handleProjectsAPI(endpoint: string, req: any, res: any): Promise<void> {
    const projects = [
      { name: 'Continuum OS', progress: 75, team: ['Claude Sonnet', 'Protocol Sheriff'], status: 'active' },
      { name: 'Widget System', progress: 45, team: ['Code Specialist'], status: 'active' }
    ];
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ success: true, data: projects }));
  }

  private async handleAgentsAPI(endpoint: string, req: any, res: any): Promise<void> {
    const agents = [
      { name: 'Joel (You)', type: 'human', status: 'online', capabilities: ['leadership'] },
      { name: 'Claude Sonnet', type: 'ai', status: 'online', capabilities: ['coding', 'analysis'] },
      { name: 'Protocol Sheriff', type: 'ai', status: 'online', capabilities: ['protocol enforcement', 'security'] }
    ];
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ success: true, data: agents }));
  }

  private async handlePersonasAPI(endpoint: string, req: any, res: any): Promise<void> {
    const personas = [
      { name: 'Protocol Sheriff', specialization: 'protocol_enforcement', experience: 98.7, status: 'active' },
      { name: 'Code Specialist', specialization: 'code_analysis', experience: 92.2, status: 'active' }
    ];
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ success: true, data: personas }));
  }

  private async handleVersionAPI(endpoint: string, req: any, res: any): Promise<void> {
    const version = { version: '0.2.2205', build: 'TypeScript Daemon System', timestamp: new Date().toISOString() };
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(version));
  }

  private async handleStaticFiles(pathname: string, req: any, res: any): Promise<void> {
    this.log(`📁 DEBUG: Static file request: ${pathname}`, 'debug');
    
    // TEMP: Serve basic static files until proper file serving is implemented
    try {
      const { promises: fs } = await import('fs');
      const path = await import('path');
      
      // Remove leading slash and resolve file path
      const filePath = path.join(process.cwd(), pathname);
      this.log(`📁 DEBUG: Resolved file path: ${filePath}`, 'debug');
      
      const fileContent = await fs.readFile(filePath, 'utf8');
      
      // Determine content type
      let contentType = 'text/plain';
      if (pathname.endsWith('.js')) contentType = 'application/javascript';
      else if (pathname.endsWith('.css')) contentType = 'text/css';
      else if (pathname.endsWith('.html')) contentType = 'text/html';
      
      res.writeHead(200, { 'Content-Type': contentType });
      res.end(fileContent);
      
      this.log(`✅ DEBUG: Served static file: ${pathname}`, 'debug');
      
    } catch (error) {
      this.log(`❌ DEBUG: Static file error: ${pathname} - ${error.message}`, 'debug');
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Static file not found');
    }
  }
}

// Main execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new WebSocketDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  daemon.start().catch(error => {
    console.error('❌ WebSocket daemon failed:', error);
    process.exit(1);
  });
}