/**
 * Events Daemon - Server Implementation
 *
 * Handles cross-context event bridging in server environment.
 * Critical for AI agents running on server to receive same events as human users in browser.
 */

import { EventsDaemon, type EventBridgePayload, type EventBridgeResponse } from '../shared/EventsDaemon';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { EventSubscriptionManager } from '../../../system/events/shared/EventSubscriptionManager';
import type { IEventSubscriptionProvider } from '../../../system/events/shared/IEventSubscriptionProvider';

export class EventsDaemonServer extends EventsDaemon implements IEventSubscriptionProvider {
  protected eventManager = new EventManager();
  private subscriptionManager = new EventSubscriptionManager();

  // ✅ FIX: Deduplicate event processing (same event emitted multiple times via different paths)
  private recentEvents = new Map<string, number>(); // eventKey → timestamp
  private readonly dedupeWindowMs = 100; // Consider events within 100ms as duplicates

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Get subscription manager for unified event subscriptions
   * Exposed to JTAGClient.daemons.events interface
   */
  public getSubscriptionManager(): EventSubscriptionManager {
    return this.subscriptionManager;
  }

  /**
   * Handle local event bridging - emit to event system and trigger subscriptions
   * ALWAYS called for server events, even if origin context matches (PersonaUsers need this!)
   */
  protected handleLocalEventBridge(eventName: string, eventData: unknown): void {
    // ✅ FIX: Deduplicate - same event can arrive via multiple paths (WebSocket broadcast, local emit, etc.)
    const entityId = (eventData as { id?: string })?.id || 'unknown';
    const eventKey = `${eventName}:${entityId}`;
    const now = Date.now();

    // Check if we recently processed this exact event
    const lastProcessed = this.recentEvents.get(eventKey);
    if (lastProcessed && (now - lastProcessed) < this.dedupeWindowMs) {
      return; // Skip duplicate
    }

    // Record this event
    this.recentEvents.set(eventKey, now);

    // Cleanup old entries (every 100 events to avoid memory leak)
    if (this.recentEvents.size > 100) {
      const cutoff = now - this.dedupeWindowMs;
      for (const [key, timestamp] of this.recentEvents.entries()) {
        if (timestamp < cutoff) {
          this.recentEvents.delete(key);
        }
      }
    }

    // 1. Emit to local event system (legacy EventManager)
    this.eventManager.events.emit(eventName, eventData);

    // 2. Trigger unified subscription manager (NEW!)
    // This is critical for AI agents to receive events
    this.subscriptionManager.trigger(eventName, eventData);
  }

  /**
   * Override handleMessage to ensure server-side events are ALWAYS emitted locally
   * This fixes PersonaUser event subscriptions (they run in same context as DataDaemon)
   */
  async handleMessage(message: JTAGMessage): Promise<EventBridgeResponse> {
    const payload = message.payload as EventBridgePayload;

    const result = await super.handleMessage(message);

    // CRITICAL FIX: For event-bridge messages, ALWAYS emit locally on server
    // PersonaUsers run in same server context as DataDaemon, so without this,
    // they never receive events (isOriginContext check in parent skips emission)
    // Check payload type only (endpoint is now 'events', not 'event-bridge')
    if (payload.type === 'event-bridge') {
      this.handleLocalEventBridge(payload.eventName, payload.data);
    }

    return result;
  }
}