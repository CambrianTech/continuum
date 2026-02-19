/**
 * Events Daemon - Browser Implementation
 * 
 * Handles cross-context event bridging in browser environment
 * and bridges JTAG events to DOM events for widget consumption
 */

import { EventsDaemon } from '../shared/EventsDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { EventManager } from '../../../system/events/shared/JTAGEventSystem';
import { DOMEventBridge } from '../../../system/events/browser/DOMEventBridge';
import { Events } from '../../../system/core/shared/Events';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import { EventSubscriptionManager } from '../../../system/events/shared/EventSubscriptionManager';
import type { IEventSubscriptionProvider } from '../../../system/events/shared/IEventSubscriptionProvider';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

// EventBridge metadata structure for better type safety
interface EventBridgeMetadata {
  __JTAG_BRIDGED__?: boolean;
  __JTAG_ORIGINAL_CONTEXT__?: string;
  __JTAG_BRIDGE_TIMESTAMP__?: string;
  __JTAG_BRIDGE_HOP_COUNT__?: number;
  message?: unknown;
}

export class EventsDaemonBrowser extends EventsDaemon implements IEventSubscriptionProvider {
  protected eventManager = new EventManager();
  private domEventBridge: DOMEventBridge;
  private subscriptionManager = new EventSubscriptionManager();

  /**
   * Registry of event names with active DOM listeners.
   * DOM CustomEvent dispatch is skipped for events not in this set.
   * Widgets register via registerDOMInterest() when they need document-level events.
   */
  private static _domInterest = new Set<string>();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Setup DOM event bridge for widget communication
    this.domEventBridge = new DOMEventBridge(this.eventManager);
    verbose() && console.log('🌉 EventsDaemonBrowser: DOM event bridge initialized');
    verbose() && console.log('🎧 EventsDaemonBrowser: Unified subscription manager initialized');
  }

  /**
   * Get subscription manager for unified event subscriptions
   * Exposed to JTAGClient.daemons.events interface
   */
  public getSubscriptionManager(): EventSubscriptionManager {
    return this.subscriptionManager;
  }

  /**
   * Register interest in receiving DOM CustomEvents for a specific event name.
   * Only events with registered interest will be dispatched to the document.
   * Returns an unregister function.
   */
  public static registerDOMInterest(eventName: string): () => void {
    EventsDaemonBrowser._domInterest.add(eventName);
    return () => {
      EventsDaemonBrowser._domInterest.delete(eventName);
    };
  }

  /**
   * Check if anything has registered DOM interest for this event name.
   * Checks both:
   * - Events.domInterest (populated by Events.subscribe() in browser)
   * - _domInterest (populated by registerDOMInterest() from BaseWidget/WidgetEventServiceBrowser)
   * Uses prefix matching: 'data:chat_messages' matches 'data:chat_messages:created'.
   */
  private hasDOMInterest(eventName: string): boolean {
    // Direct match in either registry
    if (Events.domInterest.has(eventName)) return true;
    if (EventsDaemonBrowser._domInterest.has(eventName)) return true;

    // Prefix match against both registries
    for (const interest of Events.domInterest) {
      if (eventName.startsWith(interest + ':') || interest.startsWith(eventName + ':')) return true;
    }
    for (const interest of EventsDaemonBrowser._domInterest) {
      if (eventName.startsWith(interest + ':') || interest.startsWith(eventName + ':')) return true;
    }
    return false;
  }

  /**
   * Handle local event bridging - emit to event system AND DOM for BaseWidget
   *
   * Dispatch order:
   * 1. Internal EventEmitter (DOMEventBridge handles mapped events)
   * 2. SubscriptionManager (exact, wildcard, elegant pattern matching)
   * 3. Legacy wildcard subscriptions
   * 4. DOM CustomEvent ONLY if a widget registered interest (filter-first, not spam-then-filter)
   */
  protected handleLocalEventBridge(eventName: string, eventData: unknown): void {
    // 1. Emit to local event system — DOMEventBridge handles its mapped events
    this.eventManager.events.emit(eventName, eventData);

    // 2. Trigger unified subscription manager (exact, wildcard, and elegant patterns)
    this.subscriptionManager.trigger(eventName, eventData);

    // 3. Legacy: Also check wildcard subscriptions from Events.subscribe()
    try {
      Events.checkWildcardSubscriptions(eventName, eventData);
    } catch (error) {
      console.error('Failed to check wildcard subscriptions:', error);
    }

    // 4. DOM dispatch — ONLY if a widget registered interest for this event namespace
    // This prevents creating DOM CustomEvents for high-frequency events no widget cares about
    if (this.hasDOMInterest(eventName)) {
      if (typeof globalThis !== 'undefined' && 'document' in globalThis) {
        const domEvent = new CustomEvent(eventName, { detail: eventData });
        (globalThis as typeof globalThis & { document: Document }).document.dispatchEvent(domEvent);
      }
    }
  }

  /**
   * Emit a generic entity event to trigger DOM events for widgets
   * Architecture-compliant: Works with any BaseEntity, not specific types
   */
  public emitEntityEvent<T extends BaseEntity>(eventName: string, entity: T): void {
    this.eventManager.events.emit(eventName, { entity });
    verbose() && console.log(`🔄 EventsDaemonBrowser: Emitted ${eventName} event for ${entity.collection}/${entity.id}`);
  }
}