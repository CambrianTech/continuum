/**
 * Chat Send Message Server Command
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory, createPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { EVENT_ENDPOINTS } from '../../../../daemons/events-daemon/shared/EventEndpoints';
import type { EventBridgePayload } from '../../../../daemons/events-daemon/shared/EventsDaemon';
import { ChatSendMessageCommand } from '../shared/ChatSendMessageCommand';
import { CHAT_EVENTS } from '../../../../widgets/chat/shared/ChatEventConstants';

export class ChatSendMessageServerCommand extends ChatSendMessageCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }

  // REMOVED BROKEN CODE - Going back to base implementation until proper types are defined

  /**
   * Server-specific event emission with proper Node imports
   */
  protected async emitMessageEvent(message: any): Promise<void> {
    console.log(`🔥 SERVER-EVENT: emitMessageEvent called for message ${message.messageId}`);
    try {
      // Cross-environment event emission (to browser listeners)
      console.log(`🔥 SERVER-EVENT: Starting cross-environment event creation...`);
      
      const eventBridgeData: EventBridgePayload = {
        type: 'event-bridge' as const,
        eventName: CHAT_EVENTS.MESSAGE_RECEIVED,  // Use constant and correct event name
        data: { message },
        scope: {
          type: 'room' as const,
          id: message.roomId,
          // Remove sessionId to broadcast to ALL room participants, not just sender
        },
        originSessionId: message.senderId,
        originContextUUID: this.context.uuid,  // Track the originating context for recursion prevention
        timestamp: message.timestamp,
        // Required JTAGPayload fields
        context: this.context,
        sessionId: message.senderId
      };
      
      console.log(`🔥 SERVER-EVENT: Event payload created:`, JSON.stringify(eventBridgeData, null, 2));
      
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'chat-send-message',
        `events/${EVENT_ENDPOINTS.BRIDGE}`,
        eventBridgeData
      );
      
      console.log(`🔥 SERVER-EVENT: Event message created, posting to router...`);
      const result = await this.commander.router.postMessage(eventMessage);
      console.log(`🔥 SERVER-EVENT: Router result:`, result);
      console.log(`📨 SERVER-EVENT: Sent ${CHAT_EVENTS.MESSAGE_RECEIVED} event for message ${message.messageId} to room ${message.roomId}`);
      
    } catch (error) {
      console.error(`❌ Failed to emit message event:`, error);
      // Don't fail the entire operation if event emission fails
    }
  }
}