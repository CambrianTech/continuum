/**
 * Chat Daemon Server - Chat System Orchestrator
 * 
 * Uses DataDaemon for persistence, provides universal participant-agnostic
 * chat functionality via strong typing and constants.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import { DataDaemon, DataOperationContext } from '../../data-daemon/shared/DataDaemon';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';

// Professional Data Architecture - Rust-like strict typing
import { DataServiceFactory, DataServiceMode } from '../../../system/data/services/DataServiceFactory';
import type { DataService } from '../../../system/data/services/DataService';
import type { DataResult, ISOString, BaseEntity } from '../../../system/data/domains/CoreTypes';
import { UserId, SessionId, RoomId, MessageId } from '../../../system/data/domains/CoreTypes';
import type { ChatMessage, CreateMessageData, MessageContent, MessageMetadata } from '../../../system/data/domains/ChatMessage';
import { validateMessageData, processMessageFormatting } from '../../../system/data/domains/ChatMessage';
import type { User, CreateUserData } from '../../../system/data/domains/User';
import { validateUserData } from '../../../system/data/domains/User';
import {
  SessionParticipant,
  ChatRoom,
  ChatMessage as LegacyChatMessage,
  ChatCreateRoomParams,
  ChatJoinRoomParams,
  ChatSendMessageParams,
  ChatListRoomsParams,
  ChatLeaveRoomParams,
  ChatCreateRoomResult,
  ChatJoinRoomResult,
  ChatSendMessageResult,
  ChatListRoomsResult,
  ChatLeaveRoomResult,
  createChatCreateRoomResult,
  createChatJoinRoomResult,
  createChatSendMessageResult,
  createChatListRoomsResult,
  createChatLeaveRoomResult
} from '../shared/ChatTypes';

/**
 * Chat operation payload for message handling
 */
interface ChatOperationPayload {
  readonly operation: 'create_room' | 'join_room' | 'send_message' | 'list_rooms' | 'leave_room' | 'get_history';
  readonly params: any; // Specific chat params based on operation
}

/**
 * ChatDaemonServer - Universal Chat System Orchestrator
 * 
 * Uses data daemon for all persistence operations. Provides participant-agnostic
 * chat functionality with strong typing preventing runtime errors.
 */
export class ChatDaemonServer extends DaemonBase {
  public readonly subpath: string = 'chat';
  private readonly dataDaemon: DataDaemon;
  private professionalDataService: DataService | null = null; // Rust-like nullable type
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('chat-daemon', context, router);
    
    // Initialize data daemon for chat persistence (legacy - keep working)
    this.dataDaemon = new DataDaemon({
      strategy: 'file',
      backend: 'file',
      namespace: 'chat-system',
      options: {
        basePath: '.continuum/jtag/sessions/user',
        createDirectories: true,
        atomicWrites: true
      }
    });
    
    // Professional service will be initialized in the initialize() method
  }

  /**
   * Initialize professional data service - Rust-like error handling with Result types
   * Non-blocking initialization - if it fails, we continue with legacy DataDaemon
   */
  private async initializeProfessionalDataService(): Promise<void> {
    try {
      console.log('🏗️ ChatDaemon: Initializing professional data service with strict typing');
      
      // JsonFileAdapter: uses existing JSON files (proven working)
      // Note: DataService needs a session ID, but daemon doesn't have one yet
      // The actual sessionId will be provided in the message parameters
      this.professionalDataService = await DataServiceFactory.createJsonCompatible(
        '.continuum/database'            // Existing JSON files (backwards compatible)
        // No sessionId provided here - will be passed per-operation in context override
      );
      
      console.log('✅ ChatDaemon: Professional data service initialized successfully');
      console.log('   📊 Features: Rust-like typing, Discord-scale messaging, rich formatting');
      console.log('   🔄 Migration: Reading existing JSON, writing to SQLite');
      
    } catch (error: any) {
      console.log('⚠️ ChatDaemon: Professional data service unavailable, using legacy DataDaemon');
      console.log(`   💡 Reason: ${error.message}`);
      console.log('   🔧 Install sqlite3 for professional features: npm install sqlite3 @types/sqlite3');
      this.professionalDataService = null;
    }
  }

  /**
   * Handle incoming chat operation messages
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as unknown as ChatOperationPayload;
    
    try {
      let result: any;
      
      switch (payload.operation) {
        case 'create_room':
          result = await this.handleCreateRoom(payload.params as ChatCreateRoomParams);
          break;
        case 'join_room':
          result = await this.handleJoinRoom(payload.params as ChatJoinRoomParams);
          break;
        case 'send_message':
          result = await this.handleSendMessage(payload.params as ChatSendMessageParams);
          break;
        case 'list_rooms':
          result = await this.handleListRooms(payload.params as ChatListRoomsParams);
          break;
        case 'leave_room':
          result = await this.handleLeaveRoom(payload.params as ChatLeaveRoomParams);
          break;
        default:
          result = createBaseResponse(false, this.context, payload.params?.sessionId || this.context.uuid, {
            error: `Unknown chat operation: ${payload.operation}`
          });
      }
      
      return result;
      
    } catch (error: any) {
      return createBaseResponse(false, this.context, payload.params?.sessionId || this.context.uuid, {
        error: `Chat daemon error: ${error.message}`
      });
    }
  }

  /**
   * Create new chat room
   */
  private async handleCreateRoom(params: ChatCreateRoomParams): Promise<BaseResponsePayload> {
    const context = this.createDataContext('chat-create-room');
    
    const roomData: Partial<ChatRoom> = {
      name: params.name,
      description: params.description,
      isPrivate: params.isPrivate || false,
      participantCount: 0,
      messageCount: 0,
      createdAt: context.timestamp,
      lastActivity: context.timestamp,
      category: 'general'
    };

    const createResult = await this.dataDaemon.create('chat-rooms', roomData, context);
    
    if (!createResult.success) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: createResult.error
      });
    }

    const room: ChatRoom = {
      roomId: createResult.data!.id,
      name: roomData.name!,
      description: roomData.description,
      createdAt: roomData.createdAt!,
      lastActivity: roomData.lastActivity!,
      participantCount: roomData.participantCount!,
      messageCount: roomData.messageCount!,
      isPrivate: roomData.isPrivate!,
      category: roomData.category
    };

    const result = createChatCreateRoomResult(params.context, params.sessionId, {
      success: true,
      roomId: room.roomId,
      room,
      timestamp: context.timestamp
    });

    return createBaseResponse(true, this.context, params.sessionId, result);
  }
  /**
   * Join participant to chat room
   */
  private async handleJoinRoom(params: ChatJoinRoomParams): Promise<BaseResponsePayload> {
    const context = this.createDataContext('chat-join-room');
    
    // Get room data
    const roomResult = await this.dataDaemon.read('chat-rooms', params.roomId, context);
    if (!roomResult.success || !roomResult.data) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: 'Room not found'
      });
    }

    // Create participant record
    const participantData: Partial<SessionParticipant> = {
      sessionId: params.sessionId,
      displayName: params.participantName,
      joinedAt: context.timestamp,
      lastSeen: context.timestamp,
      isOnline: true,
      capabilities: params.capabilities || {
        canSendMessages: true,
        canReceiveMessages: true,
        canCreateRooms: false,
        canInviteOthers: false,
        canModerate: false,
        autoResponds: false,
        providesContext: false
      },
      subscribedRooms: [params.roomId]
    };

    const participantResult = await this.dataDaemon.create('chat-participants', participantData, context);
    
    if (!participantResult.success) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: participantResult.error
      });
    }

    // Update room participant count
    const roomData = roomResult.data.data as any;
    const updatedRoomData = {
      participantCount: (roomData.participantCount || 0) + 1,
      lastActivity: context.timestamp
    };
    
    await this.dataDaemon.update('chat-rooms', params.roomId, updatedRoomData, context);

    const participant: SessionParticipant = {
      participantId: participantResult.data!.id,
      sessionId: participantData.sessionId!,
      displayName: participantData.displayName!,
      joinedAt: participantData.joinedAt!,
      lastSeen: participantData.lastSeen!,
      isOnline: participantData.isOnline!,
      capabilities: participantData.capabilities,
      subscribedRooms: participantData.subscribedRooms
    };

    const room: ChatRoom = {
      roomId: roomResult.data.id,
      name: roomData.name,
      description: roomData.description,
      createdAt: roomData.createdAt,
      lastActivity: roomData.lastActivity,
      participantCount: roomData.participantCount,
      messageCount: roomData.messageCount,
      isPrivate: roomData.isPrivate
    };

    const result = createChatJoinRoomResult(params.context, params.sessionId, {
      success: true,
      participantId: participant.participantId,
      room,
      recentMessages: [],
      participantList: [participant],
      timestamp: context.timestamp
    });

    return createBaseResponse(true, this.context, params.sessionId, result);
  }

  /**
   * Send message to chat room
   */
  private async handleSendMessage(params: ChatSendMessageParams): Promise<BaseResponsePayload> {
    // Hybrid approach: try professional first, fall back to legacy
    console.log(`🔍 DEBUG: professionalDataService is ${this.professionalDataService ? 'available' : 'null'}`);
    
    if (this.professionalDataService) {
      console.log('💬 ChatDaemon: Using professional data service with Rust-like typing');
      try {
        return await this.handleSendMessageProfessional(params);
      } catch (error: any) {
        console.log(`⚠️ Professional path failed: ${error.message}, falling back to legacy`);
        return await this.handleSendMessageLegacy(params);
      }
    } else {
      console.log('💬 ChatDaemon: Using legacy DataDaemon (professional service unavailable)');
      return await this.handleSendMessageLegacy(params);
    }
  }

  /**
   * Professional message handling with Rust-like strict typing
   * - Uses branded types (UserId, RoomId, MessageId)
   * - Rich message formatting (@mentions, #hashtags, ```code```)
   * - Result type error handling (no exceptions)
   * - Discord-scale features
   */
  private async handleSendMessageProfessional(params: ChatSendMessageParams): Promise<BaseResponsePayload> {
    try {
      // Rust-like strict typing with branded types
      const roomId: RoomId = RoomId(params.roomId);
      const senderId: UserId = UserId(params.sessionId); // Use session as sender ID
      
      // Professional message data with rich formatting
      const messageData: CreateMessageData = {
        roomId,
        senderId,
        content: {
          text: params.content,
          formatting: processMessageFormatting(params.content) // Discord-like rich formatting!
        },
        priority: 'normal',
        mentions: (params.mentions || []).map((id: string) => UserId(id)),
        replyToId: params.replyToId ? MessageId(params.replyToId) : undefined
      };

      // Validate message data (Rust-like explicit validation)
      const validation = validateMessageData(messageData);
      if (!validation.success) {
        return createBaseResponse(false, this.context, params.sessionId, {
          error: `Message validation failed: ${validation.error.message}`
        });
      }

      // Create context for professional data service
      const professionalContext = {
        sessionId: params.sessionId as any, // TODO: Fix branded type conversion
        timestamp: new Date().toISOString() as ISOString,
        source: 'chat-daemon'
      };

      // Convert CreateMessageData to ChatMessage format for DataService.create()
      const chatMessage: Omit<ChatMessage, keyof BaseEntity> = {
        messageId: MessageId(generateUUID()),
        roomId: messageData.roomId,
        senderId: messageData.senderId,
        senderName: 'Professional User', // Would be populated from user service
        content: {
          text: messageData.content.text,
          attachments: messageData.content.attachments || [],
          formatting: processMessageFormatting(messageData.content.text),
          embeds: []
        } as MessageContent,
        status: 'sent',
        priority: messageData.priority || 'normal',
        timestamp: professionalContext.timestamp as ISOString,
        reactions: [],
        replyToId: messageData.replyToId,
        metadata: {
          source: 'user' as const,
          ...messageData.metadata
        } as MessageMetadata
      };

      // Create message using professional data service
      const createResult = await this.professionalDataService!.create<ChatMessage>('messages', chatMessage, professionalContext);
      
      // Type guard to ensure we get a ChatMessage from the create operation
      if (createResult.success && typeof createResult.data !== 'object') {
        console.log('⚠️ Unexpected data type from create operation');
        return await this.handleSendMessageLegacy(params);
      }
      
      if (!createResult.success) {
        console.log('⚠️ Professional create failed, falling back to legacy');
        return await this.handleSendMessageLegacy(params);
      }

      const professionalMessage = createResult.data as ChatMessage;

      // Convert professional message to legacy format for backwards compatibility
      const legacyMessage: LegacyChatMessage = {
        messageId: professionalMessage.id,
        roomId: professionalMessage.roomId,
        senderId: professionalMessage.senderId,
        senderName: 'Participant', // Would get from user lookup
        content: professionalMessage.content.text,
        timestamp: professionalMessage.createdAt,
        mentions: professionalMessage.content.formatting.mentions || [],
        category: 'chat',
        replyToId: professionalMessage.replyToId,
        messageContext: params.messageContext
      };

      const result = createChatSendMessageResult(params.context, params.sessionId, {
        success: true,
        messageId: legacyMessage.messageId,
        message: legacyMessage,
        timestamp: professionalMessage.createdAt
      });

      console.log('✅ ChatDaemon: Professional message created with rich formatting');
      console.log(`   📝 Features: ${professionalMessage.content.formatting.hashtags.length} hashtags, ${professionalMessage.content.formatting.links.length} links`);
      
      return createBaseResponse(true, this.context, params.sessionId, result);
      
    } catch (error: any) {
      console.log(`⚠️ Professional message handling failed: ${error.message}, falling back to legacy`);
      return await this.handleSendMessageLegacy(params);
    }
  }

  /**
   * Legacy message handling (original implementation)
   * Keeps existing functionality working unchanged
   */
  private async handleSendMessageLegacy(params: ChatSendMessageParams): Promise<BaseResponsePayload> {
    const context = this.createDataContext('chat-send-message');
    
    const messageData: Partial<LegacyChatMessage> = {
      roomId: params.roomId,
      senderId: params.sessionId, // Use session as sender
      senderName: 'Participant', // Would get from participant record
      content: params.content,
      timestamp: context.timestamp,
      mentions: params.mentions || [],
      category: params.category || 'chat',
      replyToId: params.replyToId,
      messageContext: params.messageContext
    };

    const createResult = await this.dataDaemon.create('chat-messages', messageData, context);
    
    if (!createResult.success) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: createResult.error
      });
    }

    const message: LegacyChatMessage = {
      messageId: createResult.data!.id,
      roomId: messageData.roomId!,
      senderId: messageData.senderId!,
      senderName: messageData.senderName!,
      content: messageData.content!,
      timestamp: messageData.timestamp!,
      mentions: messageData.mentions!,
      category: messageData.category!,
      replyToId: messageData.replyToId,
      messageContext: messageData.messageContext
    };

    const result = createChatSendMessageResult(params.context, params.sessionId, {
      success: true,
      messageId: message.messageId,
      message,
      timestamp: context.timestamp
    });

    return createBaseResponse(true, this.context, params.sessionId, result);
  }

  /**
   * List all chat rooms
   */
  private async handleListRooms(params: ChatListRoomsParams): Promise<BaseResponsePayload> {
    const context = this.createDataContext('chat-list-rooms');
    
    const queryResult = await this.dataDaemon.query({
      collection: 'chat-rooms',
      filters: {},
      sort: [{ field: 'data.lastActivity', direction: 'desc' }]
    }, context);

    if (!queryResult.success) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: queryResult.error
      });
    }

    const rooms: ChatRoom[] = queryResult.data?.map(record => {
      const data = record.data as any;
      return {
        roomId: record.id,
        name: data.name,
        description: data.description,
        createdAt: data.createdAt,
        lastActivity: data.lastActivity,
        participantCount: data.participantCount,
        messageCount: data.messageCount,
        isPrivate: data.isPrivate,
        category: data.category
      };
    }) || [];

    const result = createChatListRoomsResult(params.context, params.sessionId, {
      success: true,
      rooms,
      totalCount: rooms.length,
      timestamp: context.timestamp
    });

    return createBaseResponse(true, this.context, params.sessionId, result);
  }

  /**
   * Leave chat room
   */
  private async handleLeaveRoom(params: ChatLeaveRoomParams): Promise<BaseResponsePayload> {
    const context = this.createDataContext('chat-leave-room');
    
    // Remove participant record
    const deleteResult = await this.dataDaemon.delete('chat-participants', params.participantId, context);
    
    if (!deleteResult.success) {
      return createBaseResponse(false, this.context, params.sessionId, {
        error: deleteResult.error
      });
    }

    // Update room participant count (simplified - would need proper count query)
    const roomUpdateData = {
      lastActivity: context.timestamp
    };
    
    await this.dataDaemon.update('chat-rooms', params.roomId, roomUpdateData, context);

    const result = createChatLeaveRoomResult(params.context, params.sessionId, {
      success: true,
      roomId: params.roomId,
      participantId: params.participantId,
      timestamp: context.timestamp
    });

    return createBaseResponse(true, this.context, params.sessionId, result);
  }

  /**
   * Create data operation context for chat operations
   */
  private createDataContext(source: string): DataOperationContext {
    return {
      sessionId: this.context.uuid,
      timestamp: new Date().toISOString(),
      source,
      consistency: 'strong'
    };
  }

  /**
   * Initialize daemon-specific functionality
   */
  protected async initialize(): Promise<void> {
    console.log('💬 ChatDaemonServer: Initializing with data daemon persistence');
    // Data daemon initializes itself
    
    // Initialize professional data service (blocks until ready)
    await this.initializeProfessionalDataService();
  }
}
