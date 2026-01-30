/**
 * RAG Query Fetch Command Types
 *
 * Fetches results from an open query at any position
 * Supports bidirectional navigation and random access
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { CodeSearchResult } from '../../query-open/shared/RagQueryOpenTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Parameters for fetching results from a query handle
 */
export interface RagQueryFetchParams extends CommandParams {
  // Query handle from query-open
  queryHandle: UUID;

  // Absolute positioning (takes precedence over direction)
  offset?: number;   // Jump to specific position (0-based)
  limit?: number;    // How many items to fetch (defaults to handle's pageSize)

  // OR: Relative positioning (if offset not provided)
  direction?: 'forward' | 'backward';  // Move relative to current position
  // When using direction, moves by 'limit' items in that direction
}

/**
 * Normalize and validate RAG query fetch parameters
 * Ensures type safety when params come from CLI (strings) or API (typed)
 */
export function normalizeRagQueryFetchParams(params: Partial<RagQueryFetchParams>): {
  offset?: number;
  limit?: number;
  direction?: 'forward' | 'backward';
} {
  // Parse offset (may be string from CLI)
  let offset: number | undefined;
  if (params.offset !== undefined) {
    offset = typeof params.offset === 'string'
      ? parseInt(params.offset, 10)
      : params.offset;

    if (isNaN(offset) || offset < 0) {
      throw new Error(`Invalid offset: must be >= 0, got ${params.offset}`);
    }
  }

  // Parse limit (may be string from CLI)
  let limit: number | undefined;
  if (params.limit !== undefined) {
    limit = typeof params.limit === 'string'
      ? parseInt(params.limit, 10)
      : params.limit;

    if (isNaN(limit) || limit < 1 || limit > 100) {
      throw new Error(`Invalid limit: must be between 1 and 100, got ${params.limit}`);
    }
  }

  // Validate direction
  const direction = params.direction;
  if (direction && direction !== 'forward' && direction !== 'backward') {
    throw new Error(`Invalid direction: must be 'forward' or 'backward', got ${direction}`);
  }

  return { offset, limit, direction };
}

/**
 * Result of fetching query results
 */
export interface RagQueryFetchResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  // Current position
  readonly offset: number;
  readonly limit: number;
  readonly totalMatches: number;

  // Results at this position (sorted by relevance)
  readonly results: CodeSearchResult[];

  // Navigation hints
  readonly hasMore: boolean;      // Can go forward
  readonly hasPrevious: boolean;  // Can go backward
}

/**
 * RagQueryFetch — Type-safe command executor
 *
 * Usage:
 *   import { RagQueryFetch } from '...shared/RagQueryFetchTypes';
 *   const result = await RagQueryFetch.execute({ ... });
 */
export const RagQueryFetch = {
  execute(params: CommandInput<RagQueryFetchParams>): Promise<RagQueryFetchResult> {
    return Commands.execute<RagQueryFetchParams, RagQueryFetchResult>('ai/rag/query-fetch', params as Partial<RagQueryFetchParams>);
  },
  commandName: 'ai/rag/query-fetch' as const,
} as const;
