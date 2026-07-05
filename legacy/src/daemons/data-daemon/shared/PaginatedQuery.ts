/**
 * Paginated Query - DataDaemon maintains pagination state
 *
 * Client opens a query handle with filters/sorting.
 * DataDaemon manages the cursor internally.
 * Client just calls nextPage() to get more data.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';

/**
 * Paginated query handle - opaque to client
 */
export interface PaginatedQueryHandle {
  readonly queryId: UUID;
  readonly collection: string;
  readonly totalCount: number;
  readonly pageSize: number;
  readonly hasMore: boolean;
}

/**
 * Query parameters for opening a paginated query
 */
export interface OpenPaginatedQueryParams {
  readonly collection: string;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize?: number;
}

/**
 * Page result from paginated query
 */
export interface PaginatedQueryPage<T extends BaseEntity> {
  readonly items: readonly T[];
  readonly pageNumber: number;
  readonly hasMore: boolean;
  readonly totalCount: number;
}

/**
 * Internal state for paginated query
 * DataDaemon keeps this in memory
 */
export interface PaginatedQueryState {
  readonly queryId: UUID;
  readonly collection: string;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize: number;
  readonly totalCount: number;
  currentPage: number;
  currentCursor?: any; // Internal cursor - native type for storage adapter comparison
  hasMore: boolean;
}

/**
 * Paginated Query Manager
 * Maintains active queries in memory
 */
export class PaginatedQueryManager {
  private queries: Map<UUID, PaginatedQueryState> = new Map();

  /**
   * Open a new paginated query
   * Returns a handle for fetching pages
   */
  openQuery(params: OpenPaginatedQueryParams, totalCount: number): PaginatedQueryHandle {
    const queryId = generateUUID();

    const state: PaginatedQueryState = {
      queryId,
      collection: params.collection,
      filter: params.filter,
      orderBy: params.orderBy,
      pageSize: params.pageSize ?? 100,
      totalCount,
      currentPage: 0,
      hasMore: totalCount > 0
    };

    this.queries.set(queryId, state);

    return {
      queryId,
      collection: params.collection,
      totalCount,
      pageSize: state.pageSize,
      hasMore: state.hasMore
    };
  }

  /**
   * Get query state for fetching next page
   */
  getQueryState(queryId: UUID): PaginatedQueryState | undefined {
    return this.queries.get(queryId);
  }

  /**
   * Update query state after fetching a page
   */
  updateQueryState(queryId: UUID, cursor: any, hasMore: boolean): void {
    const state = this.queries.get(queryId);
    if (state) {
      state.currentPage++;
      state.currentCursor = cursor;
      state.hasMore = hasMore;
    }
  }

  /**
   * Close query and free resources
   */
  closeQuery(queryId: UUID): void {
    this.queries.delete(queryId);
  }

  /**
   * Get all active queries (for debugging)
   */
  getActiveQueries(): UUID[] {
    return Array.from(this.queries.keys());
  }
}
