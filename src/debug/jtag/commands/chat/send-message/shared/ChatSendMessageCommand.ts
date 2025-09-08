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
import type { DataCreateParams, DataCreateResult } from '../../../data/create/shared/DataCreateTypes';
import { ChatMessage } from '../../../../domain/chat/ChatMessage';

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
      // 1. Create domain object - will handle validation and structure
      // TODO: Need User and ChatRoom objects - for now create directly
      const message = ChatMessage.fromData({
        messageId: generateUUID(),
        roomId: chatParams.roomId,
        content: chatParams.content,
        senderId: chatParams.sessionId,
        timestamp: new Date().toISOString(),
        mentions: [], // TODO: Parse mentions from content or params
        category: 'chat',
        replyToId: chatParams.replyToId
      });

      // 2. Store message - let errors bubble up, no fallbacks  
      await this.storeMessage(message);
      
      // 3. Emit event for widgets and other listeners (server-specific)
      await this.emitMessageEvent(message);
      
      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Message ${message.messageId} sent in ${duration}ms`);

      return createChatSendMessageResult(chatParams, {
        messageId: message.messageId,
        message: message
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
   * Store message using data/create command - domain object approach
   */
  private async storeMessage(message: ChatMessage): Promise<void> {
    const createParams: DataCreateParams = {
      collection: 'chat_messages',
      data: message.toData(),
      context: this.context,
      sessionId: message.senderId
    };
    
    const result = await this.remoteExecute<DataCreateParams, DataCreateResult>(
      createParams, 
      'data/create'
    );

    if (!result.success) {
      throw new Error(`Failed to store message ${message.messageId}: ${result.error}`);
    }
    
    console.log(`💾 Stored message ${message.messageId} in global database`);
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
   * Emit message event for widgets and listeners (abstract - implemented per environment)
   */
  protected abstract emitMessageEvent(message: any): Promise<void>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}