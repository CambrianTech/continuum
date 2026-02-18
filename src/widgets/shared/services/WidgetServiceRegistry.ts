/**
 * Widget Service Registry - Clean Dependency Injection
 * 
 * Replaces the god object pattern in BaseWidget with proper service interfaces.
 * Services are injected into widgets, providing clean separation of concerns.
 * 
 * Each widget gets a service registry with adapters for:
 * - Data operations (database, caching, persistence)
 * - Event coordination (broadcasting, routing, WebSocket)  
 * - Resource loading (templates, styles, files)
 * - AI communications (Academy daemon, persona management)
 */

// Base service interface - all services implement this
export interface IWidgetService {
  readonly serviceName: string;
  readonly serviceVersion: string;
  initialize(context: WidgetServiceContext): Promise<void>;
  cleanup(): Promise<void>;
}

// Widget context for service initialization
export interface WidgetServiceContext {
  widgetId: string;
  widgetName: string;
  sessionId: string;
  environment: 'browser' | 'server' | 'shared';
  permissions: string[];
  capabilities: string[];
}

// Service registry interface for dependency injection
export interface IWidgetServiceRegistry {
  // Service registration
  register<T extends IWidgetService>(serviceName: string, service: T): void;
  
  // Service retrieval with type safety
  get<T extends IWidgetService>(serviceName: string): T | undefined;
  getRequired<T extends IWidgetService>(serviceName: string): T;
  
  // Service lifecycle
  initializeAll(context: WidgetServiceContext): Promise<void>;
  cleanupAll(): Promise<void>;
  
  // Service queries
  hasService(serviceName: string): boolean;
  listServices(): string[];
}

// Concrete service registry implementation
export class WidgetServiceRegistry implements IWidgetServiceRegistry {
  private services = new Map<string, IWidgetService>();
  private initialized = false;

  register<T extends IWidgetService>(serviceName: string, service: T): void {
    if (this.services.has(serviceName)) {
      console.warn(`⚠️ WidgetServiceRegistry: Service '${serviceName}' already registered - overwriting`);
    }
    
    this.services.set(serviceName, service);
    console.debug(`✅ WidgetServiceRegistry: Registered service '${serviceName}'`);
  }

  get<T extends IWidgetService>(serviceName: string): T | undefined {
    return this.services.get(serviceName) as T | undefined;
  }

  getRequired<T extends IWidgetService>(serviceName: string): T {
    const service = this.get<T>(serviceName);
    if (!service) {
      throw new Error(`❌ WidgetServiceRegistry: Required service '${serviceName}' not found`);
    }
    return service;
  }

  async initializeAll(context: WidgetServiceContext): Promise<void> {
    if (this.initialized) {
      console.warn(`⚠️ WidgetServiceRegistry: Already initialized - skipping`);
      return;
    }

    console.debug(`🔧 WidgetServiceRegistry: Initializing ${this.services.size} services...`);
    
    const initPromises = Array.from(this.services.entries()).map(async ([name, service]) => {
      try {
        await service.initialize(context);
        console.debug(`✅ WidgetServiceRegistry: Initialized service '${name}'`);
      } catch (error) {
        console.error(`❌ WidgetServiceRegistry: Failed to initialize service '${name}':`, error);
        throw error;
      }
    });

    await Promise.all(initPromises);
    this.initialized = true;
    console.debug(`✅ WidgetServiceRegistry: All services initialized successfully`);
  }

  async cleanupAll(): Promise<void> {
    if (!this.initialized) {
      return;
    }

    console.debug(`🧹 WidgetServiceRegistry: Cleaning up ${this.services.size} services...`);
    
    const cleanupPromises = Array.from(this.services.entries()).map(async ([name, service]) => {
      try {
        await service.cleanup();
        console.debug(`✅ WidgetServiceRegistry: Cleaned up service '${name}'`);
      } catch (error) {
        console.error(`❌ WidgetServiceRegistry: Failed to cleanup service '${name}':`, error);
        // Continue cleanup even if one service fails
      }
    });

    await Promise.all(cleanupPromises);
    this.initialized = false;
    console.debug(`✅ WidgetServiceRegistry: All services cleaned up`);
  }

  hasService(serviceName: string): boolean {
    return this.services.has(serviceName);
  }

  listServices(): string[] {
    return Array.from(this.services.keys());
  }
}

// Service factory for creating pre-configured registries
export class WidgetServiceFactory {
  
  /**
   * Create a standard service registry with common widget services
   */
  static createStandardRegistry(): IWidgetServiceRegistry {
    const registry = new WidgetServiceRegistry();
    
    // Services will be registered as they're implemented
    // registry.register('data', new WidgetDataService());
    // registry.register('events', new WidgetEventService());
    // registry.register('resources', new WidgetResourceService());  
    // registry.register('ai', new WidgetAIService());
    
    return registry;
  }
  
  /**
   * Create a minimal registry for lightweight widgets
   */
  static createMinimalRegistry(): IWidgetServiceRegistry {
    const registry = new WidgetServiceRegistry();
    
    // Only essential services
    // registry.register('resources', new WidgetResourceService());
    
    return registry;
  }
  
  /**
   * Create a registry for AI-focused widgets
   */
  static createAIRegistry(): IWidgetServiceRegistry {
    const registry = new WidgetServiceRegistry();
    
    // AI-focused services
    // registry.register('ai', new WidgetAIService());
    // registry.register('events', new WidgetEventService());
    // registry.register('data', new WidgetDataService());
    
    return registry;
  }
}

// Service interface exports for type checking
export type { IWidgetDataService } from './data/WidgetDataService';
export type { IWidgetEventService } from './events/WidgetEventService';
export type { IWidgetResourceService } from './resources/WidgetResourceService';
export type { IWidgetAIService } from './ai/WidgetAIService';

// Export service names as constants to prevent typos
export const WIDGET_SERVICES = {
  DATA: 'data',
  EVENTS: 'events', 
  RESOURCES: 'resources',
  AI: 'ai'
} as const;

export type WidgetServiceName = typeof WIDGET_SERVICES[keyof typeof WIDGET_SERVICES];