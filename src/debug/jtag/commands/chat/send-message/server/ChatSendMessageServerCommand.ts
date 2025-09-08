/**
 * Chat Send Message Server Command
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { JTAGMessageFactory, createPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { EVENT_ENDPOINTS } from '../../../../daemons/events-daemon/shared/EventEndpoints';
import type { EventBridgePayload } from '../../../../daemons/events-daemon/shared/EventsDaemon';
import { ChatSendMessageCommand } from '../shared/ChatSendMessageCommand';

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
    // console.debug(`🔍 DEBUG: emitMessageEvent called for message ${message.messageId}`);
    try {
      // Skip local event emission for now - focus on cross-environment
      // console.debug(`🔄 SKIPPING local event emission - focusing on cross-environment`);
      
      // Then send to cross-environment bridge (for browser listeners)
      // console.debug(`🔍 DEBUG: Starting cross-environment event creation...`);
      
      const eventBridgeData: EventBridgePayload = {
        type: 'event-bridge' as const,
        eventName: 'chat-message-sent',
        data: { message },
        scope: {
          type: 'room' as const,
          id: message.roomId,
          sessionId: message.senderId
        },
        originSessionId: message.senderId,
        originContextUUID: this.context.uuid,  // Track the originating context for recursion prevention
        timestamp: message.timestamp,
        // Required JTAGPayload fields
        context: this.context,
        sessionId: message.senderId
      };
      
      // console.debug(`🔍 DEBUG: Event payload created:`, JSON.stringify(eventBridgeData, null, 2));
      
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'chat-send-message',
        `events/${EVENT_ENDPOINTS.BRIDGE}`,
        eventBridgeData
      );
      
      // console.debug(`🔍 DEBUG: Event message created, posting to router...`);
      const result = await this.commander.router.postMessage(eventMessage);
      // console.debug(`🔍 DEBUG: Router result:`, result);
      // console.debug(`📨 Sent cross-environment chat-message-sent event for message ${message.messageId}`);
      
    } catch (error) {
      console.error(`❌ Failed to emit message event:`, error);
      // Don't fail the entire operation if event emission fails
    }
  }
}