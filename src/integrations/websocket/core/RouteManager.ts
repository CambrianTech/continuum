// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Route Manager - Clean HTTP route handling with WebSocket messaging
 * Pure routing logic, no content generation, uses WebSocket for daemon communication
 * 
 * ✅ CLEANED UP: Removed hardcoded session management logic (2025-07-13)
 * ✅ CLEANED UP: Made session management modular via daemons (2025-07-13)
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
    // Use console.log since RouteManager doesn't extend BaseDaemon (no this.log available)
    console.log(`🎯🎯🎯 ROUTE MANAGER: Handling request for pathname: ${pathname} 🎯🎯🎯`);
    console.log(`🎯🎯🎯 ROUTE MANAGER: Available routes:`, Array.from(this.routes.entries()));
    
    // Exact match first
    const exactHandler = this.routes.get(pathname);
    if (exactHandler) {
      await this.forwardRequestToDaemon(exactHandler, pathname, req, res);
      return true;
    }

    // Pattern matching for wildcards (excluding catch-all)
    let catchAllHandler: RouteHandler | null = null;
    
    for (const [pattern, handler] of this.routes) {
      if (pattern === '*') {
        // Save catch-all for last
        catchAllHandler = handler;
        continue;
      }
      
      if (this.matchesPattern(pathname, pattern)) {
        console.log(`🔥🔥🔥 ROUTE MANAGER: Pattern ${pattern} matches ${pathname}, forwarding to ${handler.daemonName}::${handler.handlerName} 🔥🔥🔥`);
        await this.forwardRequestToDaemon(handler, pathname, req, res);
        return true;
      }
    }
    
    // Use catch-all if no other routes matched
    if (catchAllHandler) {
      await this.forwardRequestToDaemon(catchAllHandler, pathname, req, res);
      return true;
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
      // Parse request body if it's a POST/PUT request
      let body: any = null;
      if (req.method === 'POST' || req.method === 'PUT') {
        body = await this.parseRequestBody(req);
      }

      const message = {
        type: handler.handlerName,
        data: {
          pathname,
          method: req.method,
          headers: req.headers,
          url: req.url,
          body,
          // Include raw request info for daemons to handle session extraction
          requestInfo: {
            headers: req.headers,
            url: req.url,
            method: req.method
          }
        }
      };
      
      console.log(`🚀🚀🚀 ROUTE MANAGER: Sending message to ${handler.daemonName}:`, message.type, `for ${pathname} 🚀🚀🚀`);

      const response = await this.messageCallback(handler.daemonName, message);
      
      if (response.success) {
        // Handle different response formats
        if (response.data && response.data.status && response.data.status !== 200) {
          // Special status codes (304, 404, etc.)
          res.writeHead(response.data.status, response.data.headers || {});
          res.end(response.data.content || '');
        } else {
          // Normal 200 response
          let content: string;
          let contentType: string;
          
          if (response.data && response.data.content) {
            // Response already formatted for HTTP (like static files)
            content = response.data.content;
            contentType = response.data.contentType || 'text/plain';
          } else {
            // Command result - return just the data, not the wrapper
            content = JSON.stringify(response.data, null, 2);
            contentType = 'application/json';
          }
          
          const headers = response.data?.headers || {};
          
          // If response includes session management headers, include them
          if (response.sessionHeaders) {
            Object.assign(headers, response.sessionHeaders);
          }
          
          res.writeHead(200, { 
            'Content-Type': contentType,
            ...headers 
          });
          res.end(content);
        }
      } else {
        // Always return JSON for API errors to maintain consistent format
        const errorResponse = {
          success: false,
          error: response.error || 'Unknown error',
          timestamp: new Date().toISOString()
        };
        res.writeHead(response.data?.status || 500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify(errorResponse, null, 2));
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorResponse = {
        success: false,
        error: `Request forwarding error: ${errorMessage}`,
        timestamp: new Date().toISOString()
      };
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(errorResponse, null, 2));
    }
  }

  private matchesPattern(pathname: string, pattern: string): boolean {
    // Handle file extension patterns like *.css
    if (pattern.startsWith('*.')) {
      const extension = pattern.substring(1); // Get .css from *.css
      return pathname.endsWith(extension);
    }
    
    // Handle prefix patterns like /src/*
    if (pattern.endsWith('*')) {
      const prefix = pattern.slice(0, -1);
      return pathname.startsWith(prefix);
    }
    
    // Exact match
    return pathname === pattern;
  }

  getRegisteredRoutes(): string[] {
    return Array.from(this.routes.keys());
  }


  private parseRequestBody(req: any): Promise<any> {
    return new Promise((resolve, reject) => {
      let body = '';
      req.on('data', (chunk: any) => {
        body += chunk.toString();
      });
      req.on('end', () => {
        if (!body) {
          resolve(null);
          return;
        }
        try {
          // Try to parse as JSON
          const parsed = JSON.parse(body);
          resolve(parsed);
        } catch {
          // If not JSON, return raw body
          resolve(body);
        }
      });
      req.on('error', reject);
    });
  }
}