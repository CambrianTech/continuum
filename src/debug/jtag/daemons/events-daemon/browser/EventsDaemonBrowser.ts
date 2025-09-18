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
import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';

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
    document.dispatchEvent(domEvent);
    console.log(`🔥 CLAUDE-DOM-EVENT-${Date.now()}: Dispatched DOM event '${eventName}' for BaseWidget`);
  }

  /**
   * Emit a chat message event to trigger DOM events for widgets
   */
  public emitChatMessageEvent(message: ChatMessageData): void {
    this.eventManager.events.emit('chat-message-sent', { message });
    console.log(`💬 EventsDaemonBrowser: Emitted chat-message-sent event for message ${message.messageId}`);
  }
}