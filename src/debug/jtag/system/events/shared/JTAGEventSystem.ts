// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAG Event System - Promise-Powered Event Architecture
 * 
 * Advanced event-driven messaging system with promise integration, fluent APIs,
 * and cross-context event routing. Enables reactive programming patterns across
 * the entire JTAG architecture with type-safe event handling.
 * 
 * CORE ARCHITECTURE:
 * - Type-safe event name and data mapping across all system modules
 * - Promise-based event listening with timeout support
 * - Fluent API for event stream processing (filter, take, etc.)
 * - Cross-context event routing via JTAGMessage transport
 * - Subscription management with automatic cleanup
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Event emission, subscription, and cleanup logic
 * - Integration tests: Cross-context event delivery reliability
 * - Performance tests: High-frequency event handling
 * - Concurrency tests: Multiple subscriber race conditions
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Modular event categories co-located with their source modules
 * - Promise-based waitFor() enables synchronization patterns
 * - Event stream API provides reactive programming capabilities
 * - Cross-context routing uses existing JTAG message infrastructure
 * - Subscription cleanup prevents memory leaks in long-running systems
 */

import type { JTAGMessage, JTAGContext } from '@shared/JTAGTypes';
import { type JTAGPayload, JTAGMessageFactory, createPayload } from '@shared/JTAGTypes';
import { type UUID } from '@shared/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '@shared/SystemScopes';

// Import modular event categories - co-located with their modules
import type { SystemEventData, SystemEventName } from './SystemEvents';
import type { RouterEventData, RouterEventName } from '../router/RouterEvents';
import type { TransportEventData, TransportEventName } from '@system/transports/shared/TransportEvents';
import type { ConsoleEventData, ConsoleEventName } from '@daemonsConsoleDaemon/ConsoleEvents';
import type { CommandEventData, CommandEventName } from '@daemonsCommandDaemon/CommandEvents';

// Combine all event types for the system
export type JTAGEventName = 
  | SystemEventName 
  | RouterEventName 
  | TransportEventName 
  | ConsoleEventName 
  | CommandEventName;

// Combine all event data types
export interface JTAGEventData extends 
  SystemEventData,
  RouterEventData,
  TransportEventData, 
  ConsoleEventData,
  CommandEventData {}

// Event stream interfaces for fluent API
export interface EventStream<T extends JTAGEventName> {
  take: (count: number) => Promise<JTAGMessage[]>;
  filter: (predicate: (message: JTAGMessage) => boolean) => EventStream<T>;
  close: () => void;
}

export interface EventsInterface {
  emit(eventName: string, data?: any): void;
  on(eventName: string, listener: (data?: any) => void): () => void;
  waitFor?(eventName: string, timeout?: number): Promise<any>;
}

export class EventManager {

  public listeners = new Map<string, Array<(data?: any) => void>>();

  get events(): EventsInterface {
      return {
        emit: (eventName: string, data?: any): void => { // TODO: any is bad
          const listeners = this.listeners.get(eventName) ?? [];
          listeners.forEach(listener => {
            try {
              listener(data);
            } catch (error) {
              console.error(`Event listener error for ${eventName}:`, error);
            }
          });
        },

        on: (eventName: string, listener: (data?: any) => void): (() => void) => { // TODO: any is bad
          if (!this.listeners.has(eventName)) {
            this.listeners.set(eventName, []);
          }
          this.listeners.get(eventName)!.push(listener);
          
          // Return unsubscribe function
          return () => {
            const listeners = this.listeners.get(eventName);
            if (listeners) {
              const index = listeners.indexOf(listener);
              if (index > -1) {
                listeners.splice(index, 1);
              }
            }
          };
        },

        waitFor: async (eventName: string, timeout: number = 10000): Promise<any> => { // TODO: any is bad
          return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
              unsubscribe();
              reject(new Error(`Event ${eventName} timeout after ${timeout}ms`));
            }, timeout);

            const unsubscribe = this.events.on(eventName, (data) => {
              clearTimeout(timer);
              unsubscribe();
              resolve(data);
            });
          });
        }
      };
    }
}

// Event system statistics interface
export interface EventSystemStats {
  subscriptions: number;
  queuedEvents: number;
  processing: boolean;
  contexts: string[];
}

export interface EventMessage<T extends JTAGEventName = JTAGEventName> extends JTAGPayload {
  eventName: T;
  data?: JTAGEventData[T];
  timestamp: string;
  source: string;
  encode(): string;
  equals(other: JTAGPayload): boolean;
  hashCode(): string;
}

export const createEventMessage = <T extends JTAGEventName = JTAGEventName>(
  context: JTAGContext,
  sessionId: UUID,
  eventName: T,
  data?: JTAGEventData[T],
  source?: string
): EventMessage<T> => {
  const timestamp = new Date().toISOString();
  const eventSource = source || 'event-system';
  
  return {
    ...createPayload(context, sessionId, {
      eventName,
      data,
      timestamp,
      source: eventSource
    }),
    encode(): string {
      return Buffer.from(JSON.stringify({
        eventName,
        data,
        timestamp,
        source: eventSource
      })).toString('base64');
    },
    equals(other: JTAGPayload): boolean {
      return 'eventName' in other && 'timestamp' in other && 'source' in other &&
             other.eventName === eventName && 
             other.timestamp === timestamp &&
             other.source === eventSource;
    },
    hashCode(): string {
      return `${eventName}:${eventSource}:${timestamp}`;
    }
  };
};

export interface EventFilter<T extends JTAGEventName = JTAGEventName> {
  eventName?: T | RegExp;
  source?: string | RegExp;
  context?: 'browser' | 'server' | 'remote';
  predicate?: (message: JTAGMessage) => boolean;
}

export interface EventSubscription<T extends JTAGEventName = JTAGEventName> {
  id: string;
  filter: EventFilter<T>;
  handler: (message: JTAGMessage) => Promise<unknown> | unknown;
  once: boolean;
  created: Date;
}

/**
 * Promise-Powered Event System with async/await support and cross-context routing
 */
// Forward declare JTAGRouter to avoid circular imports
interface IJTAGRouter {
  postMessage(message: JTAGMessage): Promise<any>;
}

export class JTAGEventSystem {
  private subscriptions = new Map<string, EventSubscription>();
  private eventQueue: JTAGMessage[] = [];
  private processing = false;
  private router?: IJTAGRouter;
  private context: JTAGContext;

  constructor(context: JTAGContext, router?: IJTAGRouter) {
    this.context = context;
    this.router = router;
  }

  /**
   * Emit an event with promise-like power - Type-safe event names
   */
  async emit<T extends JTAGEventName>(eventName: T, data?: JTAGEventData[T], source?: string): Promise<unknown[]> {
    const eventPayload = createEventMessage(this.context, SYSTEM_SCOPES.SYSTEM, eventName, data, source ?? 'event-system');

    const message = JTAGMessageFactory.createEvent(
      this.context,
      `${this.context.environment}/events`,
      `${this.context.environment}/events`,
      eventPayload
    );

    console.log(`📡 JTAGEventSystem: Emitting '${eventName}' from ${eventPayload.source}`);

    // Add to queue for processing
    this.eventQueue.push(message);
    
    // Process queue
    return await this.processEventQueue();
  }

  /**
   * Subscribe to events with promise-based handler - Type-safe
   */
  on<T extends JTAGEventName>(filter: EventFilter<T> | T, handler: (message: JTAGMessage) => Promise<unknown> | unknown): string {
    return this.subscribe(filter, handler, false);
  }

  /**
   * Subscribe to events (one-time) - Type-safe
   */
  once<T extends JTAGEventName>(filter: EventFilter<T> | T, handler: (message: JTAGMessage) => Promise<unknown> | unknown): string {
    return this.subscribe(filter, handler, true);
  }

  /**
   * Wait for an event (Promise-based) - Type-safe with event data
   */
  async waitFor<T extends JTAGEventName>(
    eventName: T, 
    timeout?: number
  ): Promise<{ message: JTAGMessage; data: JTAGEventData[T] }> {
    return new Promise((resolve, reject) => {
      const timeoutHandle = timeout ? setTimeout(() => {
        this.unsubscribe(subscriptionId);
        reject(new Error(`Event wait timeout after ${timeout}ms`));
      }, timeout) : null;

      const subscriptionId = this.once(eventName, (message) => {
        if (timeoutHandle) clearTimeout(timeoutHandle);
        const eventPayload = message.payload as EventMessage<T>;
        resolve({ 
          message, 
          data: eventPayload.data as JTAGEventData[T] 
        });
      });
    });
  }

  /**
   * Create event streams (Promise-like chaining)
   */
  stream<T extends JTAGEventName>(filter: EventFilter<T> | T): EventStream<T> {
    const events: JTAGMessage[] = [];
    let streamHandler: ((message: JTAGMessage) => void) | null = null;

    const subscriptionId = this.on(filter, (message) => {
      events.push(message);
      if (streamHandler) streamHandler(message);
    });

    return {
      take: (count: number): Promise<JTAGMessage[]> => {
        return new Promise((resolve) => {
          if (events.length >= count) {
            resolve(events.splice(0, count));
            return;
          }

          let collected = 0;
          const results: JTAGMessage[] = [];

          streamHandler = (message) => {
            results.push(message);
            collected++;

            if (collected >= count) {
              streamHandler = null;
              resolve(results);
            }
          };
        });
      },

      filter: (predicate: (message: JTAGMessage) => boolean): EventStream<T> => {
        const filteredFilter: EventFilter<T> = typeof filter === 'string' 
          ? { eventName: filter as T, predicate }
          : { ...filter, predicate };
        
        return this.stream(filteredFilter);
      },

      close: (): void => {
        this.unsubscribe(subscriptionId);
        streamHandler = null;
      }
    };
  }

  private subscribe<T extends JTAGEventName>(filter: EventFilter<T> | T, handler: (message: JTAGMessage) => Promise<unknown> | unknown, once: boolean): string {
    const subscription: EventSubscription<T> = {
      id: `sub_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`,
      filter: typeof filter === 'string' ? { eventName: filter } : filter,
      handler,
      once,
      created: new Date()
    };

    this.subscriptions.set(subscription.id, subscription);
    
    console.log(`📋 JTAGEventSystem: Subscribed to events (${subscription.id})`);
    
    return subscription.id;
  }

  /**
   * Unsubscribe from events
   */
  unsubscribe(subscriptionId: string): boolean {
    const removed = this.subscriptions.delete(subscriptionId);
    if (removed) {
      console.log(`🗑️ JTAGEventSystem: Unsubscribed (${subscriptionId})`);
    }
    return removed;
  }

  /**
   * Process the event queue with promise handling
   */
  private async processEventQueue(): Promise<unknown[]> {
    if (this.processing || this.eventQueue.length === 0) {
      return [];
    }

    this.processing = true;
    const results: unknown[] = [];

    try {
      const eventsToProcess = [...this.eventQueue];
      this.eventQueue = [];

      for (const message of eventsToProcess) {
        const eventResults = await this.processEvent(message);
        results.push(...eventResults);
      }

      return results;
    } finally {
      this.processing = false;
      
      // Process any events that were added during processing
      if (this.eventQueue.length > 0) {
        setTimeout(() => this.processEventQueue(), 0);
      }
    }
  }

  /**
   * Process a single event through all matching subscriptions
   */
  private async processEvent(message: JTAGMessage): Promise<unknown[]> {
    const results: unknown[] = [];
    const subscriptionsToRemove: string[] = [];

    for (const [id, subscription] of this.subscriptions) {
      if (this.matchesFilter(message, subscription.filter)) {
        try {
          console.log(`🎯 JTAGEventSystem: Processing '${(message.payload as EventMessage).eventName}' for subscription ${id}`);
          
          const result = await subscription.handler(message);
          results.push(result);

          if (subscription.once) {
            subscriptionsToRemove.push(id);
          }
        } catch (error) {
          console.error(`❌ JTAGEventSystem: Handler error for '${(message.payload as EventMessage).eventName}':`, error);
          results.push({ error: error instanceof Error ? error.message : String(error) });
        }
      }
    }

    // Remove one-time subscriptions
    subscriptionsToRemove.forEach(id => this.subscriptions.delete(id));

    return results;
  }

  /**
   * Check if event message matches subscription filter
   */
  private matchesFilter(message: JTAGMessage, filter: EventFilter): boolean {
    const eventPayload = message.payload as EventMessage; // EventMessage type
    
    // Event name matching
    if (filter.eventName) {
      if (filter.eventName instanceof RegExp) {
        if (!filter.eventName.test(eventPayload.eventName)) return false;
      } else {
        if (filter.eventName !== eventPayload.eventName) return false;
      }
    }

    // Source matching
    if (filter.source) {
      if (filter.source instanceof RegExp) {
        if (!filter.source.test(eventPayload.source)) return false;
      } else {
        if (filter.source !== eventPayload.source) return false;
      }
    }

    // Context matching
    if (filter.context && filter.context !== message.context.environment) {
      return false;
    }

    // Custom predicate
    if (filter.predicate && !filter.predicate(message)) {
      return false;
    }

    return true;
  }


  /**
   * Enable cross-context event routing via router
   */
  enableCrossContextRouting(router: IJTAGRouter): void {
    this.router = router;
    console.log(`🌐 JTAGEventSystem: Cross-context routing enabled`);
  }

  /**
   * Send event to different context (browser ↔ server)
   */
  async emitCrossContext<T extends JTAGEventName>(targetContext: 'browser' | 'server', eventName: T, data?: JTAGEventData[T]): Promise<void> {
    if (!this.router) {
      throw new Error('Cross-context routing not enabled');
    }

    const payload = createEventMessage(this.context, SYSTEM_SCOPES.SYSTEM, eventName, data, 'cross-context');
    
    const message = JTAGMessageFactory.createEvent(
      this.context,
      `${this.context.environment}/events`,
      `${targetContext}/events`,
      payload
    );
    await this.router.postMessage(message);
  }

  /**
   * Get system statistics
   */
  getStats(): EventSystemStats {
    return {
      subscriptions: this.subscriptions.size,
      queuedEvents: this.eventQueue.length,
      processing: this.processing,
      contexts: ['browser', 'server', 'remote']
    };
  }

  /**
   * Shutdown the event system
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 JTAGEventSystem: Shutting down...`);
    
    // Process remaining events
    if (this.eventQueue.length > 0) {
      await this.processEventQueue();
    }
    
    // Clear subscriptions
    this.subscriptions.clear();
    this.eventQueue = [];
    
    console.log(`✅ JTAGEventSystem: Shutdown complete`);
  }
}