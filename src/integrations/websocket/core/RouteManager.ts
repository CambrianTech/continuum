/**
 * Route Manager - Clean HTTP route handling with WebSocket messaging
 * Pure routing logic, no content generation, uses WebSocket for daemon communication
 */

export interface RouteHandler {
  daemonName: string;
  handlerName: string;
}

export class RouteManager {
  private routes = new Map<string, RouteHandler>();
  private messageCallback: ((daemonName: string, message: any) => Promise<any>) | null = null;

  constructor(messageCallback?: (daemonName: string, message: any) => Promise<any>) {
    this.messageCallback = messageCallback || null;
  }

  registerRoute(pattern: string, daemonName: string, handlerName: string): void {
    this.routes.set(pattern, { daemonName, handlerName });
    console.log(`🔗 Registered route: ${pattern} → ${daemonName}::${handlerName}`);
  }

  async handleRequest(pathname: string, req: any, res: any): Promise<boolean> {
    // Exact match first
    const exactHandler = this.routes.get(pathname);
    if (exactHandler) {
      await this.forwardRequestToDaemon(exactHandler, pathname, req, res);
      return true;
    }

    // Pattern matching for wildcards
    for (const [pattern, handler] of this.routes) {
      if (this.matchesPattern(pathname, pattern)) {
        await this.forwardRequestToDaemon(handler, pathname, req, res);
        return true;
      }
    }

    return false; // No route found
  }

  private async forwardRequestToDaemon(handler: RouteHandler, pathname: string, req: any, res: any): Promise<void> {
    if (!this.messageCallback) {
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('No message callback configured');
      return;
    }

    try {
      const message = {
        type: 'http_request',
        data: {
          pathname,
          handler: handler.handlerName,
          method: req.method,
          headers: req.headers,
          url: req.url
        }
      };

      const response = await this.messageCallback(handler.daemonName, message);
      
      if (response.success) {
        const { contentType, content, headers } = response.data;
        res.writeHead(200, { 
          'Content-Type': contentType,
          ...headers 
        });
        res.end(content);
      } else {
        res.writeHead(500, { 'Content-Type': 'text/plain' });
        res.end(`Daemon error: ${response.error}`);
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end(`Request forwarding error: ${errorMessage}`);
    }
  }

  private matchesPattern(pathname: string, pattern: string): boolean {
    if (pattern.endsWith('*')) {
      const prefix = pattern.slice(0, -1);
      return pathname.startsWith(prefix);
    }
    return pathname === pattern;
  }

  getRegisteredRoutes(): string[] {
    return Array.from(this.routes.keys());
  }
}