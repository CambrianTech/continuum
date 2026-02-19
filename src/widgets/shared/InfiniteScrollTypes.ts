/**
 * Generic Infinite Scroll Types
 *
 * Reusable interfaces for infinite scroll functionality across any widget
 * Extracted from ChatWidget's proven implementation with alignment support
 */

import type { AlignmentStrategy, AlignmentContext } from './AlignmentTypes';

/**
 * Generic infinite scroll configuration
 */
export interface InfiniteScrollConfig {
  readonly pageSize: number;
  readonly threshold: number;
  readonly rootMargin: string;
  readonly enabled: boolean;
}

/**
 * Generic pagination state for any cursor type
 */
export interface PaginationState<TCursor = string> {
  readonly hasMore: boolean;
  readonly isLoading: boolean;
  readonly oldestCursor?: TCursor;
  readonly newestCursor?: TCursor;
}

/**
 * Generic load result for any item type
 */
export interface LoadResult<TItem = any> {
  readonly items: ReadonlyArray<TItem>;
  readonly hasMore: boolean;
  readonly cursor?: string;
}

/**
 * Generic infinite scroll callbacks
 */
export interface InfiniteScrollCallbacks<TItem = any, TCursor = string> {
  /**
   * Load items with cursor-based pagination
   */
  readonly loadItems: (cursor?: TCursor, pageSize?: number) => Promise<LoadResult<TItem>>;

  /**
   * Extract cursor from item for pagination
   */
  readonly getCursor: (item: TItem) => TCursor;

  /**
   * Compare cursors for sorting (return negative if a < b, positive if a > b)
   */
  readonly compareCursors: (a: TCursor, b: TCursor) => number;

  /**
   * Create DOM element for item with alignment support
   */
  readonly createItemElement: (item: TItem, alignmentContext?: AlignmentContext) => HTMLElement;

  /**
   * Optional: Strategy for determining item alignment
   */
  readonly alignmentStrategy?: AlignmentStrategy<TItem>;
}

/**
 * Default configuration values
 */
export const DEFAULT_INFINITE_SCROLL_CONFIG: InfiniteScrollConfig = {
  pageSize: 20,
  threshold: 0.1,
  rootMargin: '50px',
  enabled: true
} as const;