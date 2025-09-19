/**
 * Generic Data Loaders - Configurable Pagination Strategies
 *
 * Rust-like typed pagination with strategy pattern for different data access patterns.
 * Supports cursor-based, offset-based, and hybrid pagination with proper type safety.
 */

import type { BaseEntity } from '../../system/data/domains/CoreTypes';
import type { LoadFn, LoadResult } from './EntityScroller';

// =============================================================================
// PAGINATION STRATEGY TYPES
// =============================================================================

export type PaginationDirection = 'asc' | 'desc';
export type DisplayOrder = 'natural' | 'reverse';

export interface CursorConfig<T extends BaseEntity> {
  readonly field: keyof T;
  readonly direction: PaginationDirection;
  readonly displayOrder: DisplayOrder;
}

export interface DataLoaderConfig<T extends BaseEntity> {
  readonly collection: string;
  readonly filter?: Record<string, unknown>;
  readonly cursor: CursorConfig<T>;
  readonly defaultLimit: number;
}

// Clean protocol for data loading - elegant and firm
export interface DataExecutor<T extends BaseEntity> {
  execute(params: DataQueryParams): Promise<DataQueryResult<T>>;
}

// Simplified, elegant query parameters
export interface DataQueryParams {
  readonly collection: string;
  readonly filter?: Record<string, unknown>;
  readonly orderBy?: readonly OrderBySpec[];
  readonly limit: number;
  readonly cursor?: CursorSpec;
}

// Clean cursor specification
export interface CursorSpec {
  readonly field: string;
  readonly value: string;
  readonly direction: 'before' | 'after';
}

// Clean order specification
export interface OrderBySpec {
  readonly field: string;
  readonly direction: 'asc' | 'desc';
}

// Clean result protocol
export interface DataQueryResult<T extends BaseEntity> {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly totalCount: number;
}

// =============================================================================
// DATA LIST COMMAND TYPES (Server Contract) - Import from actual command types
// =============================================================================


// =============================================================================
// PAGINATION CONSTANTS AND VALIDATION
// =============================================================================

/** Database query result validation */
export const VALIDATION_CONSTANTS = {
  MIN_ITEMS_FOR_MORE: 1,
  DEFAULT_PAGE_SIZE: 20,
  MAX_PAGE_SIZE: 100
} as const;

/** Cursor field validation */
export const CURSOR_FIELDS = {
  TIMESTAMP: 'timestamp',
  NAME: 'name',
  LAST_ACTIVE: 'lastActiveAt'
} as const;

/** Database directions */
export const DB_DIRECTIONS = {
  ASC: 'asc',
  DESC: 'desc'
} as const;

/** Display order transformations */
export const DISPLAY_ORDERS = {
  NATURAL: 'natural',
  REVERSE: 'reverse'
} as const;


// =============================================================================
// GENERIC DATA LOADER FACTORY
// =============================================================================

export function createDataLoader<T extends BaseEntity>(
  executor: DataExecutor<T>,
  config: DataLoaderConfig<T>
): LoadFn<T> {
  return async (cursor?: string, limit?: number): Promise<LoadResult<T>> => {
    const actualLimit = limit ?? config.defaultLimit;

    console.log('🔧 DataLoader: Loading data', {
      collection: config.collection,
      cursor,
      limit: actualLimit,
      cursorField: config.cursor.field,
      direction: config.cursor.direction,
      displayOrder: config.cursor.displayOrder
    });

    // Build clean query parameters - elegant and type-safe
    const queryParams: DataQueryParams = {
      collection: config.collection,
      filter: config.filter,
      orderBy: [{
        field: config.cursor.field as string,
        direction: config.cursor.direction
      }],
      limit: actualLimit,
      ...(cursor && {
        cursor: {
          field: config.cursor.field as string,
          value: cursor,
          direction: config.cursor.direction === 'desc' ? 'before' : 'after'
        }
      })
    };

    // Execute with clean protocol - firm typing, no casting needed
    const result = await executor.execute(queryParams);

    console.log('🔧 DataLoader: DB result', {
      success: result.success,
      itemsCount: result.items.length,
      totalCount: result.totalCount,
      firstItem: result.items[0]?.[config.cursor.field],
      lastItem: result.items[result.items.length - 1]?.[config.cursor.field]
    });

    // Clean validation with firm typing
    if (!result.success || result.items.length === 0) {
      console.log('🔧 DataLoader: No items found', { success: result.success, itemsLength: result.items.length });
      return { items: [], hasMore: false };
    }

    // Type-safe items - already properly typed from clean protocol
    const rawItems: readonly T[] = result.items;

    // Apply display ordering if needed
    const displayItems: T[] = config.cursor.displayOrder === DISPLAY_ORDERS.REVERSE
      ? [...rawItems].reverse()
      : [...rawItems];

    // Calculate hasMore with correct logic based on total count
    const totalItemsInDB = result.totalCount;
    const currentItemsLoaded = rawItems.length;

    // We have more items if:
    // 1. The total count indicates more items exist than we've loaded so far, AND
    // 2. We got a full page (might be more)
    const hasMoreItems = (totalItemsInDB > currentItemsLoaded) && (currentItemsLoaded === actualLimit);

    // Calculate next cursor - always use the boundary item for pagination direction
    const boundaryItem = config.cursor.direction === DB_DIRECTIONS.DESC
      ? rawItems[rawItems.length - 1]  // Oldest item for DESC queries (for loading older messages)
      : rawItems[0];                   // Newest item for ASC queries (for loading newer messages)

    const nextCursor = boundaryItem?.[config.cursor.field] as string | undefined;

    console.log('🔧 DataLoader: Processed result', {
      displayItemsCount: displayItems.length,
      totalItemsInDB,
      currentItemsLoaded,
      hasMoreItems,
      boundaryItem: boundaryItem ? `${boundaryItem.id}:${boundaryItem[config.cursor.field]}` : 'none',
      nextCursor,
      firstDisplayItem: displayItems[0]?.[config.cursor.field],
      lastDisplayItem: displayItems[displayItems.length - 1]?.[config.cursor.field]
    });

    return {
      items: displayItems,
      hasMore: hasMoreItems,
      nextCursor
    };
  };
}

// =============================================================================
// COMMON PAGINATION PRESETS (Domain Agnostic)
// =============================================================================

export const PAGINATION_PRESETS = {
  /** Chat messages - newest from DB, reverse display (oldest at top, newest at bottom) */
  CHAT_MESSAGES: {
    field: CURSOR_FIELDS.TIMESTAMP,
    direction: DB_DIRECTIONS.DESC,
    displayOrder: DISPLAY_ORDERS.REVERSE
  },

  /** Recent items - newest first, natural display order (e.g., recent activities) */
  NEWEST_FIRST: {
    field: CURSOR_FIELDS.TIMESTAMP,
    direction: DB_DIRECTIONS.DESC,
    displayOrder: DISPLAY_ORDERS.NATURAL
  },

  /** Oldest first, chronological display (e.g., timeline views) */
  CHRONOLOGICAL: {
    field: CURSOR_FIELDS.TIMESTAMP,
    direction: DB_DIRECTIONS.ASC,
    displayOrder: DISPLAY_ORDERS.NATURAL
  },

  /** Alphabetical by name (e.g., user lists, file lists) */
  ALPHABETICAL: {
    field: CURSOR_FIELDS.NAME,
    direction: DB_DIRECTIONS.ASC,
    displayOrder: DISPLAY_ORDERS.NATURAL
  },

  /** Most recent activity first (e.g., last modified files) */
  RECENT_ACTIVITY: {
    field: CURSOR_FIELDS.LAST_ACTIVE,
    direction: DB_DIRECTIONS.DESC,
    displayOrder: DISPLAY_ORDERS.NATURAL
  }
} as const;