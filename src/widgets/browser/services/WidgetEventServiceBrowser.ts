/**
 * Widget Event Service - Browser Implementation
 * 
 * Browser-specific override for DOM event coordination.
 * Extends shared base class with DOM-specific functionality.
 */

import { WidgetEventService } from '../../shared/services/events/WidgetEventService';
import { EventsDaemonBrowser } from '../../../daemons/events-daemon/browser/EventsDaemonBrowser';

export class WidgetEventServiceBrowser extends WidgetEventService {
  // Track DOM listeners for proper cleanup (document.addEventListener persists across renders)
  private _domListeners = new Map<string, EventListener>();
  // Track DOM interest unregister functions for EventsDaemonBrowser scope filtering
  private _domInterestCleanups: Array<() => void> = [];

  // Override DOM event coordination for browser environment
  emitCustomEvent(eventName: string, detail: any): void {
    const customEvent = new CustomEvent(eventName, {
      detail: {
        ...detail,
        source: this.context?.widgetId || 'unknown',
        timestamp: new Date().toISOString()
      },
      bubbles: true,
      cancelable: true
    });

    // Emit on document for global listening
    document.dispatchEvent(customEvent);

    // Also handle locally if we have handlers
    this.handleCustomEvent(eventName, customEvent);

    console.debug(`🎯 WidgetEventServiceBrowser: Emitted DOM custom event '${eventName}'`);
  }

  subscribeToCustomEvents(eventName: string, handler: (event: CustomEvent) => void): void {
    if (!this.customEventHandlers.has(eventName)) {
      this.customEventHandlers.set(eventName, []);

      // Store reference for cleanup — document listeners persist, so we must track them
      const listener = (event: Event) => {
        this.handleCustomEvent(eventName, event as CustomEvent);
      };
      this._domListeners.set(eventName, listener);
      document.addEventListener(eventName, listener);

      // Register DOM interest so EventsDaemonBrowser knows to dispatch this event to DOM
      this._domInterestCleanups.push(EventsDaemonBrowser.registerDOMInterest(eventName));
    }

    this.customEventHandlers.get(eventName)!.push(handler);
    console.debug(`🎯 WidgetEventServiceBrowser: Subscribed to DOM custom event '${eventName}'`);
  }

  async cleanup(): Promise<void> {
    // Remove all document-level DOM listeners (prevents memory leak across widget lifecycle)
    for (const [eventName, listener] of this._domListeners) {
      document.removeEventListener(eventName, listener);
    }
    this._domListeners.clear();

    // Unregister DOM interest from EventsDaemonBrowser scope filtering
    for (const unregister of this._domInterestCleanups) {
      unregister();
    }
    this._domInterestCleanups = [];

    // Call base class cleanup (clears handler maps)
    await super.cleanup();
  }
}