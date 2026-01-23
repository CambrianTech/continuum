/**
 * Ai Context Search Command - Shared Types
 *
 * Semantic context navigation - search memories, messages, timeline across all entity types using cosine similarity via Rust embedding worker
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Common entity collections with semantic content
 * These are the defaults, but ANY collection with embeddings can be searched
 */
export type CommonEntityCollection =
  | 'chat_messages'
  | 'memories'
  | 'timeline_events'
  | 'tool_results'
  | 'decisions'
  | 'wall_documents'
  | 'code_index';

/**
 * Collection name - can be any string (any BaseEntity collection)
 */
export type CollectionName = CommonEntityCollection | string;

/**
 * Search result item
 */
export interface ContextSearchItem {
  /** Entity ID */
  readonly id: UUID;
  /** Collection/entity type */
  readonly collection: CollectionName;
  /** Content summary (truncated for display) */
  readonly content: string;
  /** Semantic similarity score (0-1) */
  readonly score: number;
  /** When this happened */
  readonly timestamp: string;
  /** Source context (room name, memory type, etc) */
  readonly source: string;
  /** Additional metadata */
  readonly metadata?: Record<string, unknown>;
}

/**
 * Ai Context Search Command Parameters
 */
export interface AiContextSearchParams extends CommandParams {
  // Natural language query - what you're looking for
  query: string;
  // Collections to search - any BaseEntity collection name (default: chat_messages, memories, timeline_events)
  collections?: CollectionName[];
  // Persona ID to scope search (default: current)
  personaId?: string;
  // Context/room ID to exclude (for cross-context search)
  excludeContextId?: string;
  // Max results (default: 10, max: 50)
  limit?: number;
  // Min cosine similarity threshold 0-1 (default: 0.5)
  minSimilarity?: number;
  // ISO timestamp - only search items after this time
  since?: string;
  // Search mode: semantic, keyword, or hybrid (default: semantic)
  mode?: string;
}

/**
 * Factory function for creating AiContextSearchParams
 */
export const createAiContextSearchParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Natural language query - what you're looking for
    query: string;
    // Collections to search - any BaseEntity collection name
    collections?: CollectionName[];
    // Persona ID to scope search (default: current)
    personaId?: string;
    // Context/room ID to exclude (for cross-context search)
    excludeContextId?: string;
    // Max results (default: 10, max: 50)
    limit?: number;
    // Min cosine similarity threshold 0-1 (default: 0.5)
    minSimilarity?: number;
    // ISO timestamp - only search items after this time
    since?: string;
    // Search mode: semantic, keyword, or hybrid (default: semantic)
    mode?: string;
  }
): AiContextSearchParams => createPayload(context, sessionId, {
  collections: data.collections ?? undefined,
  personaId: data.personaId ?? '',
  excludeContextId: data.excludeContextId ?? '',
  limit: data.limit ?? 0,
  minSimilarity: data.minSimilarity ?? 0,
  since: data.since ?? '',
  mode: data.mode ?? '',
  ...data
});

/**
 * Ai Context Search Command Result
 */
export interface AiContextSearchResult extends CommandResult {
  success: boolean;
  // Matching items ranked by semantic similarity
  items: ContextSearchItem[];
  // Total matches found
  totalMatches: number;
  // Search time in milliseconds
  durationMs: number;
  error?: JTAGError;
}

/**
 * Factory function for creating AiContextSearchResult with defaults
 */
export const createAiContextSearchResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Matching items ranked by semantic similarity
    items?: ContextSearchItem[];
    // Total matches found
    totalMatches?: number;
    // Search time in milliseconds
    durationMs?: number;
    error?: JTAGError;
  }
): AiContextSearchResult => createPayload(context, sessionId, {
  items: data.items ?? [],
  totalMatches: data.totalMatches ?? 0,
  durationMs: data.durationMs ?? 0,
  ...data
});

/**
 * Smart Ai Context Search-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiContextSearchResultFromParams = (
  params: AiContextSearchParams,
  differences: Omit<AiContextSearchResult, 'context' | 'sessionId'>
): AiContextSearchResult => transformPayload(params, differences);
