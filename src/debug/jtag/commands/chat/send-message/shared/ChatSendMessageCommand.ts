/**
 * Chat Send Message Command - Shared Base
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { generateUUID, type UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { 
  type ChatSendMessageParams, 
  type ChatSendMessageResult,
  createChatSendMessageResult,
  createChatSendMessageError
} from './ChatSendMessageTypes';

export abstract class ChatSendMessageCommand extends CommandBase<ChatSendMessageParams, ChatSendMessageResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('chat-send-message', context, subpath, commander);
  }

  /**
   * Execute chat message sending
   */
  async execute(params: JTAGPayload): Promise<ChatSendMessageResult> {
    const chatParams = params as ChatSendMessageParams;
    const startTime = Date.now();
    
    console.log(`💬 ${this.getEnvironmentLabel()}: Sending message to room ${chatParams.roomId}`);

    try {
      // 1. Create message object
      const messageId = generateUUID();
      const timestamp = new Date().toISOString();
      const senderName = this.getSenderName(chatParams);
      
      const message = {
        messageId,
        roomId: chatParams.roomId,
        content: chatParams.content,
        senderName,
        timestamp,
        senderId: chatParams.sessionId,
        mentions: chatParams.mentions || [],
        category: chatParams.category || 'chat',
        replyToId: chatParams.replyToId,
        messageContext: chatParams.messageContext
      };

      // 2. Store message via DataDaemon
      await this.storeMessage(message, chatParams.sessionId);
      
      // 3. Emit event for widgets and other listeners
      await this.emitMessageEvent(message);
      
      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Message ${messageId} sent in ${duration}ms`);

      return createChatSendMessageResult(chatParams, {
        messageId,
        timestamp,
        message: {
          messageId,
          roomId: chatParams.roomId,
          content: chatParams.content,
          senderName,
          timestamp
        }
      });

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Failed to send message:`, error);
      
      return createChatSendMessageError(
        chatParams,
        error instanceof Error ? error.message : 'Unknown error'
      );
    }
  }

  /**
   * Store message using data/create command
   */
  private async storeMessage(message: any, sessionId: UUID): Promise<void> {
    try {
      const dataCreateCommand = this.commander.commands.get('data/create');
      if (!dataCreateCommand) {
        console.error(`❌ data/create command not available`);
        return;
      }

      const storeResult = await dataCreateCommand.execute({
        collection: 'chat_messages',
        data: message,
        context: this.context,
        sessionId: sessionId
      } as any);
      
      const result = storeResult as any;
      if (!result.success) {
        console.error(`❌ Failed to store message ${message.messageId}:`, result.error);
      } else {
        console.log(`💾 Stored message ${message.messageId} via data/create command`);
      }
    } catch (error) {
      console.error(`💥 Error storing message ${message.messageId}:`, error);
    }
  }

  /**
   * Get sender display name
   */
  private getSenderName(params: ChatSendMessageParams): string {
    // Try to get from message context
    if (params.messageContext && typeof params.messageContext === 'object') {
      const sessionInfo = (params.messageContext as any)?.sessionInfo;
      if (sessionInfo && typeof sessionInfo.displayName === 'string') {
        return sessionInfo.displayName;
      }
    }
    
    // Fallback to session-based name
    return `Session-${params.sessionId.substring(0, 8)}`;
  }

  /**
   * Emit message event for widgets and listeners  
   */
  private async emitMessageEvent(message: any): Promise<void> {
    try {
      // Use JTAGMessageFactory to create proper event message
      const { JTAGMessageFactory } = await import('../../../../system/core/types/JTAGTypes');
      
      const eventPayload = {
        eventName: 'chat-message-sent',
        data: { message },
        scope: {
          type: 'room',
          id: message.roomId,
          sessionId: message.senderId
        },
        originSessionId: message.senderId,
        timestamp: message.timestamp
      };
      
      const { EVENT_ENDPOINTS } = await import('../../../../daemons/events-daemon/shared/EventEndpoints');
      
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'chat-send-message',
        `events/${EVENT_ENDPOINTS.BRIDGE}`,
        eventPayload as any
      );
      
      await this.commander.router.postMessage(eventMessage);
      console.log(`📨 Emitted chat-message-sent event for message ${message.messageId}`);
      
    } catch (error) {
      console.error(`❌ Failed to emit message event:`, error);
      // Don't fail the entire operation if event emission fails
    }
  }

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}