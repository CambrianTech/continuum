/**
 * MediaArtifactSource — RAGSource for media artifacts (images, files, audio, video)
 *
 * Extracts media attachments from conversation messages and preprocesses them
 * for the target model's capabilities. Replaces inline artifact extraction in
 * ChatRAGBuilder with a proper, recipe-controllable RAG source.
 *
 * Architecture:
 * - Queries DB directly (parallel with ConversationHistorySource — can't share cache)
 * - Fixed 50-message scan window (media is sparse, cap prevents DB congestion)
 * - Vision preprocessing is LAZY — only describe images when a non-vision model requests
 * - Vision has 10s timeout to avoid blocking the RAG build (LLaVA can take 30s+)
 * - Descriptions are CACHED via VisionDescriptionService (content-addressed, in-flight dedup)
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

    // Scan a fixed window for media. We don't need hundreds of messages — media is sparse
    // and we only want the few most recent attachments. Cap at 50 to avoid DB congestion
    // when many personas query simultaneously.
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
   * Always queries DB directly — ConversationHistorySource cache uses a narrow
   * window (recent messages only), but media scanning needs a wider window
   * since images are sparse. Both sources run in parallel during compose,
   * so the cache may not be populated yet anyway.
   */
  private async extractArtifacts(roomId: string, maxMessages: number): Promise<{ artifacts: RAGArtifact[]; messageCount: number; source: string }> {
    const result = await ORM.query<ChatMessageEntity>({
      collection: ChatMessageEntity.collection,
      filter: { roomId },
      sort: [{ field: 'timestamp', direction: 'desc' }],
      limit: maxMessages
    }, 'default');

    if (!result.success || !result.data) return { artifacts: [], messageCount: 0, source: 'db' };
    const messages = result.data.map(record => record.data);
    const source = 'db';

    const artifacts: RAGArtifact[] = [];

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

    const processed: RAGArtifact[] = [];

    for (const artifact of artifacts) {
      if (artifact.type !== 'image' || !artifact.base64) {
        processed.push(artifact);
        continue;
      }

      // Already preprocessed (cached from previous call)
      if (artifact.preprocessed?.result) {
        processed.push(artifact);
        continue;
      }

      // Already has a human-provided description
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

      // Generate description via vision model (cached + deduped by VisionDescriptionService)
      // Use a 10s timeout — vision inference can take 30s+ and would kill the whole RAG build.
      // If description isn't ready, return artifact without it. Text-only models get the
      // metadata (filename, type) which is still useful context.
      try {
        const mimeType = (hasMediaMetadata(artifact) ? artifact.metadata.mimeType : undefined) ?? 'image/png';
        const VISION_TIMEOUT_MS = 10_000;
        const descriptionPromise = visionService.describeBase64(artifact.base64, mimeType, {
          maxLength: 500,
          detectText: true,
          preferredProvider: 'candle',
        });
        const timeoutPromise = new Promise<null>((resolve) =>
          setTimeout(() => resolve(null), VISION_TIMEOUT_MS)
        );
        const description = await Promise.race([descriptionPromise, timeoutPromise]);

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
          log.info('Vision description timed out, returning artifact without description');
          processed.push(artifact);
        }
      } catch (error) {
        log.error('Failed to describe image:', error);
        processed.push(artifact);
      }
    }

    return processed;
  }
}
