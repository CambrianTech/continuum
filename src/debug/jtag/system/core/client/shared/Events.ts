/**
 * Events - Clean Client Interface
 *
 * Provides simple static interface for event emission.
 * Replaces manual EventBridge payload creation.
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

export class Events {
  /**
   * Emit an event with clean interface
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
          id: options.scopeId,
          sessionId: options.sessionId || jtagClient.sessionId
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

      // Route event through Router
      const result = await jtagClient.router.postMessage(eventMessage);

      console.log(`📨 Events: Emitted ${eventName} (${options.scope || 'global'})`, result);

      return { success: true };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Events: Failed to emit ${eventName}:`, error);
      return { success: false, error: errorMsg };
    }
  }
}