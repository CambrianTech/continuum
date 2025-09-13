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
import type { ChatMessage } from '../../chat-daemon/shared/ChatTypes';

export class EventsDaemonBrowser extends EventsDaemon {
  protected eventManager = new EventManager();
  private domEventBridge: DOMEventBridge;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    
    // Setup DOM event bridge for widget communication
    this.domEventBridge = new DOMEventBridge(this.eventManager);
    console.log('🌉 EventsDaemonBrowser: DOM event bridge initialized');
  }

  /**
   * Handle local event bridging - emit to event system, DOMEventBridge handles DOM dispatch
   */
  protected handleLocalEventBridge(eventName: string, eventData: any): void {
    // Emit to local event system - DOMEventBridge will automatically handle DOM dispatch
    this.eventManager.events.emit(eventName, eventData);
  }

  /**
   * Emit a chat message event to trigger DOM events for widgets
   */
  public emitChatMessageEvent(message: ChatMessage): void {
    this.eventManager.events.emit('chat-message-sent', { message });
    console.log(`💬 EventsDaemonBrowser: Emitted chat-message-sent event for message ${message.messageId}`);
  }
}