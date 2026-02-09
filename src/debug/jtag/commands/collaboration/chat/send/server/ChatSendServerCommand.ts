/**
 * Chat Send Command - Server Implementation
 * Sends chat messages directly to database (no UI)
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ChatSendCommand } from '../shared/ChatSendCommand';
import type { ChatSendParams, ChatSendResult } from '../shared/ChatSendTypes';
import { UserEntity } from '@system/data/entities/UserEntity';
import { ChatMessageEntity, type MediaItem } from '@system/data/entities/ChatMessageEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';
import { resolveRoomIdentifier } from '@system/routing/RoutingService';

import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';
import { FileMimeType } from '../../../../file/mime-type/shared/FileMimeTypeTypes';
import { FileLoad } from '../../../../file/load/shared/FileLoadTypes';
export class ChatSendServerCommand extends ChatSendCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    console.log('🔧 ChatSendServerCommand.executeChatSend START', { room: params.room });

    // 1. Find room (single source of truth: RoutingService)
    const resolved = await resolveRoomIdentifier(params.room || 'general');
    if (!resolved) {
      throw new Error(`Room not found: ${params.room || 'general'}`);
    }
    console.log('🔧 ChatSendServerCommand.executeChatSend ROOM FOUND', { roomId: resolved.id, roomName: resolved.displayName });

    // 2. Get sender - auto-detect caller identity (Claude Code, Joel, etc.)
    const sender = params.senderId
      ? await this.findUserById(params.senderId, params)
      : await this.findCallerIdentity(params);
    console.log('🔧 ChatSendServerCommand.executeChatSend SENDER FOUND', { senderId: sender.id, senderName: sender.entity.displayName });

    // 3. Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = resolved.id;  // From RoutingService resolution
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

    // 4. Store message using data/create command (proper delegation)
    // data/create handles validation, storage, and event broadcast
    const createResult = await DataCreate.execute<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        data: messageEntity,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!createResult.success || !createResult.data) {
      throw new Error(`Failed to store message: ${createResult.error || 'Unknown error'}`);
    }

    const storedEntity = createResult.data;

    // 5. Generate short ID (last 6 chars of UUID - from BaseEntity.id)
    const shortId = storedEntity.id.slice(-6);

    console.log(`✅ Message sent: #${shortId} to ${resolved.displayName}`);

    return transformPayload(params, {
      success: true,
      message: `Message sent to ${resolved.displayName} (#${shortId})`,
      messageEntity: storedEntity,
      shortId: shortId,
      roomId: resolved.id
    });
  }

  /**
   * Find user by ID
   */
  private async findUserById(userId: UUID, params: ChatSendParams): Promise<{ id: UUID; entity: UserEntity }> {
    const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: userId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (result.success && result.items && result.items.length > 0) {
      const user = result.items[0];
      return { id: user.id, entity: user };
    }

    throw new Error(`User not found: ${userId}`);
  }

  /**
   * Find caller identity - prefers context.userId (for PersonaUsers), falls back to process detection
   *
   * Priority:
   * 1. params.context?.userId - When a PersonaUser executes a command, their ID is in context
   * 2. UserIdentityResolver.resolve() - Detects Claude Code, Joel, etc. based on process info
   */
  private async findCallerIdentity(params: ChatSendParams): Promise<{ id: UUID; entity: UserEntity }> {
    // FIRST: Check if caller's userId is in the context (PersonaUsers set this)
    if (params.context?.userId) {
      console.log('🔧 ChatSendServerCommand.findCallerIdentity USING CONTEXT userId', { userId: params.context.userId });
      return this.findUserById(params.context.userId, params);
    }

    // FALLBACK: Use UserIdentityResolver to detect calling process (Claude Code, human, etc.)
    const identity = await UserIdentityResolver.resolve();

    console.log('🔧 ChatSendServerCommand.findCallerIdentity DETECTED', {
      uniqueId: identity.uniqueId,
      displayName: identity.displayName,
      type: identity.type,
      exists: identity.exists,
      agentName: identity.agentInfo.name
    });

    // If user exists in database, return it
    if (identity.exists && identity.userId) {
      const result = await DataList.execute<UserEntity>({
          collection: UserEntity.collection,
          filter: { id: identity.userId },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        const user = result.items[0];
        return { id: user.id, entity: user };
      }
    }

    // User doesn't exist - throw error with helpful message
    throw new Error(
      `Detected caller: ${identity.displayName} (${identity.uniqueId}) but user not found in database. ` +
      `Run seed script to create users.`
    );
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
        const mimeResult = await FileMimeType.execute({
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
        const fileResult = await FileLoad.execute({
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
