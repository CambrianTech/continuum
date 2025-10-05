/**
 * Events - Universal Static Interface
 *
 * Clean interface for event emission and subscription that works everywhere.
 * Server: Uses EventBridge mechanism like Commands.execute<T>()
 * Browser: Uses DOM events for subscription, EventBridge for emission
 */

import { JTAGClient } from './JTAGClient';
import { JTAGMessageFactory } from '../../types/JTAGTypes';
import { JTAG_ENDPOINTS } from '../../router/shared/JTAGEndpoints';
import { EVENT_SCOPES, EventRoutingUtils } from '../../../events/shared/EventSystemConstants';
import type { EventBridgePayload } from '../../../events/shared/EventSystemTypes';
import type { EventScope } from '../../../events/shared/EventSystemConstants';
import { ElegantSubscriptionParser, type SubscriptionFilter } from '../../../events/shared/ElegantSubscriptionParser';

export interface EventEmitOptions {
  scope?: EventScope;
  scopeId?: string;
  sessionId?: string;
}

/**
 * Universal Events Interface - Works on Server and Browser
 * Same elegance as Commands.execute<T>()
 */
export class Events {
  /**
   * Emit an event with clean interface - Works on Server and Browser
   * Same elegance as Commands.execute<T>()
   *
   * @example Server: await Events.emit<UserEntity>('data:User:created', userEntity)
   * @example Browser: await Events.emit<ChatMessage>('chat:message-sent', message)
   */
  static async emit<T>(
    eventName: string,
    eventData: T,
    options: EventEmitOptions = {}
  ): Promise<{ success: boolean; error?: string }> {
    try {
      // Get the shared JTAG client instance
      const jtagClient = await JTAGClient.sharedInstance;

      // Build event payload
      const eventPayload: EventBridgePayload = {
        context: jtagClient.context,
        sessionId: options.sessionId || jtagClient.sessionId,
        type: 'event-bridge',
        scope: {
          type: options.scope || EVENT_SCOPES.GLOBAL,
          id: options.scopeId || '',
          sessionId: options.sessionId || jtagClient.sessionId || jtagClient.context.uuid
        },
        eventName,
        data: EventRoutingUtils.addBridgeMetadata(
          eventData,
          options.sessionId || jtagClient.sessionId,
          new Date().toISOString()
        ),
        originSessionId: options.sessionId || jtagClient.sessionId,
        originContextUUID: jtagClient.context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message using JTAG message factory
      // Route to 'events' endpoint (EventsDaemon.subpath), not 'event-bridge'
      const eventMessage = JTAGMessageFactory.createEvent(
        jtagClient.context,
        'events',
        JTAG_ENDPOINTS.EVENTS.BASE,
        eventPayload
      );

      // Try to route event through Router - if it fails, we still dispatch DOM event
      const router = (jtagClient as any).getRouter();
      let routerSuccess = false;
      try {
        const result = await router.postMessage(eventMessage);
        console.log(`📨 Events: Emitted ${eventName} via EventBridge`, result);
        routerSuccess = true;
      } catch (routerError) {
        // EventBridge routing failed, but we'll still dispatch DOM event for local subscribers
        console.warn(`⚠️ Events: EventBridge routing failed for ${eventName}, using DOM fallback:`, routerError);
      }

      // Always dispatch DOM event for local subscribers (even if EventBridge fails)
      if (typeof document !== 'undefined') {
        const domEvent = new CustomEvent(eventName, {
          detail: eventData,
          bubbles: true
        });
        document.dispatchEvent(domEvent);
        console.log(`📨 Events: Dispatched DOM event ${eventName} (bridge=${routerSuccess})`);

        // Also check wildcard subscriptions for pattern matches
        this.checkWildcardSubscriptions(eventName, eventData);
      }

      return { success: true };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Events: Failed to emit ${eventName}:`, error);
      return { success: false, error: errorMsg };
    }
  }

  /**
   * Subscribe to events with elegant patterns and filtering
   *
   * @example Events.subscribe('data:users {created,updated}', handler)  // Multiple actions
   * @example Events.subscribe('data:users', handler, { where: { roomId: 'uuid' } })  // With filtering
   * @example Events.subscribe('data:users:uuid {updated}', handler)  // Specific entity
   */
  static subscribe<T>(
    patternOrEventName: string,
    listener: (eventData: T) => void,
    filter?: SubscriptionFilter
  ): () => void {
    try {
      console.log(`🎧 Events: Subscribing to ${patternOrEventName}`);

      // Check if this is an elegant pattern (contains data: with optional {})
      const isElegantPattern = patternOrEventName.startsWith('data:') &&
        (patternOrEventName.includes('{') || !patternOrEventName.includes(':'));

      if (isElegantPattern) {
        // Parse elegant pattern
        const parsedPattern = ElegantSubscriptionParser.parsePattern(patternOrEventName);
        console.log(`🎯 Events: Parsed elegant pattern:`, parsedPattern);

        // Create subscription ID
        const subscriptionId = `${patternOrEventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

        // Store elegant subscriptions in registry
        if (!this.elegantSubscriptions) {
          this.elegantSubscriptions = new Map();
        }

        this.elegantSubscriptions.set(subscriptionId, {
          pattern: parsedPattern,
          filter,
          listener,
          originalPattern: patternOrEventName
        });

        console.log(`🎧 Events: Added elegant subscription ${subscriptionId} for pattern ${patternOrEventName}`);

        // Return unsubscribe function
        return () => {
          this.elegantSubscriptions?.delete(subscriptionId);
          console.log(`🔌 Events: Unsubscribed elegant pattern ${patternOrEventName} (${subscriptionId})`);
        };

      } else if (patternOrEventName.includes('*')) {
        // Legacy wildcard support
        const pattern = new RegExp('^' + patternOrEventName.replace(/\*/g, '.*') + '$');
        const subscriptionId = `${patternOrEventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

        if (!this.wildcardSubscriptions) {
          this.wildcardSubscriptions = new Map();
        }

        this.wildcardSubscriptions.set(subscriptionId, {
          pattern,
          listener,
          eventName: patternOrEventName
        });

        return () => {
          this.wildcardSubscriptions?.delete(subscriptionId);
        };

      } else {
        // Regular exact match subscription
        const eventHandler = (event: Event) => {
          const customEvent = event as CustomEvent<T>;
          listener(customEvent.detail);
        };

        document.addEventListener(patternOrEventName, eventHandler);

        return () => {
          document.removeEventListener(patternOrEventName, eventHandler);
          console.log(`🔌 Events: Unsubscribed from ${patternOrEventName}`);
        };
      }
    } catch (error) {
      console.error(`❌ Events: Failed to subscribe to ${patternOrEventName}:`, error);
      return () => {}; // No-op unsubscribe
    }
  }

  // Storage for wildcard subscriptions
  private static wildcardSubscriptions?: Map<string, { pattern: RegExp; listener: (data: any) => void; eventName: string }>;

  // Storage for elegant subscriptions
  private static elegantSubscriptions?: Map<string, {
    pattern: import('../../../events/shared/ElegantSubscriptionParser').SubscriptionPattern;
    filter?: SubscriptionFilter;
    listener: (data: any) => void;
    originalPattern: string;
  }>;

  /**
   * Check if any pattern subscriptions match the emitted event
   * Made public so EventsDaemonBrowser can trigger subscriptions for EventBridge events
   */
  public static checkWildcardSubscriptions(eventName: string, eventData: any): void {
    let totalMatchCount = 0;

    // Check wildcard subscriptions
    if (this.wildcardSubscriptions && this.wildcardSubscriptions.size > 0) {
      this.wildcardSubscriptions.forEach((subscription, subscriptionId) => {
        if (subscription.pattern.test(eventName)) {
          try {
            console.log(`🎯 Events: Wildcard match! ${subscription.eventName} pattern matches ${eventName}`);
            subscription.listener(eventData);
            totalMatchCount++;
          } catch (error) {
            console.error(`❌ Events: Wildcard listener error for ${subscriptionId}:`, error);
          }
        }
      });
    }

    // Check elegant subscriptions
    if (this.elegantSubscriptions && this.elegantSubscriptions.size > 0) {
      this.elegantSubscriptions.forEach((subscription, subscriptionId) => {
        try {
          // Check if event matches pattern
          if (ElegantSubscriptionParser.matchesPattern(eventName, subscription.pattern)) {
            // Check if event data matches filters
            if (ElegantSubscriptionParser.matchesFilter(eventData, subscription.filter)) {
              console.log(`🎯 Events: Elegant pattern match! ${subscription.originalPattern} matches ${eventName}`);

              // Create enhanced event data with action metadata
              const enhancedEvent = ElegantSubscriptionParser.createEnhancedEvent(eventName, eventData);
              subscription.listener(enhancedEvent);
              totalMatchCount++;
            } else {
              console.log(`🔍 Events: Pattern matched but filter rejected for ${subscription.originalPattern}`);
            }
          }
        } catch (error) {
          console.error(`❌ Events: Elegant subscription error for ${subscriptionId}:`, error);
        }
      });
    }

    if (totalMatchCount > 0) {
      console.log(`🎯 Events: Triggered ${totalMatchCount} subscription(s) for ${eventName}`);
    }
  }
}