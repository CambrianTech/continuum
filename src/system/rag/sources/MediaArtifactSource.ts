/**
 * MediaArtifactSource — RAGSource for media artifacts (images, files, audio, video)
 *
 * Extracts media attachments from conversation messages and preprocesses them
 * for the target model's capabilities. Replaces inline artifact extraction in
 * ChatRAGBuilder with a proper, recipe-controllable RAG source.
 *
 * Architecture:
 * - Cache-first: uses ConversationHistorySource cache, falls back to DB with 5s timeout
 * - 50-message scan window (media is sparse, cap prevents DB congestion)
 * - Vision preprocessing is LAZY — only describe images when a non-vision model requests
 * - Adaptive timeout: 90s when LLaVA pre-warm is in-flight, 10s otherwise
 * - Descriptions CACHED via VisionDescriptionService (content-addressed, in-flight dedup)
 * - Budget controls how many artifacts to include (not all 50 images in history)
 *
 * "So the blind can see" — text-only models get descriptions of images.
 * Vision models get raw base64. Everyone gets the representation their capabilities support.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { type RAGArtifact, type MediaArtifactMetadata, hasMediaMetadata } from '../shared/RAGTypes';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { VisionDescriptionService } from '../../vision/VisionDescriptionService';
import { ConversationHistorySource } from './ConversationHistorySource';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('MediaArtifactSource', 'rag');

/** Estimated tokens per image artifact (base64 is huge, but description is small) */
const TOKENS_PER_IMAGE_DESCRIPTION = 150;
const TOKENS_PER_IMAGE_BASE64 = 1000;

export class MediaArtifactSource implements RAGSource {
  readonly name = 'media-artifacts';
  readonly priority = 65;
  readonly defaultBudgetPercent = 5;

  isApplicable(context: RAGSourceContext): boolean {
    // Active when artifacts are requested (default true)
    return context.options.includeArtifacts !== false;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    // Scan window for media. Balance between finding images in chatty rooms and
    // IPC load (base64 images are ~1MB each, 10+ personas query simultaneously).
    // Cache-first path (from ConversationHistorySource) avoids the heavy DB query.
    const mediaScanLimit = 50;
    const extraction = await this.extractArtifacts(context.roomId, mediaScanLimit);

    if (extraction.artifacts.length === 0) {
      const loadTimeMs = performance.now() - startTime;
      log.info(`0 artifacts from ${extraction.messageCount} messages (source=${extraction.source}) in ${loadTimeMs.toFixed(1)}ms`);
      return {
        sourceName: this.name,
        tokenCount: 0,
        loadTimeMs,
        artifacts: [],
      };
    }

    // Budget: how many artifacts can we afford?
    const hasVision = context.options.modelCapabilities?.supportsImages === true;
    const tokensPerArtifact = hasVision ? TOKENS_PER_IMAGE_BASE64 : TOKENS_PER_IMAGE_DESCRIPTION;
    const maxArtifacts = Math.max(1, Math.floor(allocatedBudget / tokensPerArtifact));
    const budgetedArtifacts = extraction.artifacts.slice(-maxArtifacts); // Most recent first

    // Preprocess for model capabilities
    const processedArtifacts = await this.preprocessForModel(budgetedArtifacts, context);

    const tokenCount = processedArtifacts.reduce((sum, a) => {
      if (a.preprocessed?.result && typeof a.preprocessed.result === 'string') {
        return sum + Math.ceil(a.preprocessed.result.length / 4);
      }
      if (a.base64) return sum + TOKENS_PER_IMAGE_BASE64;
      return sum + TOKENS_PER_IMAGE_DESCRIPTION;
    }, 0);

    const loadTimeMs = performance.now() - startTime;
    log.info(`${extraction.artifacts.length} artifacts from ${extraction.messageCount} messages (source=${extraction.source}), budgeted=${budgetedArtifacts.length}, processed=${processedArtifacts.length} in ${loadTimeMs.toFixed(1)}ms`);

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs,
      artifacts: processedArtifacts,
    };
  }

  /**
   * Extract media artifacts from conversation history.
   *
   * Cache-first: reads from ConversationHistorySource's event-maintained cache
   * to avoid duplicate IPC queries (base64 payloads are massive). Falls back to
   * a DB query with 5s timeout when cache is empty or expired.
   */
  private async extractArtifacts(roomId: string, maxMessages: number): Promise<{ artifacts: RAGArtifact[]; messageCount: number; source: string }> {
    // Try ConversationHistorySource cache first — it's already loaded by RAG compose
    // and avoids a duplicate DB query (which is expensive with base64 image payloads).
    let messages: ChatMessageEntity[];
    let source: string;

    const cached = ConversationHistorySource.getCachedRawMessages(roomId);
    if (cached && cached.length > 0) {
      messages = cached.slice(0, maxMessages);
      source = 'cache';
    } else {
      // Cache miss — fall back to DB query with timeout.
      // Large base64 payloads can cause IPC congestion when 10+ personas query simultaneously.
      const QUERY_TIMEOUT_MS = 5_000;
      const queryPromise = ORM.query<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: maxMessages
      }, 'default');

      const result = await Promise.race([
        queryPromise,
        new Promise<null>((resolve) => setTimeout(() => resolve(null), QUERY_TIMEOUT_MS)),
      ]);

      if (!result || !result.success || !result.data) {
        if (!result) {
          log.warn(`Media scan query timed out after ${QUERY_TIMEOUT_MS}ms for room ${roomId}`);
        } else {
          log.warn(`Media scan query failed for room ${roomId}: ${result.error ?? 'no data'}`);
        }
        return { artifacts: [], messageCount: 0, source: 'db-fail' };
      }
      messages = result.data.map(record => record.data);
      source = 'db';
    }

    const artifacts: RAGArtifact[] = [];

    const withMedia = messages.filter(m => m.content?.media && m.content.media.length > 0);
    if (withMedia.length === 0) {
      log.debug(`No media in ${messages.length} messages (source=${source})`);
    }

    for (const msg of messages) {
      if (!msg.content?.media || msg.content.media.length === 0) continue;

      for (const mediaItem of msg.content.media) {
        const metadata: MediaArtifactMetadata = {
          messageId: msg.id,
          senderName: msg.senderName,
          timestamp: msg.timestamp instanceof Date
            ? msg.timestamp.getTime()
            : typeof msg.timestamp === 'string'
              ? new Date(msg.timestamp).getTime()
              : msg.timestamp as number,
          filename: mediaItem.filename,
          mimeType: mediaItem.mimeType ?? mediaItem.type,
          size: mediaItem.size,
        };
        artifacts.push({
          type: this.detectArtifactType(mediaItem),
          url: mediaItem.url,
          base64: mediaItem.base64,
          content: mediaItem.description ?? mediaItem.alt,
          metadata,
        });
      }
    }

    return { artifacts, messageCount: messages.length, source };
  }

  /**
   * Detect artifact type from media attachment MIME type.
   */
  private detectArtifactType(attachment: { mimeType?: string; type?: string }): RAGArtifact['type'] {
    const mimeType = attachment.mimeType ?? attachment.type ?? '';
    if (mimeType.startsWith('image/')) return 'image';
    if (mimeType.startsWith('audio/')) return 'audio';
    if (mimeType.startsWith('video/')) return 'video';
    return 'file';
  }

  /**
   * Preprocess artifacts for the target model's capabilities.
   *
   * "So the blind can see":
   * - Vision models get raw base64 (they can see)
   * - Text-only models get text descriptions (generated by VisionDescriptionService)
   *
   * Descriptions are content-addressed and cached — 11 personas sharing one image
   * = 1 LLaVA call, not 11.
   */
  private async preprocessForModel(
    artifacts: RAGArtifact[],
    context: RAGSourceContext
  ): Promise<RAGArtifact[]> {
    // Vision models see raw images — no preprocessing needed
    if (context.options.modelCapabilities?.supportsImages) {
      return artifacts;
    }

    const shouldPreprocess = context.options.preprocessImages ?? true;
    if (!shouldPreprocess) return artifacts;

    const imageArtifacts = artifacts.filter(a => a.type === 'image' && a.base64);
    if (imageArtifacts.length === 0) return artifacts;

    const visionService = VisionDescriptionService.getInstance();
    if (!visionService.isAvailable()) {
      log.debug('No vision model available for image descriptions');
      return artifacts;
    }

    log.info(`Preprocessing ${imageArtifacts.length} image(s) for non-vision model`);

    // Parallelize ALL vision requests with adaptive timeout.
    // Sequential 10s timeouts per image caused 60s compose times (6 images × 10s).
    // VisionDescriptionService deduplicates + caches, so parallel requests are cheap.
    //
    // Timeout strategy:
    // - If ANY image has an in-flight request (pre-warm already started LLaVA),
    //   use 90s — LLaVA on CPU takes 60-70s, worth waiting for the result
    // - Otherwise, use 10s as a safety valve for new/unknown images
    const hasInflight = imageArtifacts.some(a => a.base64 && visionService.descriptionStatus(a.base64) === 'inflight');
    const VISION_TIMEOUT_MS = hasInflight ? 90_000 : 10_000;
    if (hasInflight) {
      log.info(`In-flight vision request detected, using extended ${VISION_TIMEOUT_MS / 1000}s timeout`);
    }

    const descriptionPromises = new Map<number, Promise<{ description: string; responseTimeMs: number; provider: string; modelId: string } | null>>();

    for (let i = 0; i < artifacts.length; i++) {
      const artifact = artifacts[i];
      if (artifact.type !== 'image' || !artifact.base64) continue;
      if (artifact.preprocessed?.result) continue;
      if (artifact.content && artifact.content.length > 10) continue;

      const mimeType = (hasMediaMetadata(artifact) ? artifact.metadata.mimeType : undefined) ?? 'image/png';
      descriptionPromises.set(i, visionService.describeBase64(artifact.base64, mimeType, {
        maxLength: 500,
        detectText: true,
      }).catch((error) => {
        log.error('Failed to describe image:', error);
        return null;
      }));
    }

    // Wait for ALL descriptions in parallel with a single timeout
    let descriptions: Map<number, { description: string; responseTimeMs: number; provider: string; modelId: string } | null>;
    if (descriptionPromises.size > 0) {
      const entries = Array.from(descriptionPromises.entries());
      const keys = entries.map(([k]) => k);
      const promises = entries.map(([, v]) => v);

      const timeoutPromise = new Promise<null[]>((resolve) =>
        setTimeout(() => resolve(promises.map(() => null)), VISION_TIMEOUT_MS)
      );
      const results = await Promise.race([
        Promise.all(promises),
        timeoutPromise,
      ]);

      descriptions = new Map(keys.map((k, i) => [k, results[i]]));
    } else {
      descriptions = new Map();
    }

    // Build processed artifacts with descriptions
    const processed: RAGArtifact[] = [];

    for (let i = 0; i < artifacts.length; i++) {
      const artifact = artifacts[i];

      if (artifact.type !== 'image' || !artifact.base64) {
        processed.push(artifact);
        continue;
      }

      if (artifact.preprocessed?.result) {
        processed.push(artifact);
        continue;
      }

      if (artifact.content && artifact.content.length > 10) {
        processed.push({
          ...artifact,
          preprocessed: {
            type: 'image_description',
            result: artifact.content,
            confidence: 0.9,
            processingTime: 0,
            model: 'existing',
          },
        });
        continue;
      }

      const description = descriptions.get(i);
      if (description) {
        processed.push({
          ...artifact,
          content: description.description,
          preprocessed: {
            type: 'image_description',
            result: description.description,
            confidence: 0.85,
            processingTime: description.responseTimeMs,
            model: `${description.provider}/${description.modelId}`,
          },
        });
        log.info(`Described image (${description.responseTimeMs}ms) via ${description.modelId}`);
      } else {
        log.info('Vision description timed out or failed, returning artifact without description');
        processed.push(artifact);
      }
    }

    return processed;
  }
}
