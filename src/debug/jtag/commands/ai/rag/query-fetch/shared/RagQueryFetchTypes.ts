/**
 * RAG Query Fetch Command Types
 *
 * Fetches results from an open query at any position
 * Supports bidirectional navigation and random access
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';
import type { CodeSearchResult } from '../../query-open/shared/RagQueryOpenTypes';

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
