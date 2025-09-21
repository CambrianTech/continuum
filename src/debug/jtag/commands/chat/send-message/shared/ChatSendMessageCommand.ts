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
import { ChatMessageData, type MessagePriority, MESSAGE_STATUS } from '../../../../system/data/domains/ChatMessage';
import { processMessageFormatting } from '../../../../system/data/domains/ChatMessage';
import { UserId, RoomId, MessageId, ISOString } from '../../../../system/data/domains/CoreTypes';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
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

      // 2. Store message using data/create command - returns ChatMessage directly
      const storedMessage = await this.storeMessage(messageData);

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
   * Store message using data/create command - now returns actual ChatMessage
   */
  private async storeMessage(messageData: CreateMessageData): Promise<ChatMessageData> {
    console.log(`🔥 CLAUDE-FIX-${Date.now()}: STORE: About to store message to database`);

    // Use CreateMessageData - let the data layer handle BaseEntity fields
    const messageId = MessageId(generateUUID());

    const createData: CreateMessageData = {
      roomId: messageData.roomId,
      senderId: messageData.senderId,
      content: {
        text: messageData.content.text,
        attachments: messageData.content.attachments || [],
        formatting: {
          markdown: messageData.content.formatting?.markdown || false,
          mentions: messageData.content.formatting?.mentions || [],
          hashtags: messageData.content.formatting?.hashtags || [],
          links: messageData.content.formatting?.links || [],
          codeBlocks: messageData.content.formatting?.codeBlocks || []
        }
      },
      priority: 'normal' as MessagePriority, // Default priority
      mentions: messageData.content.formatting?.mentions,
      replyToId: messageData.replyToId as MessageId | undefined,
      metadata: {
        source: messageData.metadata?.source || 'user',
        deviceType: messageData.metadata?.deviceType,
        clientVersion: messageData.metadata?.clientVersion,
        editHistory: messageData.metadata?.editHistory,
        deliveryReceipts: messageData.metadata?.deliveryReceipts
      }
    };

    // Clean domain data - adapter handles BaseEntity fields + timestamp conversion
    const domainData = {
      messageId,
      senderName: `User-${messageData.senderId.substring(0, 8)}`,
      status: MESSAGE_STATUS.SENT,
      timestamp: new Date().toISOString() as ISOString, // Domain requires ISOString
      reactions: [],
      ...createData,
      priority: createData.priority || 'normal', // Ensure priority is always set
      content: {
        ...createData.content,
        attachments: createData.content.attachments || [], // Ensure attachments is always defined
        formatting: {
          markdown: createData.content.formatting?.markdown || false,
          mentions: createData.content.formatting?.mentions || [],
          hashtags: createData.content.formatting?.hashtags || [],
          links: createData.content.formatting?.links || [],
          codeBlocks: createData.content.formatting?.codeBlocks || []
        }
      },
      metadata: {
        source: createData.metadata?.source || 'user',
        deviceType: createData.metadata?.deviceType,
        clientVersion: createData.metadata?.clientVersion,
        editHistory: createData.metadata?.editHistory,
        deliveryReceipts: createData.metadata?.deliveryReceipts
      }
    };

    const createParams: DataCreateParams<ChatMessageData> = {
      collection: ChatMessageEntity.collection,
      data: domainData, // Properly typed as Omit<ChatMessageData, keyof BaseEntity>
      context: this.context,
      sessionId: messageData.senderId
    };

    const result = await this.remoteExecute<DataCreateParams<ChatMessageData>, DataCreateResult<ChatMessageData>>(
      createParams,
      'data/create'
    );

    console.log(`🔥 CLAUDE-FIX-${Date.now()}: STORE-RESULT: Store result for ${messageId}:`, result);

    if (!result.success) {
      throw new Error(`Failed to store message ${messageId}: ${result.error}`);
    }

    console.log(`💾 Stored message ${messageId} in global database`);

    // Return the stored ChatMessageData - adapter filled in BaseEntity fields
    return result.data!;
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
  protected abstract emitMessageEvent(message: ChatMessageData): Promise<void>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;
}