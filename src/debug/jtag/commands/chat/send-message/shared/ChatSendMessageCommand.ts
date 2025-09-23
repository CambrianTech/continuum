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
import { ChatMessageEntity, type CreateMessageData, type MessagePriority, type MessageContent, MESSAGE_STATUS, processMessageFormatting } from '../../../../system/data/entities/ChatMessageEntity';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import { DEFAULT_USERS } from '../../../../system/data/domains/DefaultEntities';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { DataCreateParams, DataCreateResult } from '../../../data/create/shared/DataCreateTypes';
// Domain types removed - using UUID directly
// UserIdManager removed - using direct user lookup via commands

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
      let senderName: string;

      if (chatParams.senderType === 'system') {
        senderId = 'system';
        senderName = 'System';
      } else if (chatParams.senderType === 'server') {
        senderId = 'server';
        senderName = 'Server';
      } else {
        // For user messages, use the hardcoded Joel user ID
        senderId = DEFAULT_USERS.HUMAN;
        senderName = 'Joel'; // Historical snapshot of displayName
        console.log(`🔧 CLAUDE-USER-ID-DEBUG: Using Joel's UUID: ${senderId}`);
      }
      
      console.log(`🔧 CLAUDE-SENDER-DEBUG: senderType="${chatParams.senderType}", sessionId="${chatParams.sessionId}", using senderId="${senderId}"`);

      // Create MessageContent object
      const baseContent: MessageContent = {
        text: chatParams.content,
        attachments: []
      };
      const messageFormatting = processMessageFormatting(baseContent);

      const messageData = new ChatMessageEntity();
      // BaseEntity auto-generates id, createdAt, updatedAt, version
      messageData.roomId = chatParams.roomId as UUID;
      messageData.senderId = senderId as UUID;
      messageData.senderName = senderName; // Historical snapshot of displayName
      messageData.content = messageFormatting;
      messageData.priority = 'normal';
      messageData.status = 'sending';
      messageData.timestamp = new Date();
      messageData.reactions = [];
      messageData.replyToId = chatParams.replyToId as UUID;

      // 2. Store message using data/create command - returns ChatMessage directly
      const storedMessage = await this.storeMessage(messageData, chatParams.sessionId);

      // 4. Emit event for widgets and other listeners (server-specific)
      await this.emitMessageEvent(storedMessage);

      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Message ${storedMessage.id} sent in ${duration}ms`);

      return createChatSendMessageResult(chatParams, {
        messageId: storedMessage.id,
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
   * Store message using superior data/create command system
   */
  private async storeMessage(messageData: ChatMessageEntity, sessionId: string): Promise<ChatMessageEntity> {
    console.log(`🔥 CLAUDE-FIX-${Date.now()}: STORE: Using superior data/create system for message ${messageData.id}`);

    // Use the superior command system instead of legacy DataDaemon.store()
    const params: DataCreateParams<ChatMessageEntity> = {
      collection: ChatMessageEntity.collection,
      data: messageData,
      id: messageData.id,
      sessionId: sessionId,
      context: this.context
    };

    const result = await this.remoteExecute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(
      params,
      'data/create',
      'server'
    );

    if (!result.success || !result.data) {
      throw new Error(`Failed to store message ${messageData.id}: ${result.error}`);
    }

    console.log(`💾 Stored message ${messageData.id} using superior command system`);
    return result.data; // Return the data from the successful create
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
  protected abstract emitMessageEvent(message: ChatMessageEntity): Promise<void>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}