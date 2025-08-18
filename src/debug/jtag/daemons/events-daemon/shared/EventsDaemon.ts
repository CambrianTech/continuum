/**
 * Events Daemon - Cross-Context Event Bridge Handler
 * 
 * Handles 'event-bridge' messages sent by ScopedEventSystem's EventBridge
 * to propagate events between browser and server contexts.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGMessage, JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { createBaseResponse, type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { EVENT_ENDPOINTS } from './EventEndpoints';
import { 
  EventRoutingUtils, 
  EVENT_METADATA_KEYS, 
  EVENT_SCOPES,
  type EventScope,
  type EventEndpoint 
} from '../../../system/events/shared/EventSystemConstants';

/**
 * Event bridge message payload with proper typing
 */
export interface EventBridgePayload extends JTAGPayload {
  type: 'event-bridge';
  scope: {
    type: EventScope;
    id?: string;
    sessionId?: string;
  };
  eventName: string;
  data: {
    message?: unknown;
    [EVENT_METADATA_KEYS.BRIDGED]?: boolean;
    [EVENT_METADATA_KEYS.ORIGINAL_CONTEXT]?: string;
    [EVENT_METADATA_KEYS.BRIDGE_TIMESTAMP]?: string;
    [EVENT_METADATA_KEYS.BRIDGE_HOP_COUNT]?: number;
    [key: string]: unknown;
  };
  originSessionId: UUID;
  originContextUUID?: UUID; // Track originating context for recursion prevention
  timestamp: string;
}

/**
 * Event bridge response types
 */
export interface EventBridgeResponse extends BaseResponsePayload {
  bridged?: boolean;
  eventName?: string;
  scope?: string;
}

/**
 * Events Daemon - Handles cross-context event bridging
 */
export abstract class EventsDaemon extends DaemonBase {
  public readonly subpath: string = 'events';
  protected abstract eventManager: EventManager;

  constructor(
    context: JTAGContext,
    router: JTAGRouter
  ) {
    super('EventsDaemon', context, router);
  }

  /**
   * Initialize events daemon
   */
  protected async initialize(): Promise<void> {
    console.log(`🌉 EventsDaemon: Initialized for cross-context event bridging`);
  }

  /**
   * Handle event bridge messages from other contexts
   */
  async handleMessage(message: JTAGMessage): Promise<EventBridgeResponse> {
    // Normalize endpoint using shared utility
    const endpoint = EventRoutingUtils.normalizeEndpoint(message.endpoint);
    
    if (endpoint === EVENT_ENDPOINTS.BRIDGE) {
      return await this.handleEventBridge(message);
    }
    
    if (endpoint === EVENT_ENDPOINTS.STATS) {
      return await this.getBridgeStats();
    }
    
    const errorMsg = `Unknown endpoint: ${message.endpoint}`;
    console.error(`❌ EventsDaemon: ${errorMsg}`);
    return createBaseResponse(false, message.context, message.payload.sessionId, {}) as EventBridgeResponse;
  }

  /**
   * Handle incoming event bridge message
   */
  private async handleEventBridge(message: JTAGMessage): Promise<EventBridgeResponse> {
    const payload = message.payload as EventBridgePayload;
    
    try {
      console.log(`🌉 EventsDaemon: Received bridged event from ${payload.originSessionId}`);
      console.log(`   Event: ${payload.eventName}`);
      console.log(`   Scope: ${payload.scope.type}${payload.scope.id ? `:${payload.scope.id}` : ''}`);
      console.log(`   Current context: ${this.context.environment}/${this.context.uuid}`);
      console.log(`   Origin context: ${payload.originContextUUID}`);
      
      // Check if we're the origin context - if so, skip local emission but still route cross-environment
      const isOriginContext = payload.originContextUUID && payload.originContextUUID === this.context.uuid;
      
      if (isOriginContext) {
        console.log(`🔄 EventsDaemon: Origin context - skipping local emission, routing to other environments`);
      } else {
        // Re-emit the event in this context with bridge metadata using shared constants
        const bridgedData = EventRoutingUtils.addBridgeMetadata(
          payload.data,
          payload.originSessionId,
          payload.timestamp
        );
        
        // Emit to local event system
        this.eventManager.events.emit(payload.eventName, bridgedData);
        console.log(`✨ EventsDaemon: Bridged event '${payload.eventName}' to local context`);
      }
      
      // Always route to other environments if this is NOT already a bridged event
      // Check if event is already bridged to prevent infinite recursion using shared utility
      if (!EventRoutingUtils.isEventBridged(payload.data)) {
        await this.routeToOtherEnvironments(payload);
      } else {
        console.log(`🔄 EventsDaemon: Skipping re-bridge of already bridged event '${payload.eventName}'`);
      }
      
      console.log(`✨ EventsDaemon: Processed event '${payload.eventName}' ${isOriginContext ? '(origin context)' : '(bridged locally)'}`);
      
      return createBaseResponse(true, message.context, payload.sessionId, {
        bridged: true,
        eventName: payload.eventName,
        scope: `${payload.scope.type}${payload.scope.id ? `:${payload.scope.id}` : ''}`
      }) as EventBridgeResponse;
      
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ EventsDaemon: Event bridge failed: ${errorMsg}`);
      return createBaseResponse(false, message.context, payload.sessionId, {}) as EventBridgeResponse;
    }
  }

  /**
   * Route event to other environments (cross-environment bridging)
   */
  private async routeToOtherEnvironments(payload: EventBridgePayload): Promise<void> {
    try {
      // Determine target environments (opposite of current)
      const targetEnvironments = this.context.environment === 'server' ? ['browser'] : ['server'];
      
      for (const targetEnv of targetEnvironments) {
        // Create cross-environment message using shared utility
        const crossEnvEndpoint = EventRoutingUtils.createCrossEnvEndpoint(
          targetEnv as any, 
          EVENT_ENDPOINTS.BRIDGE
        );
        
        const crossEnvMessage = JTAGMessageFactory.createEvent(
          this.context,
          'events-daemon',
          crossEnvEndpoint,
          payload
        );
        
        // Route to other environment via router's transport
        try {
          console.log(`🌉 EventsDaemon: Attempting to route event '${payload.eventName}' to ${targetEnv} with endpoint: ${crossEnvMessage.endpoint}`);
          const result = await this.router.postMessage(crossEnvMessage);
          console.log(`🌉 EventsDaemon: Router result for ${targetEnv}:`, result);
          console.log(`🌉 EventsDaemon: Routed event '${payload.eventName}' to ${targetEnv} environment`);
        } catch (routingError) {
          console.warn(`⚠️ EventsDaemon: Failed to route event to ${targetEnv}:`, routingError);
        }
      }
    } catch (error) {
      console.error(`❌ EventsDaemon: Cross-environment routing failed:`, error);
    }
  }

  /**
   * Get event bridge statistics
   */
  async getBridgeStats(): Promise<EventBridgeResponse> {
    return createBaseResponse(true, this.context, this.context.uuid as UUID, {
      bridged: false // Stats request, not a bridge operation
    }) as EventBridgeResponse;
  }
}