/**
 * Events - Universal Event System (Shared/Browser/Server)
 *
 * ✨ MAGIC: Events.emit(eventName, data) works ANYWHERE
 * - Server code: await Events.emit('data:users:created', user)
 * - Browser code: await Events.emit('chat:message', msg)
 * - Shared code: await Events.emit('system:ready', {})
 *
 * Auto-discovers context+router, no passing needed!
 * Overload: Events.emit(context, eventName, data) for explicit context
 */

import type { JTAGContext } from '../types/JTAGTypes';
import { JTAGMessageFactory } from '../types/JTAGTypes';
import { JTAG_ENDPOINTS } from '../router/shared/JTAGEndpoints';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';
import type { EventBridgePayload } from '../../events/shared/EventSystemTypes';
import type { EventScope } from '../../events/shared/EventSystemConstants';
import { RouterRegistry } from './RouterRegistry';
import { BaseEntity } from '../../data/entities/BaseEntity';
import { ElegantSubscriptionParser, type SubscriptionFilter } from '../../events/shared/ElegantSubscriptionParser';

export interface EventEmitOptions {
  scope?: EventScope;
  scopeId?: string;
  sessionId?: string;
}

/**
 * Universal Events Interface - Works Anywhere, Any Environment
 *
 * Auto-discovers context+router, auto-routes events across environments,
 * auto-handles scoping. Developer just provides event + data = magic!
 */
export class Events {
  /**
   * ✨ Universal event emission - works in server, browser, shared code
   *
   * TWO FORMS:
   * 1. Auto-context: await Events.emit('data:users:created', userEntity)
   * 2. Explicit context: await Events.emit(context, 'data:users:created', userEntity)
   *
   * @example
   * // In DataDaemon (shared) - auto-context!
   * await Events.emit('data:users:created', userEntity);
   *
   * // In PersonaUser (shared) - explicit context
   * await Events.emit(this.context, 'chat:message:sent', message);
   *
   * // In Widget (browser) - auto-context!
   * await Events.emit('ui:theme:changed', theme);
   */
  static async emit<T>(
    contextOrEventName: JTAGContext | string,
    eventNameOrData: string | T,
    eventDataOrOptions?: T | EventEmitOptions,
    optionsParam?: EventEmitOptions
  ): Promise<{ success: boolean; error?: string }> {
    try {
      // Parse overloaded parameters
      let context: JTAGContext;
      let eventName: string;
      let eventData: T;
      let options: EventEmitOptions;

      if (typeof contextOrEventName === 'string') {
        // Form 1: emit(eventName, data, options?)
        // Auto-discover context from JTAGClient.sharedInstance
        const { JTAGClient } = await import('../client/shared/JTAGClient');

        try {
          const client = await JTAGClient.sharedInstance;
          context = client.context;
        } catch (error) {
          // sharedInstance not ready yet (browser initialization race)
          // Use minimal fallback context - will trigger DOM-only event path
          const isBrowserRuntime = typeof document !== 'undefined';
          if (isBrowserRuntime) {
            // Create minimal context for DOM-only events
            const { generateUUID } = await import('../types/CrossPlatformUUID');
            context = {
              uuid: generateUUID(),
              environment: 'browser' as const,
              config: {} as any,
              getConfig: () => ({} as any)
            };
          } else {
            // Server runtime - re-throw error
            throw error;
          }
        }

        eventName = contextOrEventName;
        eventData = eventNameOrData as T;
        options = (eventDataOrOptions as EventEmitOptions) ?? {};
      } else {
        // Form 2: emit(context, eventName, data, options?)
        context = contextOrEventName;
        eventName = eventNameOrData as string;
        eventData = eventDataOrOptions as T;
        options = optionsParam ?? {};
      }

      // Auto-discover router from context
      const router = RouterRegistry.getForContext(context);

      // Check runtime environment (not context.environment, which is the connected server's environment)
      const isBrowserRuntime = typeof document !== 'undefined';

      if (!router) {
        // If no router found and we're running in browser, fall back to DOM-only events
        if (isBrowserRuntime) {
          console.log(`🌐 Events: No router for context ${context.environment}/${context.uuid}, using DOM-only event for ${eventName}`);

          // Trigger wildcard/pattern subscriptions
          this.checkWildcardSubscriptions(eventName, eventData);

          // Dispatch DOM event for direct listeners
          const domEvent = new CustomEvent(eventName, {
            detail: eventData,
            bubbles: true
          });
          document.dispatchEvent(domEvent);

          console.log(`✅ Events: Emitted DOM-only event ${eventName}`);
          return { success: true };
        } else {
          // Server runtime without router is an error
          const error = `Events: No router found for context ${context.environment}/${context.uuid}`;
          console.error(`❌ ${error}`);
          return { success: false, error };
        }
      }

      // Router found - use full EventBridge routing
      // Create event payload
      const eventPayload: EventBridgePayload = {
        context,
        sessionId: options.sessionId ?? context.uuid,
        type: 'event-bridge',
        scope: {
          type: options.scope ?? EVENT_SCOPES.GLOBAL,
          id: options.scopeId ?? '',
          sessionId: options.sessionId ?? context.uuid
        },
        eventName,
        data: eventData as Record<string, unknown>,
        originSessionId: options.sessionId ?? context.uuid,
        originContextUUID: context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message
      const eventMessage = JTAGMessageFactory.createEvent(
        context,
        'events',
        JTAG_ENDPOINTS.EVENTS.BASE,
        eventPayload
      );

      // Route event through discovered router
      await router.postMessage(eventMessage);

      // Trigger local wildcard/pattern/elegant subscriptions (server + browser)
      // checkWildcardSubscriptions() handles both wildcard AND elegant subscriptions
      this.checkWildcardSubscriptions(eventName, eventData);

      // Also dispatch DOM events if running in browser
      if (isBrowserRuntime) {
        // Dispatch DOM event for direct listeners
        const domEvent = new CustomEvent(eventName, {
          detail: eventData,
          bubbles: true
        });
        document.dispatchEvent(domEvent);
      }

      // console.log(`✅ Events: Emitted ${eventName} from ${context.environment}/${context.uuid}`);
      return { success: true };

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Events: Failed to emit:`, error);
      return { success: false, error: errorMsg };
    }
  }

  /**
   * Emit CRUD event for an entity - convenience wrapper
   *
   * TWO FORMS:
   * 1. Auto-context: await Events.emitCrud('created', 'users', userEntity)
   * 2. Explicit context: await Events.emitCrud(context, 'created', 'users', userEntity)
   *
   * @example
   * await Events.emitCrud('created', 'users', userEntity);
   * await Events.emitCrud(context, 'updated', 'rooms', roomEntity);
   */
  static async emitCrud<T extends BaseEntity>(
    contextOrOperation: JTAGContext | 'created' | 'updated' | 'deleted',
    operationOrCollection: 'created' | 'updated' | 'deleted' | string,
    collectionOrEntity: string | T,
    entityParam?: T
  ): Promise<{ success: boolean; error?: string }> {
    // Parse overloaded parameters
    let context: JTAGContext | undefined;
    let operation: 'created' | 'updated' | 'deleted';
    let collection: string;
    let entity: T;

    if (typeof contextOrOperation === 'object' && 'environment' in contextOrOperation) {
      // Form 2: emitCrud(context, operation, collection, entity)
      context = contextOrOperation;
      operation = operationOrCollection as 'created' | 'updated' | 'deleted';
      collection = collectionOrEntity as string;
      entity = entityParam as T;
    } else {
      // Form 1: emitCrud(operation, collection, entity)
      operation = contextOrOperation as 'created' | 'updated' | 'deleted';
      collection = operationOrCollection as string;
      entity = collectionOrEntity as T;
    }

    const eventName = BaseEntity.getEventName(collection, operation);

    if (context) {
      return await this.emit(context, eventName, entity);
    } else {
      return await this.emit(eventName, entity);
    }
  }

  /**
   * ✨ Universal event subscription - works in browser (DOM events), server (EventBridge), shared code
   *
   * @param subscriberId Optional ID to enable deduplication. If provided and subscription already exists for this {subscriberId, eventName}, replaces it instead of creating duplicate.
   *
   * @example
   * // Elegant pattern matching - multiple actions
   * Events.subscribe('data:users {created,updated}', (user) => console.log(user));
   *
   * // With filtering
   * Events.subscribe('data:rooms', (room) => console.log(room), { where: { public: true } });
   *
   * // With deduplication (prevents duplicate subscriptions)
   * Events.subscribe('chat:message', handler, undefined, personaId);  // Replaces previous subscription for same persona
   *
   * // Wildcard patterns
   * Events.subscribe('data:*:created', (entity) => console.log('Created:', entity));
   *
   * // Simple event subscription
   * Events.subscribe('chat:message', (msg) => console.log(msg));
   */
  static subscribe<T>(
    patternOrEventName: string,
    listener: (eventData: T) => void,
    filter?: SubscriptionFilter,
    subscriberId?: string
  ): () => void {
    try {
      //console.log(`🎧 Events: Subscribing to ${patternOrEventName}`);

      // Check if we're in browser environment (document available)
      const isBrowser = typeof document !== 'undefined';

      // Check if this is an elegant pattern (contains data: with optional {})
      const isElegantPattern = patternOrEventName.startsWith('data:') &&
        (patternOrEventName.includes('{') || !patternOrEventName.includes(':'));

      if (isElegantPattern) {
        // Parse elegant pattern
        const parsedPattern = ElegantSubscriptionParser.parsePattern(patternOrEventName);
        console.log(`🎯 Events: Parsed elegant pattern:`, parsedPattern);

        // Create subscription ID - use subscriberId if provided for deduplication
        const subscriptionId = subscriberId
          ? `${patternOrEventName}_${subscriberId}`
          : `${patternOrEventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

        // Store elegant subscriptions in registry
        this.elegantSubscriptions ??= new Map();

        // Check if replacing existing subscription (deduplication)
        const existingSubscription = this.elegantSubscriptions.get(subscriptionId);
        if (existingSubscription) {
          console.log(`🔄 Events: Replacing existing subscription ${subscriptionId} for pattern ${patternOrEventName}`);
        }

        this.elegantSubscriptions.set(subscriptionId, {
          pattern: parsedPattern,
          filter,
          listener,
          originalPattern: patternOrEventName
        });

        console.log(`🎧 Events: ${existingSubscription ? 'Replaced' : 'Added'} elegant subscription ${subscriptionId} for pattern ${patternOrEventName}`);

        // Return unsubscribe function
        return () => {
          this.elegantSubscriptions?.delete(subscriptionId);
          console.log(`🔌 Events: Unsubscribed elegant pattern ${patternOrEventName} (${subscriptionId})`);
        };

      } else if (patternOrEventName.includes('*')) {
        // Legacy wildcard support
        const pattern = new RegExp('^' + patternOrEventName.replace(/\*/g, '.*') + '$');

        // Create subscription ID - use subscriberId if provided for deduplication
        const subscriptionId = subscriberId
          ? `${patternOrEventName}_${subscriberId}`
          : `${patternOrEventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

        this.wildcardSubscriptions ??= new Map();

        // Check if replacing existing subscription (deduplication)
        const existingSubscription = this.wildcardSubscriptions.get(subscriptionId);
        if (existingSubscription) {
          console.log(`🔄 Events: Replacing existing wildcard subscription ${subscriptionId} for pattern ${patternOrEventName}`);
        }

        this.wildcardSubscriptions.set(subscriptionId, {
          pattern,
          listener,
          eventName: patternOrEventName
        });

        //console.log(`🎧 Events: ${existingSubscription ? 'Replaced' : 'Added'} wildcard subscription ${subscriptionId}`);

        return () => {
          this.wildcardSubscriptions?.delete(subscriptionId);
        };

      } else if (isBrowser) {
        // Regular exact match subscription (browser only - DOM events)
        const eventHandler = (event: Event): void => {
          const customEvent = event as CustomEvent<T>;
          listener(customEvent.detail);
        };

        document.addEventListener(patternOrEventName, eventHandler);

        return () => {
          document.removeEventListener(patternOrEventName, eventHandler);
          //console.log(`🔌 Events: Unsubscribed from ${patternOrEventName}`);
        };
      } else {
        // Server environment - store exact-match subscriptions in map

        // Create subscription ID - use subscriberId if provided for deduplication
        const subscriptionId = subscriberId
          ? `${patternOrEventName}_${subscriberId}`
          : `${patternOrEventName}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;

        this.exactMatchSubscriptions ??= new Map();

        // Check if replacing existing subscription (deduplication)
        const existingSubscription = this.exactMatchSubscriptions.get(subscriptionId);
        if (existingSubscription) {
          console.log(`🔄 Events: Replacing existing exact-match subscription ${subscriptionId} for ${patternOrEventName}`);
        }

        this.exactMatchSubscriptions.set(subscriptionId, {
          eventName: patternOrEventName,
          listener,
          filter
        });

        //console.log(`✅ Events: ${existingSubscription ? 'Replaced' : 'Added'} exact-match server subscription for ${patternOrEventName} (${subscriptionId})`);

        return () => {
          this.exactMatchSubscriptions?.delete(subscriptionId);
          //console.log(`🔌 Events: Unsubscribed from ${patternOrEventName} (${subscriptionId})`);
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
    pattern: import('../../events/shared/ElegantSubscriptionParser').SubscriptionPattern;
    filter?: SubscriptionFilter;
    listener: (data: any) => void;
    originalPattern: string;
  }>;

  // Storage for exact-match server subscriptions
  private static exactMatchSubscriptions?: Map<string, {
    eventName: string;
    listener: (data: any) => void;
    filter?: SubscriptionFilter;
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
              // console.log(`🔍 Events: Pattern matched but filter rejected for ${subscription.originalPattern}`);
            }
          }
        } catch (error) {
          console.error(`❌ Events: Elegant subscription error for ${subscriptionId}:`, error);
        }
      });
    }

    // Check exact-match server subscriptions
    if (this.exactMatchSubscriptions && this.exactMatchSubscriptions.size > 0) {
      this.exactMatchSubscriptions.forEach((subscription, subscriptionId) => {
        if (subscription.eventName === eventName) {
          try {
            // Check if event data matches filters
            if (ElegantSubscriptionParser.matchesFilter(eventData, subscription.filter)) {
              // console.log(`🎯 Events: Exact-match server subscription triggered for ${eventName} (${subscriptionId})`);
              subscription.listener(eventData);
              totalMatchCount++;
            } else {
              // console.log(`🔍 Events: Exact-match matched but filter rejected for ${eventName}`);
            }
          } catch (error) {
            console.error(`❌ Events: Exact-match subscription error for ${subscriptionId}:`, error);
          }
        }
      });
    }

    // if (totalMatchCount > 0) {
    //   console.log(`🎯 Events: Triggered ${totalMatchCount} subscription(s) for ${eventName}`);
    // }
  }
}
