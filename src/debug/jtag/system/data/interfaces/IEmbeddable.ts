/**
 * IEmbeddable - Interface for entities that can be semantically embedded
 *
 * Core principle: Engram = Entity
 * Every entity in the system is a memory trace (engram) that can be:
 * - Stored (persisted to database)
 * - Embedded (semantic vector for meaning)
 * - Connected (relationships to other engrams)
 * - Recalled (semantic search finds relevant engrams)
 *
 * Implementing this interface gives any entity automatic semantic capabilities.
 */

import type { ISOString } from '../domains/CoreTypes';

/**
 * Interface for entities that support semantic embedding
 *
 * Implementations should return the text content that best represents
 * the semantic meaning of the entity for embedding purposes.
 */
export interface IEmbeddable {
  /**
   * Get the text content to embed for semantic search
   * This should return the most semantically meaningful text representation
   *
   * @example
   * // MemoryEntity
   * getEmbeddableContent(): string { return this.content; }
   *
   * // ChatMessageEntity
   * getEmbeddableContent(): string { return this.text; }
   *
   * // TaskEntity
   * getEmbeddableContent(): string { return `${this.title}: ${this.description}`; }
   */
  getEmbeddableContent(): string;

  /**
   * Cached vector embedding (if already computed)
   * 384 dimensions for all-minilm, 768 for nomic-embed-text, 1536 for OpenAI
   */
  embedding?: number[];

  /**
   * When the embedding was generated (for cache invalidation)
   */
  embeddedAt?: ISOString;

  /**
   * Which model generated the embedding (for compatibility checking)
   */
  embeddingModel?: string;
}

/**
 * Type guard to check if an entity implements IEmbeddable
 */
export function isEmbeddable(entity: unknown): entity is IEmbeddable {
  return (
    entity !== null &&
    typeof entity === 'object' &&
    'getEmbeddableContent' in entity &&
    typeof (entity as IEmbeddable).getEmbeddableContent === 'function'
  );
}

/**
 * Helper to check if an embeddable entity needs (re)embedding
 *
 * @param entity - Entity to check
 * @param maxAgeMs - Maximum age of embedding before considering stale (default: 7 days)
 */
export function needsEmbedding(entity: IEmbeddable, maxAgeMs: number = 7 * 24 * 60 * 60 * 1000): boolean {
  // No embedding at all
  if (!entity.embedding || entity.embedding.length === 0) {
    return true;
  }

  // Check if embedding is stale
  if (entity.embeddedAt) {
    const age = Date.now() - new Date(entity.embeddedAt).getTime();
    return age > maxAgeMs;
  }

  return false;
}
