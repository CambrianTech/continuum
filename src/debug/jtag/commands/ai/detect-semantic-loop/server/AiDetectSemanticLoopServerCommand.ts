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
import { RustEmbeddingClient } from '../../../../system/core/services/RustEmbeddingClient';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

import { DataList } from '../../../data/list/shared/DataListTypes';
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
const WARN_THRESHOLD = 0.80;   // Above this: WARN (raised from 0.75)
const BLOCK_THRESHOLD = 0.95;  // Above this: BLOCK (raised from 0.85 - more lenient)

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

  /**
   * Compute cosine similarity between two embedding vectors
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) {
      throw new Error(`Embedding dimension mismatch: ${a.length} vs ${b.length}`);
    }

    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    const denominator = Math.sqrt(normA) * Math.sqrt(normB);
    return denominator === 0 ? 0 : dotProduct / denominator;
  }

  /**
   * Fast text similarity using word n-gram Jaccard coefficient
   * This is a fast O(n) algorithm that doesn't require embeddings
   * Returns 0-1 similarity score (1 = identical)
   */
  private computeTextSimilarity(text1: string, text2: string): number {
    // Handle edge cases
    if (!text1 || !text2) return 0;
    if (text1 === text2) return 1.0;

    // Tokenize into words and create n-grams (unigrams + bigrams)
    const tokenize = (text: string): Set<string> => {
      const words = text.toLowerCase().split(/\s+/).filter(w => w.length > 0);
      const ngrams = new Set<string>();

      // Unigrams
      for (const word of words) {
        ngrams.add(word);
      }

      // Bigrams (pairs of consecutive words)
      for (let i = 0; i < words.length - 1; i++) {
        ngrams.add(`${words[i]} ${words[i + 1]}`);
      }

      return ngrams;
    };

    const set1 = tokenize(text1);
    const set2 = tokenize(text2);

    // Jaccard similarity = intersection / union
    let intersection = 0;
    for (const gram of set1) {
      if (set2.has(gram)) intersection++;
    }

    const union = set1.size + set2.size - intersection;
    return union === 0 ? 0 : intersection / union;
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

    // Generate embedding for the input message via Rust worker (fast, ~5ms)
    const client = RustEmbeddingClient.instance;
    if (!await client.isAvailable()) {
      // Can't check similarity without embedding - allow the message
      return createAiDetectSemanticLoopResultFromParams(params, {
        success: true,
        isLoop: false,
        maxSimilarity: 0,
        matches: [],
        recommendation: 'ALLOW',
        explanation: 'Rust embedding worker not available - allowing by default'
      });
    }

    let inputEmbedding: number[];
    try {
      inputEmbedding = await client.embed(params.messageText);
    } catch (error: any) {
      return createAiDetectSemanticLoopResultFromParams(params, {
        success: true,
        isLoop: false,
        maxSimilarity: 0,
        matches: [],
        recommendation: 'ALLOW',
        explanation: `Could not generate embedding: ${error.message} - allowing by default`
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
      const result = await DataList.execute({
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

    // Compare input against each recent message
    const matches: Array<{ messageId: string; similarity: number; timestamp: string; excerpt: string }> = [];
    let maxSimilarity = 0;
    const inputTextNormalized = params.messageText.toLowerCase().trim();

    for (const msg of recentMessages) {
      const msgText = this.getMessageText(msg.content);
      if (!msgText) continue;

      let similarity = 0;

      // Fast path: Check for exact/near-exact match first (very fast, no embeddings needed)
      const msgTextNormalized = msgText.toLowerCase().trim();
      const textSimilarity = this.computeTextSimilarity(inputTextNormalized, msgTextNormalized);

      // If texts are very similar (>90% Jaccard), skip embedding check - it's definitely a duplicate
      if (textSimilarity >= 0.90) {
        similarity = textSimilarity;
      } else if (inputEmbedding) {
        // Semantic path: Use embeddings only if we have input embedding and message has one
        let msgEmbedding = msg.embedding;
        if (!msgEmbedding || msgEmbedding.length === 0) {
          // Skip embedding generation for individual messages - too slow
          // Fall back to text similarity
          similarity = textSimilarity;
        } else {
          try {
            similarity = this.cosineSimilarity(inputEmbedding, msgEmbedding);
          } catch {
            // Incompatible embeddings - use text similarity
            similarity = textSimilarity;
          }
        }
      } else {
        // No input embedding available - use text similarity
        similarity = textSimilarity;
      }

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
