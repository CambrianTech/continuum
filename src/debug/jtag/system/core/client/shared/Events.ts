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
      const eventMessage = JTAGMessageFactory.createEvent(
        jtagClient.context,
        'events',
        JTAG_ENDPOINTS.EVENTS.BRIDGE,
        eventPayload
      );

      // Route event through Router - same mechanism as DataCreateServerCommand
      const router = (jtagClient as any).getRouter();
      const result = await router.postMessage(eventMessage);

      // Also dispatch DOM event for local subscribers (bridge server→browser)
      if (typeof document !== 'undefined') {
        const domEvent = new CustomEvent(eventName, {
          detail: eventData,
          bubbles: true
        });
        document.dispatchEvent(domEvent);
        console.log(`📨 Events: Also dispatched DOM event ${eventName}`);

        // Also check wildcard subscriptions for pattern matches
        this.checkWildcardSubscriptions(eventName, eventData);
      }

      console.log(`📨 Events: Emitted ${eventName} via EventBridge`, result);

      return { success: true };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Events: Failed to emit ${eventName}:`, error);
      return { success: false, error: errorMsg };
    }
  }

  /**
   * Subscribe to an event with clean interface and wildcard support
   *
   * @example Events.subscribe('data:User:*', handler)  // All User CRUD operations
   * @example Events.subscribe('data:User:updated', handler)  // Specific operation
   */
  static subscribe<T>(
    eventName: string,
    listener: (eventData: T) => void,
    options: EventEmitOptions = {}
  ): () => void {
    try {
      console.log(`🎧 Events: Subscribing to ${eventName}`);

      // Check if this is a wildcard subscription (contains *)
      const isWildcard = eventName.includes('*');

      if (isWildcard) {
        // Convert wildcard pattern to regex (e.g., 'data:User:*' -> /^data:User:.*$/)
        const pattern = new RegExp('^' + eventName.replace(/\*/g, '.*') + '$');
        console.log(`🎯 Events: Created wildcard pattern ${pattern} for ${eventName}`);

        // Store wildcard subscriptions in a registry for pattern matching
        if (!this.wildcardSubscriptions) {
          this.wildcardSubscriptions = new Map();
        }

        const subscriptionId = `${eventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
        this.wildcardSubscriptions.set(subscriptionId, { pattern, listener, eventName });

        console.log(`🎧 Events: Added wildcard subscription ${subscriptionId} for pattern ${eventName}`);

        // Return unsubscribe function for wildcard
        return () => {
          this.wildcardSubscriptions?.delete(subscriptionId);
          console.log(`🔌 Events: Unsubscribed wildcard pattern ${eventName} (${subscriptionId})`);
        };
      } else {
        // Regular exact match subscription
        const eventHandler = (event: Event) => {
          const customEvent = event as CustomEvent<T>;
          listener(customEvent.detail);
        };

        document.addEventListener(eventName, eventHandler);

        // Return unsubscribe function
        return () => {
          document.removeEventListener(eventName, eventHandler);
          console.log(`🔌 Events: Unsubscribed from ${eventName}`);
        };
      }
    } catch (error) {
      console.error(`❌ Events: Failed to subscribe to ${eventName}:`, error);
      return () => {}; // No-op unsubscribe
    }
  }

  // Storage for wildcard subscriptions
  private static wildcardSubscriptions?: Map<string, { pattern: RegExp; listener: (data: any) => void; eventName: string }>;

  /**
   * Check if any wildcard subscriptions match the emitted event
   * Made public so EventsDaemonBrowser can trigger wildcard subscriptions for EventBridge events
   */
  public static checkWildcardSubscriptions(eventName: string, eventData: any): void {
    if (!this.wildcardSubscriptions || this.wildcardSubscriptions.size === 0) {
      return;
    }

    let matchCount = 0;
    this.wildcardSubscriptions.forEach((subscription, subscriptionId) => {
      if (subscription.pattern.test(eventName)) {
        try {
          console.log(`🎯 Events: Wildcard match! ${subscription.eventName} pattern matches ${eventName}`);

          // Create a synthetic event object with the actual event name
          const syntheticEvent = {
            detail: eventData,
            type: eventName,
            target: null,
            currentTarget: null
          };

          // Pass the synthetic event to mimic DOM event structure
          subscription.listener(eventData);
          matchCount++;
        } catch (error) {
          console.error(`❌ Events: Wildcard listener error for ${subscriptionId}:`, error);
        }
      }
    });

    if (matchCount > 0) {
      console.log(`🎯 Events: Triggered ${matchCount} wildcard subscription(s) for ${eventName}`);
    }
  }
}