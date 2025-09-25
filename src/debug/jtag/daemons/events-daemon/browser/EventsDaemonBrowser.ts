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
import { Events } from '../../../system/core/client/shared/Events';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';

// EventBridge metadata structure for better type safety
interface EventBridgeMetadata {
  __JTAG_BRIDGED__?: boolean;
  __JTAG_ORIGINAL_CONTEXT__?: string;
  __JTAG_BRIDGE_TIMESTAMP__?: string;
  __JTAG_BRIDGE_HOP_COUNT__?: number;
  message?: unknown;
}

export class EventsDaemonBrowser extends EventsDaemon {
  protected eventManager = new EventManager();
  private domEventBridge: DOMEventBridge;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    console.log(`🔥 CLAUDE-BROWSER-DAEMON-DEBUG-${Date.now()}: EventsDaemonBrowser constructor called!`);
    console.log(`🔥 Context: ${context.environment}/${context.uuid}`);
    console.log(`🔥 ENDPOINT-DEBUG: EventsDaemonBrowser.subpath = "${this.subpath}"`);
    console.log(`🔥 ENDPOINT-DEBUG: Expected browser endpoint should be "browser/${this.subpath}"`);

    // Setup DOM event bridge for widget communication
    this.domEventBridge = new DOMEventBridge(this.eventManager);
    console.log('🌉 EventsDaemonBrowser: DOM event bridge initialized');
  }

  /**
   * Handle local event bridging - emit to event system AND DOM for BaseWidget
   */
  protected handleLocalEventBridge(eventName: string, eventData: unknown): void {
    console.log(`🔥 CLAUDE-BROWSER-EVENT-${Date.now()}: handleLocalEventBridge called with eventName='${eventName}'`);

    // Emit to local event system - DOMEventBridge will automatically handle DOM dispatch
    this.eventManager.events.emit(eventName, eventData);

    // CRITICAL: Also dispatch DOM event for BaseWidget integration
    const domEvent = new CustomEvent(eventName, {
      detail: eventData
    });

    // Type-safe document access for browser environment
    if (typeof globalThis !== 'undefined' && 'document' in globalThis) {
      (globalThis as typeof globalThis & { document: Document }).document.dispatchEvent(domEvent);
    }
    console.log(`🔥 CLAUDE-DOM-EVENT-${Date.now()}: Dispatched DOM event '${eventName}' for BaseWidget`);

    // CRITICAL FIX: Also check wildcard subscriptions from Events.subscribe()
    // This ensures server events trigger browser widget handlers properly
    try {
      Events.checkWildcardSubscriptions(eventName, eventData);
      console.log(`🎯 CLAUDE-WILDCARD-${Date.now()}: Checked wildcard subscriptions for '${eventName}'`);
    } catch (error) {
      console.error(`❌ CLAUDE-WILDCARD-ERROR-${Date.now()}: Failed to check wildcard subscriptions:`, error);
    }
  }

  /**
   * Emit a generic entity event to trigger DOM events for widgets
   * Architecture-compliant: Works with any BaseEntity, not specific types
   */
  public emitEntityEvent<T extends BaseEntity>(eventName: string, entity: T): void {
    this.eventManager.events.emit(eventName, { entity });
    console.log(`🔄 EventsDaemonBrowser: Emitted ${eventName} event for ${entity.collection}/${entity.id}`);
  }
}