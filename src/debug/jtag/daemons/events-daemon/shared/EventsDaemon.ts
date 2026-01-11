/**
 * Events Daemon - Cross-Context Event Bridge Handler
 *
 * Handles 'event-bridge' messages sent by ScopedEventSystem's EventBridge
 * to propagate events between browser and server contexts.
 *
 * CRITICAL: Includes rate limiting to prevent cascade failures.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';

/**
 * Rate limiter to prevent cascade failures from event floods.
 * Tracks event counts per type and blocks when threshold exceeded.
 * Enhanced with diagnostics to identify trending events.
 */
class EventRateLimiter {
  private counts = new Map<string, number>();
  private windowStart = Date.now();
  private readonly windowMs = 100;      // 100ms window
  private readonly maxPerWindow = 20;   // Max 20 of same event per window
  private readonly warnThreshold = 10;  // Warn at 10+ per window
  private blocked = new Set<string>();
  private warned = new Set<string>();   // Track warned events to avoid spam

  // Global stats for diagnostics
  private totalBlocked = 0;
  private totalWarned = 0;
  private blockedHistory: Array<{ event: string; count: number; time: number }> = [];

  shouldBlock(eventName: string): boolean {
    const now = Date.now();

    // Reset window if expired
    if (now - this.windowStart > this.windowMs) {
      // Log summary of previous window if there was activity
      if (this.counts.size > 0) {
        const hotEvents = Array.from(this.counts.entries())
          .filter(([_, count]) => count >= this.warnThreshold)
          .sort((a, b) => b[1] - a[1]);

        if (hotEvents.length > 0) {
          console.warn(`⚠️ EVENT ACTIVITY: ${hotEvents.map(([e, c]) => `${e}(${c})`).join(', ')}`);
        }
      }

      this.counts.clear();
      this.blocked.clear();
      this.warned.clear();
      this.windowStart = now;
    }

    // Check if already blocked this window
    if (this.blocked.has(eventName)) {
      return true;
    }

    // Increment count
    const count = (this.counts.get(eventName) ?? 0) + 1;
    this.counts.set(eventName, count);

    // Warn at threshold (once per window per event)
    if (count === this.warnThreshold && !this.warned.has(eventName)) {
      this.warned.add(eventName);
      this.totalWarned++;
      console.warn(`⚠️ EVENT TRENDING: "${eventName}" at ${count}x in ${this.windowMs}ms (blocking at ${this.maxPerWindow})`);
    }

    // Block if over threshold
    if (count > this.maxPerWindow) {
      this.blocked.add(eventName);
      this.totalBlocked++;
      this.blockedHistory.push({ event: eventName, count, time: now });
      // Keep only last 100 blocked events
      if (this.blockedHistory.length > 100) {
        this.blockedHistory.shift();
      }
      console.error(`🛑 EVENT CASCADE BLOCKED: "${eventName}" fired ${count}x in ${this.windowMs}ms`);
      return true;
    }

    return false;
  }

  getStats(): { totalBlocked: number; totalWarned: number; recentBlocked: Array<{ event: string; count: number; time: number }> } {
    return {
      totalBlocked: this.totalBlocked,
      totalWarned: this.totalWarned,
      recentBlocked: this.blockedHistory.slice(-10)
    };
  }
}
import type { JTAGMessage, JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { createBaseResponse, type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { JTAG_ENDPOINTS, JTAG_DAEMON_ENDPOINTS } from '../../../system/core/router/shared/JTAGEndpoints';
import {
  EventRoutingUtils,
  EVENT_METADATA_KEYS,
  type EventScope
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
  public readonly subpath: string = JTAG_ENDPOINTS.EVENTS.BASE;
  protected abstract eventManager: EventManager;

  // Rate limiter to prevent cascade failures
  private rateLimiter = new EventRateLimiter();

  /**
   * Handle event bridging to local context - implemented by environment-specific subclasses
   */
  protected abstract handleLocalEventBridge(eventName: string, eventData: unknown): void;

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
    // Check payload type - events route to 'events' endpoint but have type 'event-bridge'
    const payload = message.payload as EventBridgePayload;

    if (payload.type === 'event-bridge') {
      return await this.handleEventBridge(message);
    }

    // Check for stats requests
    const endpoint = EventRoutingUtils.normalizeEndpoint(message.endpoint);
    if (endpoint === JTAG_ENDPOINTS.EVENTS.STATS) {
      return await this.getBridgeStats();
    }

    const errorMsg = `Unknown payload type: ${payload.type}, endpoint: ${message.endpoint}`;
    console.error(`❌ EventsDaemon: ${errorMsg}`);
    return createBaseResponse(false, message.context, message.payload.sessionId, {}) as EventBridgeResponse;
  }

  /**
   * Handle incoming event bridge message
   */
  private async handleEventBridge(message: JTAGMessage): Promise<EventBridgeResponse> {
    const payload = message.payload as EventBridgePayload;

    // CRITICAL: Rate limit to prevent cascade failures
    if (this.rateLimiter.shouldBlock(payload.eventName)) {
      return createBaseResponse(true, message.context, payload.sessionId, {
        bridged: false,
        eventName: payload.eventName,
        scope: 'blocked-cascade'
      }) as EventBridgeResponse;
    }

    try {
      // Check if we're the origin context - if so, skip local emission but still route cross-environment
      const isOriginContext = payload.originContextUUID && payload.originContextUUID === this.context.uuid;

      if (!isOriginContext) {
        // Re-emit the event in this context with bridge metadata using shared constants
        const bridgedData = EventRoutingUtils.addBridgeMetadata(
          payload.data,
          payload.originSessionId,
          payload.timestamp
        );

        // Delegate to environment-specific event handling
        this.handleLocalEventBridge(payload.eventName, bridgedData);

        // Note: DOM event dispatching is handled by environment-specific implementations

      }
      
      // Always route to other environments if this is NOT already a bridged event
      // Check if event is already bridged to prevent infinite recursion using shared utility
      const isAlreadyBridged = EventRoutingUtils.isEventBridged(payload.data);
      if (!isAlreadyBridged) {
        await this.routeToOtherEnvironments(payload);
      }

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
        // Create cross-environment message - send to the actual registered daemon endpoint
        const crossEnvEndpoint = `${targetEnv}/${this.subpath}`;

        const crossEnvMessage = JTAGMessageFactory.createEvent(
          this.context,
          'events-daemon',
          crossEnvEndpoint,
          payload
        );
        
        // Route to other environment via router's transport
        try {
          await this.router.postMessage(crossEnvMessage);
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