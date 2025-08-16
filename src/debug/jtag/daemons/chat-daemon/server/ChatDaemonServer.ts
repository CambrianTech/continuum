/**
 * Chat Daemon Server - Chat System Orchestrator
 * 
 * Uses DataDaemon for persistence, provides universal participant-agnostic
 * chat functionality via strong typing and constants.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import { DataDaemon, DataOperationContext } from '../../data-daemon/shared/DataDaemon';
import type { StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import {
  SessionParticipant,
  ChatRoom,
  ChatMessage,
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
  private dataDaemon: DataDaemon;
  
  constructor(context: JTAGContext, router: JTAGRouter) {
    super('chat-daemon', context, router);
    
    // Initialize data daemon for chat persistence
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
    const context = this.createDataContext('chat-send-message');
    
    const messageData: Partial<ChatMessage> = {
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

    const message: ChatMessage = {
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
  }
}
