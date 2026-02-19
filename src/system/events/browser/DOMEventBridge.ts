/**
 * DOM Event Bridge - Convert JTAG events to DOM events
 * 
 * Bridges internal JTAG event system to DOM CustomEvents for widget consumption
 */

import { EventManager } from '../shared/JTAGEventSystem';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export class DOMEventBridge {
  private eventManager: EventManager;
  private eventMappings: Map<string, string> = new Map();

  constructor(eventManager: EventManager) {
    this.eventManager = eventManager;
    this.setupDefaultMappings();
    this.setupEventListeners();
  }

  /**
   * Setup default JTAG → DOM event mappings
   */
  private setupDefaultMappings(): void {
    // Chat events
    this.eventMappings.set('chat-message-sent', 'chat:message-received');
    this.eventMappings.set('chat-room-updated', 'chat:room-updated');
    this.eventMappings.set('chat-participant-joined', 'chat:participant-joined');
    this.eventMappings.set('chat-participant-left', 'chat:participant-left');

    verbose() && console.log('🌉 DOMEventBridge: Registered event mappings');
  }

  /**
   * Setup listeners for JTAG events
   */
  private setupEventListeners(): void {
    // Listen for all mapped events and bridge them to DOM
    this.eventMappings.forEach((domEventName, jtagEventName) => {
      this.eventManager.events.on(jtagEventName, (data: any) => {
        this.bridgeToDOMEvent(domEventName, data);
      });

      verbose() && console.log(`🔗 DOMEventBridge: Listening for '${jtagEventName}' → '${domEventName}'`);
    });
  }

  /**
   * Bridge JTAG event data to DOM CustomEvent
   */
  private bridgeToDOMEvent(domEventName: string, data: any): void {
    try {
      const customEvent = new CustomEvent(domEventName, {
        detail: data,
        bubbles: true,
        cancelable: false
      });

      document.dispatchEvent(customEvent);
      verbose() && console.log(`✨ DOMEventBridge: Emitted DOM event '${domEventName}'`);
      
    } catch (error) {
      console.error(`❌ DOMEventBridge: Failed to emit DOM event '${domEventName}':`, error);
    }
  }

  /**
   * Add custom event mapping
   */
  public addMapping(jtagEventName: string, domEventName: string): void {
    this.eventMappings.set(jtagEventName, domEventName);
    
    // Add listener for new mapping
    this.eventManager.events.on(jtagEventName, (data: any) => {
      this.bridgeToDOMEvent(domEventName, data);
    });

    verbose() && console.log(`🔗 DOMEventBridge: Added mapping '${jtagEventName}' → '${domEventName}'`);
  }

  /**
   * Manually emit DOM event (for testing)
   */
  public emitDOMEvent(domEventName: string, data: any): void {
    this.bridgeToDOMEvent(domEventName, data);
  }
}