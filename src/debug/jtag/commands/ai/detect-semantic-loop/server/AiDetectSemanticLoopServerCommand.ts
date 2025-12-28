/**
 * Ai Detect Semantic Loop Command - Server Implementation
 *
 * Detects if an AI's response is semantically too similar to recent messages, preventing repetitive loop behavior.
 * Uses embedding similarity to catch semantic loops (same meaning, different words).
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { ValidationError } from '../../../../system/core/types/ErrorTypes';
import type { AiDetectSemanticLoopParams, AiDetectSemanticLoopResult } from '../shared/AiDetectSemanticLoopTypes';
import { createAiDetectSemanticLoopResultFromParams } from '../shared/AiDetectSemanticLoopTypes';
import { EmbeddingService } from '../../../../system/core/services/EmbeddingService';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

// Raw message data from database (not the decorated entity class)
interface RawChatMessage {
  id: string;
  senderId: string;
  roomId: string;
  content: { text: string } | string;  // Could be JSON or string in DB
  timestamp: string;  // ISO string in DB
  embedding?: number[];  // May be stored inline
}

// Thresholds for recommendations
const WARN_THRESHOLD = 0.75;   // Above this: WARN
const BLOCK_THRESHOLD = 0.85;  // Above this: BLOCK

export class AiDetectSemanticLoopServerCommand extends CommandBase<AiDetectSemanticLoopParams, AiDetectSemanticLoopResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Ai Detect Semantic Loop', context, subpath, commander);
  }

  /**
   * Extract text content from message (handles both MessageContent object and raw string)
   */
  private getMessageText(content: { text: string } | string | undefined): string {
    if (!content) return '';
    if (typeof content === 'string') return content;
    return content.text || '';
  }

  async execute(params: AiDetectSemanticLoopParams): Promise<AiDetectSemanticLoopResult> {
    // Validate required parameters
    if (!params.messageText || params.messageText.trim() === '') {
      throw new ValidationError(
        'messageText',
        `Missing required parameter 'messageText'. Provide the message text to check for semantic similarity.`
      );
    }

    if (!params.personaId || params.personaId.trim() === '') {
      throw new ValidationError(
        'personaId',
        `Missing required parameter 'personaId'. Provide the UUID of the persona to check message history for.`
      );
    }

    // Apply defaults
    const lookbackCount = params.lookbackCount ?? 5;
    const similarityThreshold = params.similarityThreshold ?? 0.85;
    const timeWindowMinutes = params.timeWindowMinutes ?? 10;

    // Generate embedding for the input message
    const inputEmbedding = await EmbeddingService.embedText(params.messageText);
    if (!inputEmbedding) {
      // Can't check similarity without embedding - allow the message
      return createAiDetectSemanticLoopResultFromParams(params, {
        success: true,
        isLoop: false,
        maxSimilarity: 0,
        matches: [],
        recommendation: 'ALLOW',
        explanation: 'Could not generate embedding for message - allowing by default'
      });
    }

    // Calculate time window filter
    const cutoffTime = new Date(Date.now() - timeWindowMinutes * 60 * 1000).toISOString();

    // Query recent messages from this persona (senderId, not authorId)
    const filter: Record<string, unknown> = {
      senderId: params.personaId,
      timestamp: { $gte: cutoffTime }
    };

    // Add optional room filter
    if (params.roomId) {
      filter.roomId = params.roomId;
    }

    let recentMessages: RawChatMessage[];
    try {
      const result = await Commands.execute<DataListParams<BaseEntity>, DataListResult<BaseEntity>>('data/list', {
        collection: 'chat_messages',
        filter,
        limit: lookbackCount,
        orderBy: [{ field: 'timestamp', direction: 'desc' }]
      });

      // Cast items to our raw message type
      recentMessages = result.success ? (result.items || []) as unknown as RawChatMessage[] : [];
    } catch (error) {
      // If we can't fetch history, allow the message
      return createAiDetectSemanticLoopResultFromParams(params, {
        success: true,
        isLoop: false,
        maxSimilarity: 0,
        matches: [],
        recommendation: 'ALLOW',
        explanation: `Could not fetch message history: ${error instanceof Error ? error.message : 'Unknown error'}`
      });
    }

    // Compare input embedding against each recent message
    const matches: Array<{ messageId: string; similarity: number; timestamp: string; excerpt: string }> = [];
    let maxSimilarity = 0;

    for (const msg of recentMessages) {
      const msgText = this.getMessageText(msg.content);
      if (!msgText) continue;

      // Try to use stored embedding, otherwise generate on-the-fly
      let msgEmbedding = msg.embedding;
      if (!msgEmbedding || msgEmbedding.length === 0) {
        // Generate embedding for this message (slower but necessary)
        msgEmbedding = await EmbeddingService.embedText(msgText) || undefined;
        if (!msgEmbedding) continue;
      }

      try {
        const similarity = EmbeddingService.cosineSimilarity(inputEmbedding, msgEmbedding);

        if (similarity >= WARN_THRESHOLD) {
          const excerpt = msgText.slice(0, 100) + (msgText.length > 100 ? '...' : '');
          matches.push({
            messageId: msg.id,
            similarity: Math.round(similarity * 1000) / 1000, // Round to 3 decimals
            timestamp: msg.timestamp,
            excerpt
          });
        }

        if (similarity > maxSimilarity) {
          maxSimilarity = similarity;
        }
      } catch {
        // Skip messages with incompatible embeddings (different dimensions)
        continue;
      }
    }

    // Sort matches by similarity (highest first)
    matches.sort((a, b) => b.similarity - a.similarity);

    // Determine recommendation
    let recommendation: 'ALLOW' | 'WARN' | 'BLOCK';
    let explanation: string;
    const isLoop = maxSimilarity >= similarityThreshold;

    if (maxSimilarity >= BLOCK_THRESHOLD) {
      recommendation = 'BLOCK';
      explanation = `Message is ${Math.round(maxSimilarity * 100)}% similar to a recent message - likely a semantic loop. Consider rephrasing or adding new content.`;
    } else if (maxSimilarity >= WARN_THRESHOLD) {
      recommendation = 'WARN';
      explanation = `Message is ${Math.round(maxSimilarity * 100)}% similar to a recent message. May be repetitive.`;
    } else {
      recommendation = 'ALLOW';
      explanation = recentMessages.length === 0
        ? 'No recent messages found to compare against'
        : `Message is sufficiently different from recent messages (max similarity: ${Math.round(maxSimilarity * 100)}%)`;
    }

    return createAiDetectSemanticLoopResultFromParams(params, {
      success: true,
      isLoop,
      maxSimilarity: Math.round(maxSimilarity * 1000) / 1000,
      matches,
      recommendation,
      explanation
    });
  }
}
