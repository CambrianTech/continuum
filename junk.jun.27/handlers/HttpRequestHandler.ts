/**
 * HTTP Request Handler - Modular HTTP request routing and processing
 * Extracted from WebSocketDaemon to reduce complexity
 */

import { IncomingMessage, ServerResponse } from 'http';

export interface RouteHandler {
  daemon: any;
  handler: (pathname: string, req: IncomingMessage, res: ServerResponse) => Promise<void>;
}

export interface ApiHandler {
  daemon: any;
  handler: (pathname: string, req: IncomingMessage, res: ServerResponse) => Promise<void>;
}

export class HttpRequestHandler {
  private routeHandlers = new Map<string, RouteHandler>();
  private apiHandlers = new Map<string, ApiHandler>();

  constructor(
    private serverName: string,
    private serverVersion: string,
    private generateStatusPage: () => Promise<string>
  ) {}

  registerRouteHandler(pattern: string, daemon: any, handler: Function): void {
    this.routeHandlers.set(pattern, { daemon, handler });
  }

  registerApiHandler(endpoint: string, daemon: any, handler: Function): void {
    this.apiHandlers.set(endpoint, { daemon, handler });
  }

  async handleRequest(req: IncomingMessage, res: ServerResponse, host: string, port: number): Promise<void> {
    const url = new URL(req.url!, `http://${host}:${port}`);
    
    // Check for registered route handlers first (including root '/')
    if (req.method === 'GET') {
      const routeHandler = this.findRouteHandler(url.pathname);
      if (routeHandler) {
        await routeHandler.handler(url.pathname, req, res);
        return;
      }
    }
    
    if (req.method === 'GET' && url.pathname === '/health') {
      this.handleHealthCheck(res);
    } else if (req.method === 'GET' && url.pathname === '/status') {
      await this.handleStatusPage(res);
    } else if (req.method === 'GET' && url.pathname.startsWith('/api/')) {
      await this.handleApiRequest(url.pathname, req, res);
    } else {
      this.handleNotFound(res);
    }
  }

  private findRouteHandler(pathname: string): RouteHandler | undefined {
    // Direct match first
    if (this.routeHandlers.has(pathname)) {
      return this.routeHandlers.get(pathname);
    }
    
    // Pattern matching for wildcards
    for (const [pattern, handler] of this.routeHandlers) {
      if (pattern.endsWith('*')) {
        const basePattern = pattern.slice(0, -1);
        if (pathname.startsWith(basePattern)) {
          return handler;
        }
      }
    }
    
    return undefined;
  }

  private handleHealthCheck(res: ServerResponse): void {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ 
      status: 'healthy', 
      server: this.serverName, 
      version: this.serverVersion 
    }));
  }

  private async handleStatusPage(res: ServerResponse): Promise<void> {
    const statusPage = await this.generateStatusPage();
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(statusPage);
  }

  private async handleApiRequest(pathname: string, req: IncomingMessage, res: ServerResponse): Promise<void> {
    const apiHandler = this.apiHandlers.get(pathname);
    if (apiHandler) {
      await apiHandler.handler(pathname, req, res);
    } else {
      // Built-in API endpoints would go here
      this.handleNotFound(res);
    }
  }

  private handleNotFound(res: ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('Not Found');
  }
}