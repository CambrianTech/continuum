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
import { resolveRoomIdentifier } from '@system/routing/RoutingService';

import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';
import { DataUpdate } from '../../../../data/update/shared/DataUpdateTypes';
import { FileMimeType } from '../../../../file/mime-type/shared/FileMimeTypeTypes';
import { FileLoad } from '../../../../file/load/shared/FileLoadTypes';
import { MediaPrewarm } from '../../../../media/prewarm/shared/MediaPrewarmTypes';
import { MediaBlobService } from '@system/storage/MediaBlobService';
export class ChatSendServerCommand extends ChatSendCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    // 1. Find room (single source of truth: RoutingService)
    const resolved = await resolveRoomIdentifier(params.room || 'general');
    if (!resolved) {
      throw new Error(`Room not found: ${params.room || 'general'}`);
    }

    // 2. Get sender — explicit senderId (CLI) or params.userId (auto-injected by infrastructure)
    const sender = await this.findUserById(params.senderId || params.userId, params);

    // 3. Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = resolved.id;  // From RoutingService resolution
    messageEntity.senderId = sender.id;  // sender is also DataRecord with .id
    messageEntity.senderName = sender.entity.displayName;
    messageEntity.senderType = sender.entity.type;

    // Process media: browser sends pre-encoded mediaItems, CLI sends file paths
    let mediaItems: MediaItem[] = [];
    if (params.mediaItems && params.mediaItems.length > 0) {
      // Browser path: pre-encoded base64 MediaItems from drag-and-drop
      mediaItems = params.mediaItems;
    } else if (params.media) {
      // CLI path: file paths that need loading + MIME detection
      const mediaPaths = Array.isArray(params.media) ? params.media : [params.media];
      mediaItems = await this.processMediaPaths(mediaPaths, params.context, params.sessionId);
    }

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
        dbHandle: 'default',
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

    // 5. Pre-warm vision description cache for image media (fire-and-forget).
    // LLaVA takes 60-70s. Starting inference NOW means the description is cached
    // by the time personas build RAG context (~5-10s later for the NEXT message).
    // Without pre-warming, every persona's 10s timeout fires before LLaVA finishes.
    this.prewarmVisionDescriptions(mediaItems);

    // 6. Externalize media to blob storage (fire-and-forget).
    // The data/create event already fired with full base64 for real-time rendering.
    // This updates the stored record to use blobHash + URL, clearing inline base64.
    this.externalizeMedia(storedEntity, params);

    // 7. Generate short ID (last 6 chars of UUID - from BaseEntity.id)
    const shortId = storedEntity.id.slice(-6);

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
        dbHandle: 'default',
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
   * Process media file paths into MediaItem objects
   * Uses file/mime-type and file/load commands (clean composition)
   */
  private async processMediaPaths(mediaPaths: string[], context: JTAGContext, sessionId: UUID): Promise<MediaItem[]> {
    const mediaItems: MediaItem[] = [];

    for (const filePath of mediaPaths) {
      try {
        // Step 1: Detect MIME type using file/mime-type command
        const mimeResult = await FileMimeType.execute({
          filepath: filePath,
          context,
          sessionId
        });

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

    return mediaItems;
  }

  /**
   * Fire-and-forget vision description generation for image media.
   * Calls media/prewarm command which populates VisionDescriptionService cache
   * so that when personas build RAG context seconds later, descriptions are cached.
   */
  private prewarmVisionDescriptions(mediaItems: MediaItem[]): void {
    const images = mediaItems.filter(m => m.type === 'image' && m.base64);
    if (images.length === 0) return;

    // Fire-and-forget — don't await, don't block chat/send response
    MediaPrewarm.execute({
      images: images.map(img => ({
        base64: img.base64!,
        mimeType: img.mimeType ?? 'image/png',
      })),
    }).catch(() => {
      // Best-effort pre-warming — swallow errors
    });
  }

  /**
   * Fire-and-forget: externalize media to content-addressed blob storage.
   *
   * After data/create has stored the entity AND emitted the event (with full
   * base64 for real-time browser rendering), we:
   * 1. Write base64 → binary file on disk via MediaBlobService
   * 2. Update the stored record: set blobHash + url, clear base64
   *
   * Historical loads serve media via /media/{hash}.{ext} HTTP route.
   */
  private externalizeMedia(storedEntity: ChatMessageEntity, params: ChatSendParams): void {
    const media = storedEntity.content?.media;
    if (!media || media.length === 0) return;

    // Only externalize items that have base64 data
    const hasBase64 = media.some(m => m.base64 && m.base64.length >= 5000);
    if (!hasBase64) return;

    // Fire-and-forget — don't await, don't block chat/send response
    (async () => {
      // Make mutable copies of the media items for externalization
      const mutableMedia: MediaItem[] = media.map(m => ({ ...m }));
      const stored = await MediaBlobService.externalize(mutableMedia);

      if (stored.length === 0) return;

      // Update the stored entity to use blob references instead of inline base64.
      // Suppress events — this is a storage optimization, not a content change.
      await DataUpdate.execute<ChatMessageEntity>({
        dbHandle: 'default',
        collection: ChatMessageEntity.collection,
        id: storedEntity.id,
        suppressEvents: true,
        data: {
          content: {
            text: storedEntity.content.text,
            media: mutableMedia,
          }
        } as Partial<ChatMessageEntity>,
        context: params.context,
        sessionId: params.sessionId,
      });
    })().catch(() => {
      // Best-effort externalization — inline base64 remains in DB as fallback
    });
  }
}
