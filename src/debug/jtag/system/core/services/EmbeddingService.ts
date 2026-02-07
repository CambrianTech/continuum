/**
 * EmbeddingService - Generic embedding service for any IEmbeddable entity
 *
 * This service provides a unified way to generate embeddings for any entity
 * that implements IEmbeddable. It handles:
 * - Lazy embedding (only generate if not already present)
 * - Batch embedding for efficiency
 * - Model selection (Ollama local or OpenAI)
 * - Error handling with graceful degradation
 *
 * Usage:
 *   const embeddedMemory = await EmbeddingService.embedIfNeeded(memory);
 *   const embeddedBatch = await EmbeddingService.embedBatch(memories);
 */

import { ORM } from '../../../daemons/data-daemon/shared/ORM';
import type { EmbeddingModel } from '../../../daemons/data-daemon/shared/VectorSearchTypes';
import { DEFAULT_EMBEDDING_MODELS, toNumberArray } from '../../../daemons/data-daemon/shared/VectorSearchTypes';
import type { IEmbeddable } from '../../data/interfaces/IEmbeddable';
import { needsEmbedding } from '../../data/interfaces/IEmbeddable';
import { ISOString } from '../../data/domains/CoreTypes';

/**
 * Default embedding model - all-minilm via Ollama
 * 384 dimensions, fast local inference, no API costs
 */
export const DEFAULT_EMBEDDING_MODEL: EmbeddingModel = DEFAULT_EMBEDDING_MODELS['all-minilm'];

/**
 * Embedding generation options
 */
export interface EmbedOptions {
  /** Override default embedding model */
  model?: EmbeddingModel;

  /** Force re-embedding even if already present */
  force?: boolean;

  /** Maximum age before considering embedding stale (ms) */
  maxAgeMs?: number;

  /** Log function for debugging */
  log?: (message: string) => void;
}

/**
 * Result of embedding operation
 */
export interface EmbedResult<T extends IEmbeddable> {
  entity: T;
  success: boolean;
  error?: string;
  generationTimeMs?: number;
}

/**
 * Batch embedding result
 */
export interface BatchEmbedResult<T extends IEmbeddable> {
  results: EmbedResult<T>[];
  successCount: number;
  failureCount: number;
  totalTimeMs: number;
}

/**
 * EmbeddingService - Stateless service for embedding IEmbeddable entities
 */
export class EmbeddingService {
  /**
   * Generate embedding for an entity if needed
   *
   * This is the primary method - it checks if embedding exists and is fresh,
   * then generates one if needed.
   *
   * @param entity - Entity implementing IEmbeddable
   * @param options - Embedding options
   * @returns The entity with embedding populated (mutates original)
   */
  static async embedIfNeeded<T extends IEmbeddable>(
    entity: T,
    options: EmbedOptions = {}
  ): Promise<T> {
    const model = options.model || DEFAULT_EMBEDDING_MODEL;
    const log = options.log || (() => {});

    // Check if we need to embed
    if (!options.force && !needsEmbedding(entity, options.maxAgeMs)) {
      log(`Embedding already present for entity, skipping`);
      return entity;
    }

    // Get content to embed
    const content = entity.getEmbeddableContent();
    if (!content || content.trim().length === 0) {
      log(`Empty content for embedding, skipping`);
      return entity;
    }

    // Generate embedding via DataDaemon
    const startTime = Date.now();
    try {
      const result = await ORM.generateEmbedding({
        text: content,
        model
      });

      if (result.success && result.data) {
        // Convert to number[] for entity storage (Float32Array used internally for search)
        entity.embedding = toNumberArray(result.data.embedding);
        entity.embeddedAt = ISOString(new Date().toISOString());
        entity.embeddingModel = model.name;

        log(`Generated ${model.name} embedding (${result.data.embedding.length} dims) in ${Date.now() - startTime}ms`);
      } else {
        log(`Embedding generation failed: ${result.error}`);
      }
    } catch (error) {
      log(`Embedding generation error: ${error}`);
      // Don't throw - graceful degradation, entity just won't have embedding
    }

    return entity;
  }

  /**
   * Generate embeddings for a batch of entities
   *
   * Processes entities in parallel for efficiency.
   * Failed embeddings don't stop the batch - results include success/failure status.
   *
   * @param entities - Array of entities implementing IEmbeddable
   * @param options - Embedding options
   * @returns Batch result with individual entity results
   */
  static async embedBatch<T extends IEmbeddable>(
    entities: T[],
    options: EmbedOptions = {}
  ): Promise<BatchEmbedResult<T>> {
    const startTime = Date.now();
    const log = options.log || (() => {});

    log(`Starting batch embedding of ${entities.length} entities`);

    // Process all in parallel
    const resultPromises = entities.map(async (entity): Promise<EmbedResult<T>> => {
      const entityStart = Date.now();
      try {
        await this.embedIfNeeded(entity, options);
        return {
          entity,
          success: !!entity.embedding,
          generationTimeMs: Date.now() - entityStart
        };
      } catch (error) {
        return {
          entity,
          success: false,
          error: error instanceof Error ? error.message : String(error),
          generationTimeMs: Date.now() - entityStart
        };
      }
    });

    const results = await Promise.all(resultPromises);

    const successCount = results.filter(r => r.success).length;
    const failureCount = results.filter(r => !r.success).length;
    const totalTimeMs = Date.now() - startTime;

    log(`Batch complete: ${successCount} success, ${failureCount} failed, ${totalTimeMs}ms total`);

    return {
      results,
      successCount,
      failureCount,
      totalTimeMs
    };
  }

  /**
   * Generate embedding for raw text (not tied to an entity)
   *
   * Useful for generating query embeddings for semantic search.
   *
   * @param text - Text to embed
   * @param model - Embedding model (default: all-minilm)
   * @returns Vector embedding or null on failure
   */
  static async embedText(
    text: string,
    model: EmbeddingModel = DEFAULT_EMBEDDING_MODEL
  ): Promise<number[] | null> {
    if (!text || text.trim().length === 0) {
      console.warn('⚠️ EmbeddingService.embedText: Empty text provided');
      return null;
    }

    try {
      const result = await ORM.generateEmbedding({ text, model });
      if (!result.success) {
        console.warn(`⚠️ EmbeddingService.embedText: ORM.generateEmbedding failed: ${result.error}`);
      }
      // Convert to number[] for public API (Float32Array used internally for search)
      return result.success && result.data ? toNumberArray(result.data.embedding) : null;
    } catch (error) {
      // DON'T silently swallow errors - log them for debugging
      console.error('❌ EmbeddingService.embedText: Exception:', error instanceof Error ? error.message : error);
      return null;
    }
  }

  /**
   * Calculate cosine similarity between two embeddings
   *
   * @param a - First embedding
   * @param b - Second embedding
   * @returns Similarity score 0-1 (1 = identical)
   */
  static cosineSimilarity(a: number[], b: number[]): number {
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
}
