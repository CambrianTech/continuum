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
import type { CreateMessageData } from '../../../../system/data/domains/ChatMessage';
import { ChatMessage } from '../../../../domain/chat/ChatMessage';
import { processMessageFormatting } from '../../../../system/data/domains/ChatMessage';
import { UserId, RoomId, MessageId, type ISOString } from '../../../../system/data/domains/CoreTypes';
import { userIdManager } from '../../../../system/shared/UserIdManager';

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
      // CRITICAL FIX: Use persistent User ID for user messages, system sender for others
      let senderId: string;
      
      if (chatParams.senderType === 'system') {
        senderId = 'system';
      } else if (chatParams.senderType === 'server') {
        senderId = 'server';
      } else {
        // For user messages, use persistent User ID from UserIdManager
        try {
          senderId = await userIdManager.getCurrentUserId();
          console.log(`🔧 CLAUDE-USER-ID-DEBUG: Using persistent User ID for user message: ${senderId}`);
        } catch (error) {
          console.warn('⚠️ ChatSendMessage: Failed to get User ID, falling back to Session ID:', error);
          senderId = chatParams.sessionId; // Fallback to Session ID
        }
      }
      
      console.log(`🔧 CLAUDE-SENDER-DEBUG: senderType="${chatParams.senderType}", sessionId="${chatParams.sessionId}", using senderId="${senderId}"`);

      // Create proper MessageContent object with formatting
      const messageFormatting = processMessageFormatting(chatParams.content);

      const messageData: CreateMessageData = {
        roomId: RoomId(chatParams.roomId),
        senderId: UserId(senderId),
        content: {
          text: chatParams.content,
          attachments: [],
          formatting: messageFormatting
        },
        priority: 'normal',
        mentions: messageFormatting.mentions,
        replyToId: chatParams.replyToId ? chatParams.replyToId as any : undefined,
        metadata: {
          source: chatParams.senderType === 'system' ? 'system' : 'user',
          deviceType: 'web'
        }
      };

      // 2. Store message using DataService - let errors bubble up, no fallbacks
      const storedMessageData = await this.storeMessage(messageData);

      // 3. Create proper ChatMessage domain object from stored data - adapter handles conversion
      const storedMessage = ChatMessage.fromData(storedMessageData);

      // 4. Emit event for widgets and other listeners (server-specific)
      await this.emitMessageEvent(storedMessage);

      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Message ${storedMessage.messageId} sent in ${duration}ms`);

      return createChatSendMessageResult(chatParams, {
        messageId: storedMessage.messageId,
        message: storedMessage
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
   * Store message using data/create command - proper DataService approach
   */
  private async storeMessage(messageData: CreateMessageData): Promise<any> {
    const messageId = generateUUID();
    console.log(`🔥 CLAUDE-FIX-${Date.now()}: STORE: About to store message ${messageId} to database`);

    const createParams: DataCreateParams = {
      collection: 'chat_messages',
      data: messageData,
      context: this.context,
      sessionId: messageData.senderId
    };

    const result = await this.remoteExecute<DataCreateParams, DataCreateResult>(
      createParams,
      'data/create'
    );

    console.log(`🔥 CLAUDE-FIX-${Date.now()}: STORE-RESULT: Store result for ${messageId}:`, result);

    if (!result.success) {
      throw new Error(`Failed to store message ${messageId}: ${result.error}`);
    }

    console.log(`💾 Stored message ${messageId} in global database`);

    // Return the created message with ID
    return {
      id: messageId,
      ...messageData,
      messageId: messageId
    };
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
  protected abstract emitMessageEvent(message: ChatMessage): Promise<void>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}