/**
 * Events Daemon - Server Implementation
 *
 * Handles cross-context event bridging in server environment.
 * Critical for AI agents running on server to receive same events as human users in browser.
 */

import { EventsDaemon } from '../shared/EventsDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { EventSubscriptionManager } from '../../../system/events/shared/EventSubscriptionManager';
import type { IEventSubscriptionProvider } from '../../../system/events/shared/IEventSubscriptionProvider';

export class EventsDaemonServer extends EventsDaemon implements IEventSubscriptionProvider {
  protected eventManager = new EventManager();
  private subscriptionManager = new EventSubscriptionManager();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    console.log('🎧 EventsDaemonServer: Unified subscription manager initialized');
    console.log('🤖 EventsDaemonServer: AI agents can now subscribe to events like human users');
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
   */
  protected handleLocalEventBridge(eventName: string, eventData: any): void {
    // 1. Emit to local event system (legacy EventManager)
    this.eventManager.events.emit(eventName, eventData);

    // 2. Trigger unified subscription manager (NEW!)
    // This is critical for AI agents to receive events
    this.subscriptionManager.trigger(eventName, eventData);

    console.log(`📢 EventsDaemonServer: Event '${eventName}' triggered for server-side subscribers (AI agents)`);
  }
}