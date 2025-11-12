/**
 * Chat Send Command - Server Implementation
 * Sends chat messages directly to database (no UI)
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatSendCommand } from '../shared/ChatSendCommand';
import type { ChatSendParams, ChatSendResult } from '../shared/ChatSendTypes';
import type { RoomEntity } from '../../../../system/data/entities/RoomEntity';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';

export class ChatSendServerCommand extends ChatSendCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    console.log('🔧 ChatSendServerCommand.executeChatSend START', { room: params.room });

    // 1. Find room
    const room = await this.findRoom(params.room || 'general');
    console.log('🔧 ChatSendServerCommand.executeChatSend ROOM FOUND', { roomId: room.id, roomName: room.entity.name });

    // 2. Get sender
    const sender = params.senderId
      ? await this.findUserById(params.senderId)
      : await this.findDefaultHumanUser();
    console.log('🔧 ChatSendServerCommand.executeChatSend SENDER FOUND', { senderId: sender.id, senderName: sender.entity.displayName });

    // 3. Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = room.id;  // Use DataRecord.id, not entity.id
    messageEntity.senderId = sender.id;  // sender is also DataRecord with .id
    console.log('🔧 ChatSendServerCommand.executeChatSend MESSAGE ENTITY', { roomId: messageEntity.roomId, senderId: messageEntity.senderId });
    messageEntity.senderName = sender.entity.displayName;
    messageEntity.senderType = sender.entity.type;
    messageEntity.content = {
      text: params.message,
      attachments: []
    };
    messageEntity.status = 'sent';
    messageEntity.priority = 'normal';
    messageEntity.timestamp = new Date();
    messageEntity.reactions = [];

    if (params.replyToId) {
      messageEntity.replyToId = params.replyToId;
    }

    if (params.isSystemTest) {
      messageEntity.metadata = {
        source: 'system',
        isSystemTest: true,
        testType: 'manual-test'
      };
    } else {
      messageEntity.metadata = {
        source: 'user'
      };
    }

    // 4. Store message (this emits 'data:chat_messages:created' event)
    const storedEntity = await DataDaemon.store('chat_messages', messageEntity);

    // 5. Generate short ID (last 6 chars of UUID)
    const shortId = storedEntity.id.slice(-6);

    console.log(`✅ Message sent: #${shortId} to ${room.entity.name}`);

    return transformPayload(params, {
      success: true,
      message: `Message sent to ${room.entity.name} (#${shortId})`,
      messageEntity: storedEntity,
      shortId: shortId,
      roomId: room.id
    });
  }

  /**
   * Find room by ID or name
   */
  private async findRoom(roomIdOrName: string): Promise<{ id: UUID; entity: RoomEntity }> {
    console.log('🔧 findRoom START', { roomIdOrName });

    // Query all rooms and filter in-memory (simpler, guaranteed to work)
    const result = await DataDaemon.query<RoomEntity>({
      collection: 'rooms',
      filter: {}
    });

    console.log('🔧 findRoom QUERY RESULT', {
      success: result.success,
      recordCount: result.data?.length || 0
    });

    if (!result.success || !result.data) {
      throw new Error('Failed to query rooms');
    }

    // Find by ID or name
    const record = result.data.find(r =>
      r.id === roomIdOrName || r.data.name === roomIdOrName
    );

    if (!record) {
      const roomNames = result.data.map(r => r.data.name).join(', ');
      console.log('🔧 findRoom NOT FOUND', { roomIdOrName, availableRooms: roomNames });
      throw new Error(`Room not found: ${roomIdOrName}. Available: ${roomNames}`);
    }

    console.log('🔧 findRoom FOUND', { roomId: record.id, roomName: record.data.name });
    return { id: record.id, entity: record.data };
  }

  /**
   * Find user by ID
   */
  private async findUserById(userId: UUID): Promise<{ id: UUID; entity: UserEntity }> {
    const result = await DataDaemon.query<UserEntity>({
      collection: 'users',
      filter: { id: userId },
      limit: 1
    });

    if (result.success && result.data && result.data.length > 0) {
      const record = result.data[0];
      return { id: record.id, entity: record.data };
    }

    throw new Error(`User not found: ${userId}`);
  }

  /**
   * Find default human user (most recently active)
   */
  private async findDefaultHumanUser(): Promise<{ id: UUID; entity: UserEntity }> {
    const result = await DataDaemon.query<UserEntity>({
      collection: 'users',
      filter: { type: 'human' },
      sort: [{ field: 'lastActiveAt', direction: 'desc' }],
      limit: 1
    });

    if (result.success && result.data && result.data.length > 0) {
      const record = result.data[0];
      return { id: record.id, entity: record.data };
    }

    throw new Error('No human user found');
  }
}
