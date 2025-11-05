/**
 * Widget Event Service - Browser Implementation
 * 
 * Browser-specific override for DOM event coordination.
 * Extends shared base class with DOM-specific functionality.
 */

import { WidgetEventService } from '../../shared/services/events/WidgetEventService';

export class WidgetEventServiceBrowser extends WidgetEventService {
  // Inherits serviceName from base class
  
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
      
      // Add DOM listener on first subscription
      document.addEventListener(eventName, (event) => {
        this.handleCustomEvent(eventName, event as CustomEvent);
      });
    }
    
    this.customEventHandlers.get(eventName)!.push(handler);
    console.debug(`🎯 WidgetEventServiceBrowser: Subscribed to DOM custom event '${eventName}'`);
  }

  // Browser implementation inherits all protected methods and properties
}