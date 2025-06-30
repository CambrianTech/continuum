/**
 * Route Manager - Clean HTTP route handling
 * Pure routing logic, no content generation
 */

export interface RouteHandler {
  daemon: any;
  handler: (pathname: string, req: any, res: any) => Promise<void>;
}

export class RouteManager {
  private routes = new Map<string, RouteHandler>();

  registerRoute(pattern: string, daemon: any, handler: (pathname: string, req: any, res: any) => Promise<void>): void {
    this.routes.set(pattern, { daemon, handler });
    console.log(`🔗 Registered route: ${pattern} → ${daemon.name || 'daemon'}`);
  }

  async handleRequest(pathname: string, req: any, res: any): Promise<boolean> {
    // Exact match first
    const exactHandler = this.routes.get(pathname);
    if (exactHandler) {
      await exactHandler.handler(pathname, req, res);
      return true;
    }

    // Pattern matching for wildcards
    for (const [pattern, handler] of this.routes) {
      if (this.matchesPattern(pathname, pattern)) {
        await handler.handler(pathname, req, res);
        return true;
      }
    }

    return false; // No route found
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