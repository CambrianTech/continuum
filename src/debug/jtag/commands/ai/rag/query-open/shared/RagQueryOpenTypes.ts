/**
 * RAG Query Open Command Types
 *
 * Opens a semantic similarity search query and returns a handle
 * Results are ranked by relevance score (cosine similarity)
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { CodeIndexEntity } from '../../../../../system/data/entities/CodeIndexEntity';

/**
 * Parameters for opening a RAG similarity search
 */
export interface RagQueryOpenParams extends CommandParams {
  // Search query (will be converted to embedding)
  query: string;

  // Optional: Use pre-computed embedding instead of generating from query
  queryEmbedding?: number[];

  // Optional: Which embedding model to use (defaults to nomic-embed-text)
  embeddingModel?: string;

  // Optional: Filter results by file type
  fileType?: 'typescript' | 'markdown' | 'javascript' | 'json';

  // Optional: Filter results by export type
  exportType?: string;

  // Optional: Minimum relevance score (0-1, cosine similarity)
  minRelevance?: number;

  // Pagination
  pageSize?: number;  // Default: 10
}

/**
 * Result of opening a RAG query
 * Returns handle and first page of results
 */
export interface RagQueryOpenResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  // Handle for pagination
  readonly queryHandle: UUID;

  // Query metadata
  readonly query: string;
  readonly embeddingModel: string;
  readonly totalMatches: number;  // Total results above minRelevance threshold

  // Current position in results
  readonly offset: number;  // Current position (0-based)
  readonly limit: number;   // Items per page

  // First page of results (sorted by relevance descending)
  readonly results: CodeSearchResult[];
  readonly hasMore: boolean;  // True if offset + limit < totalMatches
}

/**
 * Single search result with relevance score
 */
export interface CodeSearchResult {
  entry: CodeIndexEntity;
  relevanceScore: number;  // Cosine similarity: 0-1 (higher = more relevant)
}

/**
 * Normalize and validate RAG query parameters
 * Ensures type safety when params come from CLI (strings) or API (typed)
 */
export function normalizeRagQueryOpenParams(params: Partial<RagQueryOpenParams>): {
  pageSize: number;
  minRelevance: number;
  embeddingModel: string;
} {
  // Parse pageSize (may be string from CLI)
  const pageSize = typeof params.pageSize === 'string'
    ? parseInt(params.pageSize, 10)
    : (params.pageSize ?? 10);

  if (isNaN(pageSize) || pageSize < 1 || pageSize > 100) {
    throw new Error(`Invalid pageSize: must be between 1 and 100, got ${params.pageSize}`);
  }

  // Parse minRelevance (may be string from CLI)
  const minRelevance = typeof params.minRelevance === 'string'
    ? parseFloat(params.minRelevance)
    : (params.minRelevance ?? 0.0);

  if (isNaN(minRelevance) || minRelevance < 0 || minRelevance > 1) {
    throw new Error(`Invalid minRelevance: must be between 0 and 1, got ${params.minRelevance}`);
  }

  const embeddingModel = params.embeddingModel ?? 'nomic-embed-text';

  return { pageSize, minRelevance, embeddingModel };
}
