/**
 * Events - Server Implementation
 *
 * Server-specific event emission using EventBridge mechanism
 * Same pattern as Commands.execute<T>() but for events
 */

import type { JTAGContext } from '../../types/JTAGTypes';
import { JTAGMessageFactory } from '../../types/JTAGTypes';
import { JTAG_ENDPOINTS } from '../../router/shared/JTAGEndpoints';
import { EVENT_SCOPES } from '../../../events/shared/EventSystemConstants';
import type { EventBridgePayload } from '../../../events/shared/EventSystemTypes';
import type { EventScope } from '../../../events/shared/EventSystemConstants';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';

export interface EventEmitOptions {
  scope?: EventScope;
  scopeId?: string;
  sessionId?: string;
}

/**
 * Server Events Interface - Same elegance as Commands.execute<T>()
 */
export class Events {
  /**
   * Server-side event emission using EventBridge mechanism
   * Same elegance as Commands.execute<T>()
   *
   * @example await Events.emit<UserEntity>('data:User:created', userEntity, context, commander)
   */
  static async emit<T>(
    eventName: string,
    eventData: T,
    context: JTAGContext,
    commander: ICommandDaemon,
    options: EventEmitOptions = {}
  ): Promise<{ success: boolean; error?: string }> {
    try {
      if (!commander?.router) {
        throw new Error('Router not available for event emission');
      }

      // Create EventBridge payload - same mechanism as DataCreateServerCommand
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
        data: eventData as Record<string, unknown>, // Entity directly - same structure as data/list returns
        originSessionId: options.sessionId ?? context.uuid,
        originContextUUID: context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message using JTAG message factory
      const eventMessage = JTAGMessageFactory.createEvent(
        context,
        'events',
        JTAG_ENDPOINTS.EVENTS.BRIDGE,
        eventPayload
      );

      // Route event through Router (handles cross-context distribution)
      const result = await commander.router.postMessage(eventMessage);
      console.log(`📨 SERVER-EVENT: Emitted ${eventName} via EventBridge`, result);

      return { success: true };
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Events: Failed to emit ${eventName}:`, error);
      return { success: false, error: errorMsg };
    }
  }
}