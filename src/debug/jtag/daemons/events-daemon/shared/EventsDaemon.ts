/**
 * Events Daemon - Cross-Context Event Bridge Handler
 * 
 * Handles 'event-bridge' messages sent by ScopedEventSystem's EventBridge
 * to propagate events between browser and server contexts.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGMessage, JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { createBaseResponse, type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

/**
 * Event bridge message payload with proper typing
 */
export interface EventBridgePayload extends JTAGPayload {
  type: 'event-bridge';
  scope: {
    type: 'system' | 'room' | 'user' | 'global';
    id?: string;
    sessionId?: string;
  };
  eventName: string;
  data: any; // Must be any for spread operations
  originSessionId: UUID;
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
    if (message.endpoint === 'event-bridge') {
      return await this.handleEventBridge(message);
    }
    
    const errorMsg = `Unknown endpoint: ${message.endpoint}`;
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
      
      // Re-emit the event in this context with bridge metadata
      const bridgedData = {
        ...payload.data,
        _bridged: true,
        _originalContext: payload.originSessionId,
        _bridgeTimestamp: payload.timestamp
      };
      
      // Emit to local event system
      this.eventManager.events.emit(payload.eventName, bridgedData);
      
      console.log(`✨ EventsDaemon: Bridged event '${payload.eventName}' to local context`);
      
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
   * Get event bridge statistics
   */
  async getBridgeStats(): Promise<EventBridgeResponse> {
    return createBaseResponse(true, this.context, this.context.uuid as UUID, {
      bridged: false // Stats request, not a bridge operation
    }) as EventBridgeResponse;
  }
}