/**
 * EventSubscriptionManager - Environment-Agnostic Event Subscriptions
 *
 * Provides unified event subscription API that works identically in browser and server.
 * Critical for AI agents to receive the same events as human users.
 *
 * Features:
 * - Exact match subscriptions: 'data:User:created'
 * - Wildcard patterns: 'data:*:created'
 * - Elegant patterns: 'data:users {created,updated}'
 * - Filter support: { where: { roomId: 'abc' } }
 */

import { ElegantSubscriptionParser, type SubscriptionFilter, type SubscriptionPattern } from './ElegantSubscriptionParser';

/**
 * Subscription handler function type
 */
export type EventHandler<T = any> = (data: T) => void;

/**
 * Unsubscribe function returned by on()
 */
export type UnsubscribeFunction = () => void;

/**
 * Internal subscription record
 */
interface Subscription<T = any> {
  id: string;
  handler: EventHandler<T>;
  eventName: string;
}

/**
 * Wildcard subscription record
 */
interface WildcardSubscription<T = any> {
  id: string;
  pattern: RegExp;
  handler: EventHandler<T>;
  originalPattern: string;
}

/**
 * Elegant subscription record
 */
interface ElegantSubscription<T = any> {
  id: string;
  pattern: SubscriptionPattern;
  filter?: SubscriptionFilter;
  handler: EventHandler<T>;
  originalPattern: string;
}

/**
 * EventSubscriptionManager - Manages event subscriptions in a environment-agnostic way
 */
export class EventSubscriptionManager {
  private subscriptions = new Map<string, Map<string, Subscription>>();
  private wildcardSubscriptions = new Map<string, WildcardSubscription>();
  private elegantSubscriptions = new Map<string, ElegantSubscription>();
  private subscriptionCounter = 0;

  /**
   * Subscribe to an event with support for exact, wildcard, and elegant patterns
   *
   * @param patternOrEventName - Event name or pattern
   * @param handler - Event handler function
   * @param filter - Optional filter for elegant patterns
   * @returns Unsubscribe function
   *
   * @example
   * // Exact match
   * manager.on('data:User:created', (user) => console.log(user));
   *
   * // Wildcard
   * manager.on('data:*:created', (data) => console.log(data));
   *
   * // Elegant pattern
   * manager.on('data:users {created,updated}', (event) => {
   *   console.log(event.action, event.entity);
   * });
   */
  public on<T = any>(
    patternOrEventName: string,
    handler: EventHandler<T>,
    filter?: SubscriptionFilter
  ): UnsubscribeFunction {
    // Generate unique subscription ID
    const subscriptionId = `sub_${++this.subscriptionCounter}_${Date.now()}`;

    // Check if elegant pattern (starts with 'data:' and has { } or is a collection name)
    const isElegantPattern = patternOrEventName.startsWith('data:') &&
      (patternOrEventName.includes('{') || this.isCollectionPattern(patternOrEventName));

    if (isElegantPattern) {
      return this.subscribeElegant(subscriptionId, patternOrEventName, handler, filter);
    } else if (patternOrEventName.includes('*')) {
      return this.subscribeWildcard(subscriptionId, patternOrEventName, handler);
    } else {
      return this.subscribeExact(subscriptionId, patternOrEventName, handler);
    }
  }

  /**
   * Check if pattern is a collection pattern like 'data:users' or 'data:rooms'
   */
  private isCollectionPattern(pattern: string): boolean {
    const parts = pattern.split(':');
    return parts.length === 2 && parts[0] === 'data' && !parts[1].includes('{');
  }

  /**
   * Subscribe to exact event name
   */
  private subscribeExact<T>(
    subscriptionId: string,
    eventName: string,
    handler: EventHandler<T>
  ): UnsubscribeFunction {
    // Get or create subscriptions map for this event
    if (!this.subscriptions.has(eventName)) {
      this.subscriptions.set(eventName, new Map());
    }

    const eventSubs = this.subscriptions.get(eventName)!;

    // ✅ FIX: Check if identical handler already exists (prevent duplicates)
    for (const [existingId, existingSub] of eventSubs.entries()) {
      if (existingSub.handler === handler) {
        console.log(`⚠️ EventSubscriptionManager: Handler already subscribed to '${eventName}', returning existing unsubscribe (${existingId})`);
        // Return the existing unsubscribe function
        return () => {
          eventSubs.delete(existingId);
          if (eventSubs.size === 0) {
            this.subscriptions.delete(eventName);
          }
          console.log(`🔌 EventSubscriptionManager: Unsubscribed from '${eventName}' (${existingId})`);
        };
      }
    }

    eventSubs.set(subscriptionId, {
      id: subscriptionId,
      handler,
      eventName
    });

    console.log(`🎧 EventSubscriptionManager: Subscribed to exact event '${eventName}' (${subscriptionId})`);

    // Return unsubscribe function
    return () => {
      eventSubs.delete(subscriptionId);
      if (eventSubs.size === 0) {
        this.subscriptions.delete(eventName);
      }
      console.log(`🔌 EventSubscriptionManager: Unsubscribed from '${eventName}' (${subscriptionId})`);
    };
  }

  /**
   * Subscribe to wildcard pattern
   */
  private subscribeWildcard<T>(
    subscriptionId: string,
    pattern: string,
    handler: EventHandler<T>
  ): UnsubscribeFunction {
    // Convert wildcard pattern to regex
    const regexPattern = '^' + pattern.replace(/\*/g, '.*') + '$';
    const regex = new RegExp(regexPattern);

    this.wildcardSubscriptions.set(subscriptionId, {
      id: subscriptionId,
      pattern: regex,
      handler,
      originalPattern: pattern
    });

    console.log(`🎧 EventSubscriptionManager: Subscribed to wildcard pattern '${pattern}' (${subscriptionId})`);

    // Return unsubscribe function
    return () => {
      this.wildcardSubscriptions.delete(subscriptionId);
      console.log(`🔌 EventSubscriptionManager: Unsubscribed from wildcard '${pattern}' (${subscriptionId})`);
    };
  }

  /**
   * Subscribe to elegant pattern
   */
  private subscribeElegant<T>(
    subscriptionId: string,
    pattern: string,
    handler: EventHandler<T>,
    filter?: SubscriptionFilter
  ): UnsubscribeFunction {
    // Parse elegant pattern
    const parsedPattern = ElegantSubscriptionParser.parsePattern(pattern);

    this.elegantSubscriptions.set(subscriptionId, {
      id: subscriptionId,
      pattern: parsedPattern,
      filter,
      handler,
      originalPattern: pattern
    });

    console.log(`🎧 EventSubscriptionManager: Subscribed to elegant pattern '${pattern}' (${subscriptionId})`);

    // Return unsubscribe function
    return () => {
      this.elegantSubscriptions.delete(subscriptionId);
      console.log(`🔌 EventSubscriptionManager: Unsubscribed from elegant pattern '${pattern}' (${subscriptionId})`);
    };
  }

  /**
   * Unsubscribe from specific event (removes all handlers or specific handler)
   */
  public off(eventName: string, handler?: EventHandler): void {
    if (!handler) {
      // Remove all subscriptions for this event
      const count = this.subscriptions.get(eventName)?.size || 0;
      this.subscriptions.delete(eventName);
      console.log(`🔌 EventSubscriptionManager: Removed all ${count} subscription(s) for '${eventName}'`);
    } else {
      // Remove specific handler
      const eventSubs = this.subscriptions.get(eventName);
      if (eventSubs) {
        let removed = 0;
        eventSubs.forEach((sub, id) => {
          if (sub.handler === handler) {
            eventSubs.delete(id);
            removed++;
          }
        });
        if (eventSubs.size === 0) {
          this.subscriptions.delete(eventName);
        }
        console.log(`🔌 EventSubscriptionManager: Removed ${removed} specific handler(s) for '${eventName}'`);
      }
    }
  }

  /**
   * Trigger all matching subscriptions for an event
   *
   * @param eventName - Event name being triggered
   * @param data - Event data
   */
  public trigger(eventName: string, data: any): void {
    let totalTriggered = 0;

    // 1. Exact match subscriptions
    const exactSubs = this.subscriptions.get(eventName);
    if (exactSubs && exactSubs.size > 0) {
      exactSubs.forEach((subscription) => {
        try {
          subscription.handler(data);
          totalTriggered++;
        } catch (error) {
          console.error(`❌ EventSubscriptionManager: Handler error for '${eventName}' (${subscription.id}):`, error);
        }
      });
    }

    // 2. Wildcard pattern matches
    if (this.wildcardSubscriptions.size > 0) {
      this.wildcardSubscriptions.forEach((subscription) => {
        if (subscription.pattern.test(eventName)) {
          try {
            console.log(`🎯 EventSubscriptionManager: Wildcard match! '${subscription.originalPattern}' matches '${eventName}'`);
            subscription.handler(data);
            totalTriggered++;
          } catch (error) {
            console.error(`❌ EventSubscriptionManager: Wildcard handler error for '${subscription.originalPattern}' (${subscription.id}):`, error);
          }
        }
      });
    }

    // 3. Elegant pattern matches
    if (this.elegantSubscriptions.size > 0) {
      this.elegantSubscriptions.forEach((subscription) => {
        try {
          // Check if event name matches pattern
          if (ElegantSubscriptionParser.matchesPattern(eventName, subscription.pattern)) {
            // Check if event data matches filter
            if (ElegantSubscriptionParser.matchesFilter(data, subscription.filter)) {
              console.log(`🎯 EventSubscriptionManager: Elegant match! '${subscription.originalPattern}' matches '${eventName}'`);

              // Create enhanced event data with action metadata
              const enhancedEvent = ElegantSubscriptionParser.createEnhancedEvent(eventName, data);
              subscription.handler(enhancedEvent);
              totalTriggered++;
            } else {
              console.log(`🔍 EventSubscriptionManager: Pattern matched but filter rejected for '${subscription.originalPattern}'`);
            }
          }
        } catch (error) {
          console.error(`❌ EventSubscriptionManager: Elegant handler error for '${subscription.originalPattern}' (${subscription.id}):`, error);
        }
      });
    }

    if (totalTriggered > 0) {
      console.log(`✅ EventSubscriptionManager: Triggered ${totalTriggered} handler(s) for '${eventName}'`);
    }
  }

  /**
   * Get statistics about current subscriptions
   */
  public getStats(): {
    exactCount: number;
    wildcardCount: number;
    elegantCount: number;
    totalCount: number;
    eventNames: string[];
  } {
    let exactCount = 0;
    const eventNames: string[] = [];

    this.subscriptions.forEach((subs, eventName) => {
      exactCount += subs.size;
      eventNames.push(eventName);
    });

    return {
      exactCount,
      wildcardCount: this.wildcardSubscriptions.size,
      elegantCount: this.elegantSubscriptions.size,
      totalCount: exactCount + this.wildcardSubscriptions.size + this.elegantSubscriptions.size,
      eventNames
    };
  }

  /**
   * Clear all subscriptions (useful for testing/cleanup)
   */
  public clear(): void {
    const stats = this.getStats();
    this.subscriptions.clear();
    this.wildcardSubscriptions.clear();
    this.elegantSubscriptions.clear();
    console.log(`🧹 EventSubscriptionManager: Cleared ${stats.totalCount} subscription(s)`);
  }
}