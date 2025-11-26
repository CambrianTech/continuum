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
import { ChatMessageEntity, type MediaItem } from '../../../../system/data/entities/ChatMessageEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { Commands } from '../../../../system/core/shared/Commands';

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

    // Process media files if provided
    // Normalize to array: CLI may send string (single) or string[] (multiple)
    const mediaPaths = params.media
      ? (Array.isArray(params.media) ? params.media : [params.media])
      : [];
    console.log('🔧 ChatSendServerCommand MEDIA PROCESSING', {
      rawMedia: params.media,
      isArray: Array.isArray(params.media),
      normalizedPaths: mediaPaths,
      pathCount: mediaPaths.length
    });
    const mediaItems = mediaPaths.length > 0 ? await this.processMediaPaths(mediaPaths, params.context, params.sessionId) : [];
    console.log('🔧 ChatSendServerCommand MEDIA PROCESSED', {
      mediaItemCount: mediaItems.length,
      mediaItems: mediaItems.map(m => ({ type: m.type, mimeType: m.mimeType, filename: m.filename, hasBase64: !!m.base64, base64Length: m.base64?.length }))
    });

    messageEntity.content = {
      text: params.message,
      media: mediaItems
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

    // 4. Store message using DataDaemon.store (clean server-side interface)
    // DataDaemon.store() handles event broadcast to BOTH local (PersonaUsers) AND remote (browser) subscribers
    const storedEntity = await DataDaemon.store<ChatMessageEntity>(
      ChatMessageEntity.collection,
      messageEntity
    );

    // 5. Generate short ID (last 6 chars of UUID - from BaseEntity.id)
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

  /**
   * Process media file paths into MediaItem objects
   * Uses file/mime-type and file/load commands (clean composition)
   */
  private async processMediaPaths(mediaPaths: string[], context: JTAGContext, sessionId: UUID): Promise<MediaItem[]> {
    const mediaItems: MediaItem[] = [];
    console.log(`🔧 processMediaPaths START: Processing ${mediaPaths.length} file(s)`);

    for (const filePath of mediaPaths) {
      try {
        console.log(`🔧 processMediaPaths: Processing file: ${filePath}`);

        // Step 1: Detect MIME type using file/mime-type command
        const mimeResult = await Commands.execute<{ filepath: string; context: JTAGContext; sessionId: UUID }, any>('file/mime-type', {
          filepath: filePath,
          context,
          sessionId
        });
        console.log(`🔧 processMediaPaths: MIME result for ${filePath}:`, { success: mimeResult.success, mimeType: mimeResult.mimeType, mediaType: mimeResult.mediaType });

        if (!mimeResult.success) {
          const error = new Error(`Failed to detect MIME type for: ${filePath}`);
          if (mimeResult.error) {
            (error as any).cause = mimeResult.error;
          }
          throw error;
        }

        // Step 2: Load file content as base64 using file/load command
        const fileResult = await Commands.execute<{ filepath: string; encoding: string; context: JTAGContext; sessionId: UUID }, any>('file/load', {
          filepath: filePath,
          encoding: 'base64',
          context,
          sessionId
        });
        console.log(`🔧 processMediaPaths: Load result for ${filePath}:`, { success: fileResult.success, contentLength: fileResult.content?.length, hasError: !!fileResult.error });

        if (!fileResult.success) {
          const error = new Error(`Failed to load media file: ${filePath}`);
          if (fileResult.error) {
            (error as any).cause = fileResult.error;
          }
          throw error;
        }

        // Step 3: Create MediaItem with data from both commands
        const mediaItem: MediaItem = {
          type: mimeResult.mediaType,
          base64: fileResult.content,
          mimeType: mimeResult.mimeType,
          filename: filePath.split('/').pop() || filePath
        };
        console.log(`✅ processMediaPaths: Created MediaItem for ${filePath}:`, { type: mediaItem.type, mimeType: mediaItem.mimeType, filename: mediaItem.filename, base64Length: mediaItem.base64?.length || 0 });

        mediaItems.push(mediaItem);
      } catch (error) {
        // Re-throw with better context, preserving original error
        const enhancedError = new Error(`Media processing failed for ${filePath}: ${error instanceof Error ? error.message : String(error)}`);
        if (error instanceof Error) {
          (enhancedError as any).cause = error;
        }
        throw enhancedError;
      }
    }

    console.log(`🔧 processMediaPaths END: Processed ${mediaItems.length}/${mediaPaths.length} files successfully`);
    return mediaItems;
  }
}
