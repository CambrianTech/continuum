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
        const client = await JTAGClient.sharedInstance;
        context = client.context;
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

      if (!router) {
        const error = `Events: No router found for context ${context.environment}/${context.uuid}`;
        console.error(`❌ ${error}`);
        return { success: false, error };
      }

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

      console.log(`✅ Events: Emitted ${eventName} from ${context.environment}/${context.uuid}`);
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
}
