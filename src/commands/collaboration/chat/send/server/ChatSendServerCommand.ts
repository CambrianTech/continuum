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
import {
  AircChatDualWriteService,
  type AircChatDualWriteResult,
} from '@system/airc-chat/server/AircChatDualWriteService';
export class ChatSendServerCommand extends ChatSendCommand {

  constructor(
    context: JTAGContext,
    subpath: string,
    commander: ICommandDaemon,
    private readonly aircDualWrite: AircChatDualWriteService = new AircChatDualWriteService(),
  ) {
    super(context, subpath, commander);
  }

  protected async executeChatSend(params: ChatSendParams): Promise<ChatSendResult> {
    // viaBrowser: forward to browser command via WebSocket — fills the actual widget textarea and clicks Send
    // If browser isn't connected, fall back to direct DB send (don't hang for 60s)
    if (params.viaBrowser) {
      try {
        const timeoutMs = 5000; // 5s — if browser doesn't respond, fall back
        const result = await Promise.race([
          this.remoteExecute(params) as Promise<ChatSendResult>,
          new Promise<ChatSendResult>((_, reject) =>
            setTimeout(() => reject(new Error('viaBrowser timeout')), timeoutMs)
          )
        ]);
        if (result.success) return result;
        console.warn('⚠️ viaBrowser forwarding failed, falling back to direct send');
      } catch {
        console.warn('⚠️ viaBrowser unavailable, falling back to direct send');
      }
      // Clear viaBrowser flag so the direct path below doesn't recurse
      params = { ...params, viaBrowser: false };
    }

    // 1. Find room (single source of truth: RoutingService)
    const resolved = await resolveRoomIdentifier(params.room || 'general');
    if (!resolved) {
      throw new Error(`Room not found: ${params.room || 'general'}`);
    }

    // 2. Get sender — resolve identity from whoever initiated the command.
    // Priority: explicit senderId (if it resolves) > seeded human owner.
    // Skip system UUID (00000...) — sentinels/Academy run as SYSTEM but can't be a chat sender.
    // CLI and agent sessions inject session-scoped UUIDs in params.userId that are
    // NOT seeded users — attempting to find them throws. Fall back to the seeded
    // human owner instead so attribution lands on the actual person, not on an
    // ephemeral session ID. Caught by carl-install-smoke 2026-05-04 (PR #1038).
    const { isSystemUUID } = await import('@system/core/types/SystemScopes');
    const rawSenderId = params.senderId || params.userId;
    const senderId = rawSenderId && !isSystemUUID(rawSenderId as UUID) ? rawSenderId : undefined;
    const explicit = senderId ? await this.findUserByIdOrNull(senderId as UUID, params) : null;
    const sender = explicit ?? await this.findHumanOwnerOrFallback(params);

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

    // ── Pre-warm vision descriptions BEFORE externalize ────────────
    // Vision-description inference takes 60-70s (Qwen2-VL on M5
    // Pro). Kick it off NOW with the still-base64-resident
    // mediaItems so the description is cached by the time personas
    // build RAG context for the next turn. Fire-and-forget — doesn't
    // block this command.
    //
    // Order matters: this MUST run before externalize strips base64,
    // because MediaPrewarm captures `img.base64` from each item by
    // value at call time. After externalize, base64 is gone.
    this.prewarmVisionDescriptions(mediaItems);

    // ── Externalize SYNCHRONOUSLY before persisting ────────────────
    // Joel's directive 2026-04-21: "you CANNOT have images, audio, etc.
    // make it into a orm data column" — base64 must NEVER hit the DB,
    // not even transiently. Move bytes to disk via MediaBlobService
    // FIRST, get back blobHash + relative `/media/{hash}.{ext}` URL,
    // THEN persist the entity with refs only.
    //
    // The previous fire-and-forget pattern (post-data/create) created
    // a window where the DB row carried full base64 — and a long-lived
    // window when the externalize task lost. Synchronous closes both.
    //
    // Browser real-time rendering still works: `data:create` event
    // carries the URL ref + blobHash, browser fetches via the
    // /media/{hash}.{ext} HTTP route (already implemented). No more
    // bytes-in-events either.
    if (mediaItems.length > 0) {
      try {
        await MediaBlobService.externalize(mediaItems);
      } catch (err) {
        // Surface loudly — externalization is non-optional now. If it
        // fails the alternative is base64 in the DB, which is the
        // exact thing we're preventing. Better to fail the send and
        // let the caller see the error than silently degrade.
        throw new Error(
          `Failed to externalize media to blob storage: ${err instanceof Error ? err.message : String(err)}. ` +
          `Inline base64 in chat_messages is forbidden — see MediaBlobService.`
        );
      }
    }

    messageEntity.content = {
      text: params.message,
      media: mediaItems  // base64 stripped, blobHash + url present
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
    // data/create handles validation, storage, and event broadcast.
    // Media is already externalized — entity carries refs, not bytes.
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
    const airc = await this.publishToAirc(resolved.displayName, storedEntity);

    // 5. Pre-warm vision description cache for image media (fire-and-forget).
    // LLaVA takes 60-70s. Starting inference NOW means the description is cached
    // by the time personas build RAG context (~5-10s later for the NEXT message).
    // Without pre-warming, every persona's 10s timeout fires before LLaVA finishes.
    // (Description is read from cache by the persona path; we don't await here
    //  since chat-send shouldn't block on a 60s vision call.)
    this.prewarmVisionDescriptions(mediaItems);

    // 7. Generate short ID (last 6 chars of UUID - from BaseEntity.id)
    const shortId = storedEntity.id.slice(-6);

    // 8. No-listener warning (#980 Bug 8): if zero persona-users exist in
    // the system, the message is stored successfully but no AI will ever
    // respond to it. Carl's #980 caught this: chat-send returned success,
    // user typed "hello" + got nothing back, no signal anywhere that the
    // message had no listener. Cascade from seed-failure (Bug 3): no
    // personas seeded → agent/list returns []. Surface a clear "stored
    // but no listener" warning so the user knows to investigate.
    //
    // Cheap query: count how many persona-type users exist (limit 1 — we
    // only need to distinguish 0 vs ≥1). Non-blocking on the result
    // payload — message is still stored either way; this just adds a
    // warning string when listeners are absent.
    const personaCheck = await DataList.execute<UserEntity>({
      dbHandle: 'default',
      collection: UserEntity.collection,
      filter: { type: 'persona' },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId,
    });
    const hasListener = personaCheck.success && (personaCheck.items?.length ?? 0) > 0;
    const baseMessage = hasListener
      ? `Message sent to ${resolved.displayName} (#${shortId})`
      : `Message sent to ${resolved.displayName} (#${shortId}) ⚠️ No AI personas in system — message stored but won't get a reply. Check: ./jtag data/list --collection=users --filter='{"type":"persona"}'  (likely cascade from a failed seed; re-run: npm run data:seed)`;
    const successMessage = airc.ok
      ? baseMessage
      : `${baseMessage} ⚠️ AIRC dual-write failed: ${airc.publish.ok ? 'unknown error' : airc.publish.error}`;

    return transformPayload(params, {
      success: true,
      message: successMessage,
      messageEntity: storedEntity,
      shortId: shortId,
      roomId: resolved.id,
      airc: {
        ok: airc.ok,
        eventId: airc.publish.eventId,
        roomId: airc.publish.roomId as UUID,
        error: airc.publish.ok ? undefined : airc.publish.error,
      },
    });
  }

  private async publishToAirc(
    roomName: string,
    storedEntity: ChatMessageEntity,
  ): Promise<AircChatDualWriteResult> {
    return this.aircDualWrite.publishStoredChatMessage({
      roomName,
      storedMessage: storedEntity,
    });
  }

  /**
   * Find the seeded human owner (single-owner system), fall back to params.userId.
   * Used when no explicit senderId is provided — CLI and agent sessions should
   * attribute messages to the human, not to "@cli" or "Claude Code".
   */
  private async findHumanOwnerOrFallback(params: ChatSendParams): Promise<{ id: UUID; entity: UserEntity }> {
    // Try to find the seeded human owner
    const result = await DataList.execute<UserEntity>({
      dbHandle: 'default',
      collection: UserEntity.collection,
      filter: { type: 'human' },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId,
    });

    if (result.success && result.items && result.items.length > 0) {
      const owner = result.items[0];
      return { id: owner.id, entity: owner };
    }

    // No human owner seeded yet — try the session userId one more time.
    // If that's also missing, fail loudly with a clear message — chat without
    // any seeded user is broken state worth surfacing.
    const fallback = await this.findUserByIdOrNull(params.userId, params);
    if (fallback) return fallback;
    throw new Error(
      `No seeded human owner found and session userId ${params.userId} doesn't exist either. ` +
      `Seed appears broken — run 'npm run data:seed' or check orchestrator logs.`
    );
  }

  /**
   * Find user by ID, returning null if not found (no throw).
   * Callers compose with `?? fallback`.
   */
  private async findUserByIdOrNull(userId: UUID, params: ChatSendParams): Promise<{ id: UUID; entity: UserEntity } | null> {
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
    return null;
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
