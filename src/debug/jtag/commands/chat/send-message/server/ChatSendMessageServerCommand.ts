/**
 * Chat Send Message Server Command
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { JTAG_ENDPOINTS } from '../../../../system/core/router/shared/JTAGEndpoints';
import type { EventBridgePayload } from '../../../../daemons/events-daemon/shared/EventsDaemon';
import { ChatSendMessageCommand } from '../shared/ChatSendMessageCommand';
import type { ChatSendMessageParams, ChatSendMessageResult } from '../shared/ChatSendMessageTypes';
import { CHAT_EVENTS } from '../../../../widgets/chat/shared/ChatEventConstants';
import { ChatMessageData } from '../../../../system/data/domains/ChatMessage';
import { EVENT_SCOPES } from '../../../../system/events/shared/EventSystemConstants';

export class ChatSendMessageServerCommand extends ChatSendMessageCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }

  /**
   * Execute chat message sending using base class logic
   */
  async execute(params: ChatSendMessageParams): Promise<ChatSendMessageResult> {
    console.log(`🔥 CLAUDE-SERVER-EXECUTE-${Date.now()}: SERVER execute() called for message: "${params.content}"`);
    // Call base class which handles database storage + event emission
    const result = await super.execute(params);
    console.log(`🔥 CLAUDE-SERVER-COMPLETE-${Date.now()}: SERVER execute() completed for message: "${params.content}"`);
    return result;
  }

  /**
   * Server-specific event emission using Router's event facilities
   */
  protected async emitMessageEvent(message: ChatMessageData): Promise<void> {
    console.log(`🚨 CLAUDE-EMIT-CALLED-${Date.now()}: ChatSendMessageServerCommand.emitMessageEvent() called for ${message.messageId}`);
    console.log(`🔥 CLAUDE-FIX-${Date.now()}: SERVER-EVENT: emitMessageEvent called for message ${message.messageId}`);

    try {
      if (!this.commander?.router) {
        throw new Error('Router not available for event emission');
      }

      // Create EventBridge payload for room-scoped chat message event
      const eventPayload: EventBridgePayload = {
        context: this.context,
        sessionId: message.senderId,
        type: 'event-bridge',
        scope: {
          type: EVENT_SCOPES.ROOM,
          id: message.roomId,
          sessionId: message.senderId
        },
        eventName: CHAT_EVENTS.MESSAGE_RECEIVED,
        data: {
          eventType: 'chat:message-received',
          roomId: message.roomId,
          messageId: message.messageId,
          message: message,  // Send full ChatMessage domain object
          timestamp: new Date().toISOString()
        },
        originSessionId: message.senderId as string,
        originContextUUID: this.context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message using JTAG message factory
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'chat-send-message',
        JTAG_ENDPOINTS.EVENTS.BRIDGE,
        eventPayload
      );

      // Route event through Router (handles cross-context distribution)
      const result = await this.commander.router.postMessage(eventMessage);
      console.log(`📨 SERVER-EVENT: Emitted MESSAGE_RECEIVED for message ${message.messageId} in room ${message.roomId}`, result);

    } catch (error) {
      console.error(`❌ CLAUDE-EVENT-EMISSION-FAILED-${Date.now()}: Failed to emit message event:`, error);
      // Don't fail the entire operation if event emission fails
    }
  }
}