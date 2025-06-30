/**
 * WebSocket Daemon - Pure Content-Agnostic Router
 * ONLY handles routing and connections - knows NOTHING about content
 */

import { BaseDaemon } from '../../daemons/base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../daemons/base/DaemonProtocol';
import { RouteManager } from './core/RouteManager';
import { WebSocketManager } from './core/WebSocketManager';
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

    this.routeManager = new RouteManager();
    this.wsManager = new WebSocketManager();
    
    // Set up WebSocket message handling
    this.wsManager.onMessage = (connectionId: string, data: any) => {
      this.handleWebSocketMessage(connectionId, data);
    };
  }

  protected async onStart(): Promise<void> {
    this.log(`🌐 Starting pure router on ${this.config.host}:${this.config.port}`);
    
    // Start HTTP server for routing
    this.httpServer = createServer(async (req, res) => {
      await this.handleHttpRequest(req, res);
    });

    // Start WebSocket server
    await this.wsManager.start(this.config.port);

    // Start HTTP server on same port
    this.httpServer.listen(this.config.port, this.config.host, () => {
      this.log(`✅ Pure router ready - knows nothing about content, only routes`);
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

  /**
   * Register route handler - PURE ROUTING
   */
  public registerRouteHandler(pattern: string, daemon: any, handler: (pathname: string, req: any, res: any) => Promise<void>): void {
    this.routeManager.registerRoute(pattern, daemon, handler);
  }

  private async handleHttpRequest(req: any, res: any): Promise<void> {
    const url = new URL(req.url!, `http://${this.config.host}:${this.config.port}`);
    const pathname = url.pathname;

    try {
      // Pure routing - delegate to registered handlers
      const handled = await this.routeManager.handleRequest(pathname, req, res);
      
      if (!handled) {
        // No route found - simple 404
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not Found');
        this.log(`❌ No route for: ${pathname}`);
      } else {
        this.log(`✅ Routed: ${pathname}`);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Routing error for ${pathname}: ${errorMessage}`, 'error');
      
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Internal Server Error');
    }
  }

  private handleWebSocketMessage(connectionId: string, data: any): void {
    try {
      const message = JSON.parse(data.toString());
      
      // Route WebSocket messages to appropriate daemon
      // This is pure message routing, no content knowledge
      
      if (message.type === 'execute_command') {
        this.routeCommandToProcessor(connectionId, message);
      } else {
        this.log(`📨 Unknown WebSocket message type: ${message.type}`);
      }
    } catch (error) {
      this.log(`❌ WebSocket message parse error: ${error}`, 'error');
    }
  }

  private routeCommandToProcessor(connectionId: string, message: any): void {
    // Pure routing - find command processor daemon and forward
    const commandProcessor = this.registeredDaemons.get('command-processor');
    
    if (commandProcessor) {
      // Forward to command processor - router doesn't care about content
      commandProcessor.handleWebSocketCommand?.(connectionId, message);
    } else {
      this.log('❌ No command processor registered', 'error');
    }
  }

  private handleDaemonRegistration(_data: any): DaemonResponse {
    // Handle external daemon registration
    return {
      success: true,
      data: { registered: true }
    };
  }
}

// Main execution
if (require.main === module) {
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