/**
 * MemoryConsolidationAdapter - Interface for memory consolidation strategies
 *
 * Adapters transform raw WorkingMemory thoughts into consolidated MemoryEntities
 * for long-term storage. Different adapters implement different strategies:
 *
 * - RawMemoryAdapter: Direct pass-through (no synthesis)
 * - SemanticCompressionAdapter: LLM-based synthesis of related thoughts
 * - EmbeddingAdapter: Generate vector embeddings for semantic search
 */

import type { WorkingMemoryEntry } from '../../../cognition/memory/InMemoryCognitionStorage';
import type { MemoryEntity } from '../../../MemoryTypes';
import type { UUID } from '../../../../../../core/types/CrossPlatformUUID';

/**
 * Context provided to adapters during consolidation
 */
export interface ConsolidationContext {
  personaId: UUID;
  personaName: string;
  sessionId: UUID;
  timestamp: Date;
}

/**
 * Result of consolidation including any metadata
 */
export interface ConsolidationResult {
  memories: MemoryEntity[];
  metadata?: {
    synthesisCount?: number;
    groupsCreated?: number;
    embeddingsGenerated?: number;
    [key: string]: any;
  };
}

/**
 * Abstract adapter for memory consolidation strategies
 */
export abstract class MemoryConsolidationAdapter {
  /**
   * Consolidate working memory thoughts into long-term memories
   *
   * @param thoughts - Raw thoughts from working memory
   * @param context - Consolidation context (persona, session, etc.)
   * @returns Consolidated memories ready for LTM storage
   */
  abstract consolidate(
    thoughts: WorkingMemoryEntry[],
    context: ConsolidationContext
  ): Promise<ConsolidationResult>;

  /**
   * Get adapter name for logging/debugging
   */
  abstract getName(): string;

  /**
   * Check if adapter supports embeddings
   */
  supportsEmbeddings(): boolean {
    return false;
  }

  /**
   * Check if adapter does synthesis (vs pass-through)
   */
  doesSynthesis(): boolean {
    return false;
  }
}
