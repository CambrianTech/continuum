/**
 * Widget Event Service - Event Coordination Adapter
 * 
 * Extracts all event operations from BaseWidget god object:
 * - Event broadcasting (broadcastEvent)
 * - Router operations (WebSocket, cross-widget communication)
 * - Event coordination between widgets
 * - Custom event handling
 * 
 * Uses adapter pattern for existing JTAG event system.
 */

import type { IWidgetService, WidgetServiceContext } from '../WidgetServiceRegistry';

// Event service interface - what widgets consume
export interface IWidgetEventService extends IWidgetService {
  // Event broadcasting
  broadcastEvent(eventType: string, data: any, options?: BroadcastOptions): Promise<void>;
  
  // Event listening
  addEventListener(eventType: string, handler: EventHandler): void;
  removeEventListener(eventType: string, handler: EventHandler): void;
  
  // Router operations (WebSocket, cross-widget)
  sendToRouter(operation: string, data: any): Promise<any>;
  subscribeToRoute(route: string, handler: RouteHandler): void;
  unsubscribeFromRoute(route: string, handler: RouteHandler): void;
  
  // Widget-to-widget communication
  sendToWidget(widgetId: string, message: any): Promise<void>;
  sendToAllWidgets(message: any): Promise<void>;
  
  // Custom event coordination
  emitCustomEvent(eventName: string, detail: any): void;
  subscribeToCustomEvents(eventName: string, handler: CustomEventHandler): void;
}

// Type definitions
export interface BroadcastOptions {
  scope?: 'local' | 'session' | 'global';  // Event broadcast scope
  persistent?: boolean;                      // Should event be stored
  priority?: 'low' | 'normal' | 'high';    // Event priority
  timeout?: number;                         // Broadcast timeout
}

export type EventHandler = (eventType: string, data: any) => void;
export type RouteHandler = (route: string, data: any) => void;  
export type CustomEventHandler = (event: CustomEvent) => void;

// Event coordination implementation
export class WidgetEventService implements IWidgetEventService {
  public readonly serviceName = 'WidgetEventService';
  public readonly serviceVersion = '1.0.0';
  
  protected context?: WidgetServiceContext;
  protected eventHandlers = new Map<string, EventHandler[]>();
  protected routeHandlers = new Map<string, RouteHandler[]>();
  protected customEventHandlers = new Map<string, CustomEventHandler[]>();

  async initialize(context: WidgetServiceContext): Promise<void> {
    this.context = context;
    console.debug(`📡 WidgetEventService: Initialized for widget ${context.widgetName}`);
  }

  async cleanup(): Promise<void> {
    // Clear all event handlers
    this.eventHandlers.clear();
    this.routeHandlers.clear();
    this.customEventHandlers.clear();
    
    console.debug(`📡 WidgetEventService: Cleaned up`);
  }

  // Event broadcasting operations
  async broadcastEvent(eventType: string, data: any, options: BroadcastOptions = {}): Promise<void> {
    try {
      const broadcastData = {
        eventType,
        data,
        source: this.context?.widgetId || 'unknown',
        timestamp: new Date().toISOString(),
        scope: options.scope || 'session',
        priority: options.priority || 'normal'
      };

      // Local event emission first
      this.emitLocalEvent(eventType, data);

      // Route through JTAG system based on scope
      switch (options.scope) {
        case 'local':
          // Only emit locally (already done above)
          break;
          
        case 'global':
          // Send to router for global broadcast
          await this.sendToRouter('broadcast_global', broadcastData);
          break;
          
        case 'session':
        default:
          // Send to router for session broadcast
          await this.sendToRouter('broadcast_session', broadcastData);
          break;
      }

      console.debug(`📡 WidgetEventService: Broadcast event '${eventType}' (scope: ${options.scope || 'session'})`);
    } catch (error) {
      console.error(`❌ WidgetEventService: Failed to broadcast event '${eventType}':`, error);
      throw error;
    }
  }

  // Event listening operations
  addEventListener(eventType: string, handler: EventHandler): void {
    if (!this.eventHandlers.has(eventType)) {
      this.eventHandlers.set(eventType, []);
    }
    
    this.eventHandlers.get(eventType)!.push(handler);
    console.debug(`👂 WidgetEventService: Added event listener for '${eventType}'`);
  }

  removeEventListener(eventType: string, handler: EventHandler): void {
    const handlers = this.eventHandlers.get(eventType);
    if (handlers) {
      const index = handlers.indexOf(handler);
      if (index !== -1) {
        handlers.splice(index, 1);
        console.debug(`🚫 WidgetEventService: Removed event listener for '${eventType}'`);
      }
    }
  }

  // Router operations (adapter for JTAG router system)
  async sendToRouter(operation: string, data: any): Promise<any> {
    try {
      // This is the adapter layer - it will connect to actual JTAG router
      console.debug(`🔄 WidgetEventService: Router operation '${operation}'`);
      
      // For now, simulate router operations
      // In real implementation, this would use JTAG client to send to router daemon
      const result = {
        success: true,
        operation,
        data,
        timestamp: new Date().toISOString()
      };
      
      return result;
    } catch (error) {
      console.error(`❌ WidgetEventService: Router operation '${operation}' failed:`, error);
      throw error;
    }
  }

  subscribeToRoute(route: string, handler: RouteHandler): void {
    if (!this.routeHandlers.has(route)) {
      this.routeHandlers.set(route, []);
    }
    
    this.routeHandlers.get(route)!.push(handler);
    console.debug(`🛣️ WidgetEventService: Subscribed to route '${route}'`);
  }

  unsubscribeFromRoute(route: string, handler: RouteHandler): void {
    const handlers = this.routeHandlers.get(route);
    if (handlers) {
      const index = handlers.indexOf(handler);
      if (index !== -1) {
        handlers.splice(index, 1);
        console.debug(`🛣️ WidgetEventService: Unsubscribed from route '${route}'`);
      }
    }
  }

  // Widget-to-widget communication
  async sendToWidget(widgetId: string, message: any): Promise<void> {
    try {
      const widgetMessage = {
        targetWidgetId: widgetId,
        sourceWidgetId: this.context?.widgetId || 'unknown',
        message,
        timestamp: new Date().toISOString()
      };

      await this.sendToRouter('widget_message', widgetMessage);
      console.debug(`💬 WidgetEventService: Sent message to widget '${widgetId}'`);
    } catch (error) {
      console.error(`❌ WidgetEventService: Failed to send message to widget '${widgetId}':`, error);
      throw error;
    }
  }

  async sendToAllWidgets(message: any): Promise<void> {
    try {
      const broadcastMessage = {
        sourceWidgetId: this.context?.widgetId || 'unknown',
        message,
        timestamp: new Date().toISOString()
      };

      await this.sendToRouter('widget_broadcast', broadcastMessage);
      console.debug(`📢 WidgetEventService: Broadcast message to all widgets`);
    } catch (error) {
      console.error(`❌ WidgetEventService: Failed to broadcast message to all widgets:`, error);
      throw error;
    }
  }

  // Custom event coordination - environment agnostic in base class
  emitCustomEvent(eventName: string, detail: any): void {
    // Base implementation only handles local handlers
    // Environment-specific implementations will override this
    const event = {
      type: eventName,
      detail: {
        ...detail,
        source: this.context?.widgetId || 'unknown',
        timestamp: new Date().toISOString()
      }
    };
    
    this.handleCustomEvent(eventName, event as CustomEvent);
    console.debug(`🎯 WidgetEventService: Emitted custom event '${eventName}' (shared implementation)`);
  }

  subscribeToCustomEvents(eventName: string, handler: CustomEventHandler): void {
    if (!this.customEventHandlers.has(eventName)) {
      this.customEventHandlers.set(eventName, []);
    }
    
    this.customEventHandlers.get(eventName)!.push(handler);
    console.debug(`🎯 WidgetEventService: Subscribed to custom event '${eventName}' (shared implementation)`);
  }

  // Protected helper methods for extension
  protected emitLocalEvent(eventType: string, data: any): void {
    const handlers = this.eventHandlers.get(eventType);
    if (handlers) {
      handlers.forEach(handler => {
        try {
          handler(eventType, data);
        } catch (error) {
          console.error(`❌ WidgetEventService: Event handler for '${eventType}' threw error:`, error);
        }
      });
    }
  }

  protected handleCustomEvent(eventName: string, event: CustomEvent): void {
    const handlers = this.customEventHandlers.get(eventName);
    if (handlers) {
      handlers.forEach(handler => {
        try {
          handler(event);
        } catch (error) {
          console.error(`❌ WidgetEventService: Custom event handler for '${eventName}' threw error:`, error);
        }
      });
    }
  }
}