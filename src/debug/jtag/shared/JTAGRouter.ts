/**
 * JTAG Universal Transport Router
 * 
 * JTAG owns the WebSocket and routes messages to different transport backends:
 * - HTTP endpoints for external integration
 * - File logging for persistent storage
 * - MCP for AI agent communication
 * - Event broadcasting for real-time updates
 */

// Import the universal message format
import { JTAGUniversalMessage, JTAGMessageType, JTAGContext } from './JTAGTypes';
import { StaticFileTransport } from './StaticFileTransport';

// Export for legacy compatibility
export type { JTAGMessage, JTAGUniversalMessage } from './JTAGTypes';

// Strongly typed route patterns
export type RoutePattern = 
  | { type: 'exact'; messageType: JTAGMessageType; source?: JTAGContext; target?: string }
  | { type: 'wildcard'; pattern: string }
  | { type: 'conditional'; condition: (message: JTAGUniversalMessage) => boolean }
  | { type: 'priority'; patterns: RoutePattern[]; priority: number };

export interface RouteDefinition {
  pattern: RoutePattern;
  transport: string;
  enabled: boolean;
  metadata?: Record<string, any>;
}

export interface TypedRouteTable {
  routes: Map<string, RouteDefinition[]>;
  defaultRoute?: string;
  fallbackRoute?: string;
}

export interface JTAGTransportBackend {
  name: string;
  canHandle(message: JTAGUniversalMessage): boolean;
  process(message: JTAGUniversalMessage): Promise<any>;
  isHealthy(): boolean;
}

export class JTAGRouter {
  private transports: Map<string, JTAGTransportBackend> = new Map();
  private subscribers: Map<string, Set<(message: JTAGUniversalMessage) => void>> = new Map();
  private routeTable: TypedRouteTable = { routes: new Map() };

  constructor() {
    this.setupDefaultTransports();
    this.setupDefaultRoutes();
  }

  private setupDefaultTransports(): void {
    // Only register fallback transports - JTAG will register its own
    // HTTP Bridge Transport
    this.registerTransport(new HTTPBridgeTransport());
    
    // Event Broadcast Transport
    this.registerTransport(new EventBroadcastTransport());
    
    // Static File Transport
    this.registerTransport(new StaticFileTransport());
  }

  private setupDefaultRoutes(): void {
    // Define strongly typed routes - JTAG transports will be registered dynamically
    this.addRoute('log-to-jtag', {
      pattern: { type: 'exact', messageType: 'log' },
      transport: 'jtag-server',  // Will be registered by JTAG system
      enabled: true,
      metadata: { priority: 1, description: 'Route all log messages to JTAG file storage' }
    });

    this.addRoute('external-to-http', {
      pattern: { type: 'exact', messageType: 'log', source: 'external' },
      transport: 'http-bridge',
      enabled: true,
      metadata: { priority: 2, description: 'Route external log messages to HTTP bridge' }
    });

    this.addRoute('broadcast-all', {
      pattern: { type: 'wildcard', pattern: '*' },
      transport: 'event-broadcast',
      enabled: true,
      metadata: { priority: 0, description: 'Broadcast all messages as events' }
    });

    this.addRoute('screenshot-conditional', {
      pattern: { 
        type: 'conditional', 
        condition: (msg) => msg.type === 'screenshot' && (msg.payload as any)?.urgent === true 
      },
      transport: 'http-bridge',
      enabled: true,
      metadata: { priority: 3, description: 'Route urgent screenshots to HTTP bridge' }
    });

    this.addRoute('static-files', {
      pattern: { type: 'exact', messageType: 'static-file' },
      transport: 'static-files',
      enabled: true,
      metadata: { priority: 4, description: 'Route static file requests through router' }
    });

    // Set fallback route to JTAG server transport
    this.routeTable.fallbackRoute = 'jtag-server';
  }

  registerTransport(transport: JTAGTransportBackend): void {
    this.transports.set(transport.name, transport);
    console.log(`🔌 JTAG Router: Registered transport '${transport.name}'`);
  }

  // Route table management
  addRoute(routeId: string, route: RouteDefinition): void {
    if (!this.routeTable.routes.has(routeId)) {
      this.routeTable.routes.set(routeId, []);
    }
    this.routeTable.routes.get(routeId)!.push(route);
    console.log(`🛣️ JTAG Router: Added route '${routeId}' → '${route.transport}'`);
  }

  removeRoute(routeId: string): void {
    this.routeTable.routes.delete(routeId);
    console.log(`🗑️ JTAG Router: Removed route '${routeId}'`);
  }

  enableRoute(routeId: string): void {
    const routes = this.routeTable.routes.get(routeId);
    if (routes) {
      routes.forEach(route => route.enabled = true);
      console.log(`✅ JTAG Router: Enabled route '${routeId}'`);
    }
  }

  disableRoute(routeId: string): void {
    const routes = this.routeTable.routes.get(routeId);
    if (routes) {
      routes.forEach(route => route.enabled = false);
      console.log(`❌ JTAG Router: Disabled route '${routeId}'`);
    }
  }

  getRouteTable(): TypedRouteTable {
    return { ...this.routeTable };
  }

  async routeMessage(message: JTAGUniversalMessage): Promise<any[]> {
    // Add routing info
    message.route = message.route || [];
    message.route.push('jtag-router');

    const results: any[] = [];
    const matchedTransports = new Set<string>();

    // Use typed route table for smart routing
    const matchingRoutes = this.findMatchingRoutes(message);
    
    // Process routes in priority order
    const sortedRoutes = matchingRoutes.sort((a, b) => 
      (b.metadata?.priority ?? 0) - (a.metadata?.priority ?? 0)
    );

    for (const route of sortedRoutes) {
      if (!route.enabled) continue;
      
      const transport = this.transports.get(route.transport);
      if (transport && transport.isHealthy() && !matchedTransports.has(route.transport)) {
        matchedTransports.add(route.transport);
        
        try {
          const result = await transport.process(message);
          results.push({ 
            transport: route.transport, 
            routeId: route.metadata?.routeId,
            success: true, 
            result 
          });
        } catch (error) {
          console.error(`❌ JTAG Router: Transport '${route.transport}' failed:`, error);
          results.push({ 
            transport: route.transport, 
            routeId: route.metadata?.routeId,
            success: false, 
            error: error instanceof Error ? error.message : String(error) 
          });
        }
      }
    }

    // Fallback to legacy canHandle method if no routes matched
    if (results.length === 0) {
      for (const [name, transport] of Array.from(this.transports.entries())) {
        if (transport.canHandle(message) && transport.isHealthy() && !matchedTransports.has(name)) {
          try {
            const result = await transport.process(message);
            results.push({ transport: name, success: true, result, fallback: true });
          } catch (error) {
            console.error(`❌ JTAG Router: Fallback transport '${name}' failed:`, error);
            results.push({ transport: name, success: false, error: error instanceof Error ? error.message : String(error), fallback: true });
          }
        }
      }
    }

    // Final fallback route
    if (results.length === 0 && this.routeTable.fallbackRoute) {
      const fallbackTransport = this.transports.get(this.routeTable.fallbackRoute);
      if (fallbackTransport && fallbackTransport.isHealthy()) {
        try {
          const result = await fallbackTransport.process(message);
          results.push({ transport: this.routeTable.fallbackRoute, success: true, result, finalFallback: true });
        } catch (error) {
          console.error(`❌ JTAG Router: Final fallback transport failed:`, error);
          results.push({ transport: this.routeTable.fallbackRoute, success: false, error: error instanceof Error ? error.message : String(error), finalFallback: true });
        }
      }
    }

    // Broadcast to subscribers
    this.broadcast(message);

    return results;
  }

  private findMatchingRoutes(message: JTAGUniversalMessage): RouteDefinition[] {
    const matches: RouteDefinition[] = [];

    for (const [routeId, routes] of Array.from(this.routeTable.routes.entries())) {
      for (const route of routes) {
        if (this.matchesPattern(message, route.pattern)) {
          matches.push({ ...route, metadata: { ...route.metadata, routeId } });
        }
      }
    }

    return matches;
  }

  private matchesPattern(message: JTAGUniversalMessage, pattern: RoutePattern): boolean {
    switch (pattern.type) {
      case 'exact':
        return message.type === pattern.messageType &&
               (!pattern.source || message.source === pattern.source) &&
               (!pattern.target || message.target === pattern.target);
               
      case 'wildcard':
        if (pattern.pattern === '*') return true;
        // Simple wildcard matching - could be enhanced with regex
        return this.wildcardMatch(pattern.pattern, `${message.type}:${message.source}`);
        
      case 'conditional':
        try {
          return pattern.condition(message);
        } catch (error) {
          console.error('❌ JTAG Router: Route condition failed:', error);
          return false;
        }
        
      case 'priority':
        return pattern.patterns.some(p => this.matchesPattern(message, p));
        
      default:
        return false;
    }
  }

  private wildcardMatch(pattern: string, text: string): boolean {
    const regexPattern = pattern
      .replace(/\*/g, '.*')
      .replace(/\?/g, '.');
    return new RegExp(`^${regexPattern}$`).test(text);
  }

  subscribe(messageType: string, callback: (message: JTAGUniversalMessage) => void): void {
    if (!this.subscribers.has(messageType)) {
      this.subscribers.set(messageType, new Set());
    }
    this.subscribers.get(messageType)!.add(callback);
  }

  private broadcast(message: JTAGUniversalMessage): void {
    const typeSubscribers = this.subscribers.get(message.type);
    const allSubscribers = this.subscribers.get('*');

    [typeSubscribers, allSubscribers].forEach(subscriberSet => {
      subscriberSet?.forEach(callback => {
        try {
          callback(message);
        } catch (error) {
          console.error('❌ JTAG Router: Subscriber callback failed:', error);
        }
      });
    });
  }
}


// HTTP Bridge Transport  
class HTTPBridgeTransport implements JTAGTransportBackend {
  name = 'http-bridge';

  canHandle(message: JTAGUniversalMessage): boolean {
    return message.target === 'external' || message.source === 'external';
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    // HTTP bridging logic - forward to external HTTP endpoints
    console.log(`🌐 HTTP Bridge: Processing ${message.type} message`);
    return { bridged: true, endpoint: 'external-api' };
  }

  isHealthy(): boolean {
    return true; // HTTP always available
  }
}

// Event Broadcast Transport
class EventBroadcastTransport implements JTAGTransportBackend {
  name = 'event-broadcast';
  private eventListeners: Set<(event: any) => void> = new Set();

  canHandle(message: JTAGUniversalMessage): boolean {
    return true; // Broadcasts all messages as events
  }

  async process(message: JTAGUniversalMessage): Promise<any> {
    const event = {
      type: `jtag:${message.type}`,
      detail: message,
      timestamp: message.timestamp
    };

    // Broadcast to all event listeners
    this.eventListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        console.error('❌ Event broadcast failed:', error);
      }
    });

    return { broadcasted: true, listenerCount: this.eventListeners.size };
  }

  isHealthy(): boolean {
    return true;
  }

  addListener(listener: (event: any) => void): void {
    this.eventListeners.add(listener);
  }

  removeListener(listener: (event: any) => void): void {
    this.eventListeners.delete(listener);
  }
}

// Singleton router instance
export const jtagRouter = new JTAGRouter();