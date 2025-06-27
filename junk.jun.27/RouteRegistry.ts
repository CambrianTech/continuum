/**
 * RouteRegistry - Clean, documented route definitions for WebSocketDaemon
 * Prevents misuse by enforcing route patterns and API consistency
 */

export interface RouteDefinition {
  readonly pattern: string;
  readonly handler: string; // Which daemon handles this route
  readonly description: string;
  readonly security: 'public' | 'authenticated' | 'admin';
  readonly type: 'static' | 'api' | 'ui';
}

export class RouteRegistry {
  private routes: Map<string, RouteDefinition> = new Map();

  constructor() {
    this.initializeStandardRoutes();
  }

  /**
   * Initialize standard route definitions
   */
  private initializeStandardRoutes(): void {
    // UI Routes (HTML pages)
    this.register({
      pattern: '/',
      handler: 'renderer',
      description: 'Main application UI',
      security: 'public',
      type: 'ui'
    });

    // Static file routes (JS, CSS, assets)
    this.register({
      pattern: '/static/*',
      handler: 'renderer', 
      description: 'Static files (JS, CSS, images)',
      security: 'public',
      type: 'static'
    });

    // API Routes (all APIs under /api)
    this.register({
      pattern: '/api/health',
      handler: 'websocket-server',
      description: 'System health check',
      security: 'public',
      type: 'api'
    });

    this.register({
      pattern: '/api/version',
      handler: 'websocket-server',
      description: 'System version information',
      security: 'public',
      type: 'api'
    });

    this.register({
      pattern: '/api/commands/*',
      handler: 'command-processor',
      description: 'Command execution API',
      security: 'authenticated',
      type: 'api'
    });

    this.register({
      pattern: '/api/ui/*',
      handler: 'renderer',
      description: 'UI-related APIs (components, themes)',
      security: 'public',
      type: 'api'
    });

    this.register({
      pattern: '/api/browser/*',
      handler: 'websocket-server',
      description: 'Browser connection management',
      security: 'public',
      type: 'api'
    });
  }

  /**
   * Register a new route
   */
  register(route: RouteDefinition): void {
    if (this.routes.has(route.pattern)) {
      throw new Error(`Route ${route.pattern} already registered`);
    }

    // Validate route pattern
    this.validateRoutePattern(route);

    this.routes.set(route.pattern, route);
    console.log(`📋 Registered route: ${route.pattern} → ${route.handler} (${route.type})`);
  }

  /**
   * Find matching route for a given path
   */
  findRoute(path: string): RouteDefinition | null {
    // Exact match first
    if (this.routes.has(path)) {
      return this.routes.get(path)!;
    }

    // Pattern matching for wildcard routes
    for (const [pattern, route] of this.routes) {
      if (this.matchesPattern(path, pattern)) {
        return route;
      }
    }

    return null;
  }

  /**
   * Get all registered routes
   */
  getAllRoutes(): RouteDefinition[] {
    return Array.from(this.routes.values());
  }

  /**
   * Get routes by type
   */
  getRoutesByType(type: RouteDefinition['type']): RouteDefinition[] {
    return Array.from(this.routes.values()).filter(route => route.type === type);
  }

  /**
   * Validate route pattern follows conventions
   */
  private validateRoutePattern(route: RouteDefinition): void {
    const { pattern, type } = route;

    // API routes must start with /api
    if (type === 'api' && !pattern.startsWith('/api/')) {
      throw new Error(`API routes must start with /api/: ${pattern}`);
    }

    // Static routes should use /static prefix for clarity
    if (type === 'static' && !pattern.startsWith('/static/') && pattern !== '/') {
      console.warn(`⚠️ Static route ${pattern} should use /static/ prefix for clarity`);
    }

    // UI routes should be minimal (/, /admin, etc.)
    if (type === 'ui' && pattern.includes('*')) {
      console.warn(`⚠️ UI route ${pattern} uses wildcards - consider making it more specific`);
    }
  }

  /**
   * Check if path matches pattern (supports * wildcards)
   */
  private matchesPattern(path: string, pattern: string): boolean {
    if (!pattern.includes('*')) {
      return path === pattern;
    }

    // Convert pattern to regex
    const regexPattern = pattern
      .replace(/\*/g, '.*')
      .replace(/\//g, '\\/');
    
    const regex = new RegExp(`^${regexPattern}$`);
    return regex.test(path);
  }

  /**
   * Generate route documentation
   */
  generateDocumentation(): string {
    const routes = this.getAllRoutes();
    let doc = '# Continuum Route Documentation\n\n';

    // Group by type
    const types = ['ui', 'api', 'static'] as const;
    
    for (const type of types) {
      const typeRoutes = routes.filter(r => r.type === type);
      if (typeRoutes.length === 0) continue;

      doc += `## ${type.toUpperCase()} Routes\n\n`;
      
      for (const route of typeRoutes) {
        doc += `### \`${route.pattern}\`\n`;
        doc += `- **Handler**: ${route.handler}\n`;
        doc += `- **Security**: ${route.security}\n`;
        doc += `- **Description**: ${route.description}\n\n`;
      }
    }

    return doc;
  }
}

export default RouteRegistry;