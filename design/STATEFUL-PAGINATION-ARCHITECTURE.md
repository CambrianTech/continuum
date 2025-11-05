# Stateful Pagination Architecture

**Date:** 2025-10-08
**Status:** Design Complete - Implementation Pending
**Author:** Claude + Joel

## Problem Statement

Previous cursor-based pagination implementation had the client (widgets) managing cursor state, which led to:
- Complex cursor math in every widget
- Sorting/pagination logic duplicated across layers
- Bugs with cursor direction vs sort order coordination
- Client responsible for maintaining hasMore, totalCount, cursor values

## Solution: DataDaemon-Managed Query Handles

The DataDaemon maintains pagination state internally. Clients receive an opaque handle (UUID) and simply request "next page".

### Core Principles

1. **Stateful Server, Stateless Client** - DataDaemon keeps all pagination state
2. **Entity-Driven Configuration** - Each entity declares its own pagination defaults
3. **Opaque Handles** - Client receives UUID, doesn't see cursor implementation
4. **Simple Client API** - Open → Next → Next → Close

## Architecture

### Entity Configuration

Each entity declares its pagination preferences via static method:

```typescript
// system/data/entities/BaseEntity.ts
abstract class BaseEntity {
  static getPaginationConfig() {
    return {
      defaultSortField: 'createdAt',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 100,
      cursorField: 'createdAt'
    };
  }
}

// system/data/entities/ChatMessageEntity.ts
class ChatMessageEntity extends BaseEntity {
  static override getPaginationConfig() {
    return {
      defaultSortField: 'timestamp',      // Sort by timestamp
      defaultSortDirection: 'desc',        // Newest first
      defaultPageSize: 100,                // 100 messages per page
      cursorField: 'timestamp'             // Use timestamp for cursor
    };
  }
}
```

### DataDaemon Query Handle Management

```typescript
// daemons/data-daemon/shared/PaginatedQuery.ts

interface PaginatedQueryHandle {
  readonly queryId: UUID;           // Opaque handle
  readonly collection: string;
  readonly totalCount: number;      // Total matching records
  readonly pageSize: number;
  readonly hasMore: boolean;
}

interface PaginatedQueryState {
  readonly queryId: UUID;
  readonly collection: string;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize: number;
  readonly totalCount: number;
  currentPage: number;              // Internal: which page we're on
  currentCursor?: string;           // Internal: cursor position
  hasMore: boolean;                 // Internal: more pages available?
}

class PaginatedQueryManager {
  private queries: Map<UUID, PaginatedQueryState>;

  openQuery(params, totalCount): PaginatedQueryHandle
  getQueryState(queryId): PaginatedQueryState
  updateQueryState(queryId, cursor, hasMore): void
  closeQuery(queryId): void
}
```

### Command API

Three commands for pagination lifecycle:

#### 1. data/query-open

Opens a query and returns a handle.

```typescript
// commands/data/query-open/shared/QueryOpenTypes.ts

interface DataQueryOpenParams {
  readonly collection: string;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize?: number;  // Optional: uses entity default if not provided
}

interface DataQueryOpenResult {
  readonly success: boolean;
  readonly queryHandle: UUID;      // Opaque handle for subsequent calls
  readonly totalCount: number;     // Total records matching filter
  readonly pageSize: number;       // Actual page size being used
  readonly error?: string;
}
```

**Usage:**
```typescript
const result = await Commands.execute('data/query-open', {
  collection: 'chat_messages',
  filter: { roomId: '5e71a0c8-0303-4eb8-a478-3a121248', status: 'sent' }
  // orderBy and pageSize use ChatMessageEntity.getPaginationConfig() defaults
});

// Result:
// {
//   success: true,
//   queryHandle: 'uuid-here',
//   totalCount: 392,
//   pageSize: 100
// }
```

#### 2. data/query-next

Gets the next page of results.

```typescript
// commands/data/query-next/shared/QueryNextTypes.ts

interface DataQueryNextParams {
  readonly queryHandle: UUID;      // Handle from query-open
}

interface DataQueryNextResult<T extends BaseEntity> {
  readonly success: boolean;
  readonly items: readonly T[];    // Page of results
  readonly pageNumber: number;     // Which page (0-indexed)
  readonly hasMore: boolean;       // More pages available?
  readonly totalCount: number;     // Total matching records
  readonly error?: string;
}
```

**Usage:**
```typescript
// First page
const page1 = await Commands.execute('data/query-next', {
  queryHandle: handle.queryHandle
});
// Returns: { items: [...100 messages], pageNumber: 0, hasMore: true }

// Second page (DataDaemon maintains cursor internally)
const page2 = await Commands.execute('data/query-next', {
  queryHandle: handle.queryHandle
});
// Returns: { items: [...100 messages], pageNumber: 1, hasMore: true }
```

#### 3. data/query-close

Closes query and frees resources.

```typescript
// commands/data/query-close/shared/QueryCloseTypes.ts

interface DataQueryCloseParams {
  readonly queryHandle: UUID;
}

interface DataQueryCloseResult {
  readonly success: boolean;
  readonly error?: string;
}
```

**Usage:**
```typescript
await Commands.execute('data/query-close', {
  queryHandle: handle.queryHandle
});
```

## Widget Usage Pattern

### Before (Complex - Client Manages Cursors)

```typescript
class ChatWidget {
  private cursor?: string;
  private totalCount: number = 0;
  private loadedCount: number = 0;

  async load(cursor?: string, limit?: number) {
    const result = await Commands.execute('data/list', {
      collection: 'chat_messages',
      filter: { roomId: this.roomId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: limit ?? 100,
      ...(cursor && { cursor: {
        field: 'timestamp',
        value: cursor,
        direction: 'before'  // Complex: coordinate with sort direction!
      }})
    });

    // Complex cursor math
    const oldestMessage = result.items[result.items.length - 1];
    const nextCursor = oldestMessage?.timestamp?.toString();
    const hasMore = this.loadedCount < this.totalCount;

    return { items: result.items, hasMore, nextCursor };
  }
}
```

### After (Simple - DataDaemon Manages State)

```typescript
class ChatWidget {
  private queryHandle?: UUID;

  async openQuery() {
    const result = await Commands.execute('data/query-open', {
      collection: 'chat_messages',
      filter: { roomId: this.roomId }
      // That's it! Entity config provides sort/pageSize
    });

    this.queryHandle = result.queryHandle;
    this.totalCount = result.totalCount;
  }

  async loadMore() {
    if (!this.queryHandle) return;

    const page = await Commands.execute('data/query-next', {
      queryHandle: this.queryHandle
    });

    this.addItems(page.items);
    return page.hasMore;
  }

  async cleanup() {
    if (this.queryHandle) {
      await Commands.execute('data/query-close', {
        queryHandle: this.queryHandle
      });
    }
  }
}
```

## Implementation Plan

### Phase 1: Core Infrastructure ✅
- [x] Design PaginatedQuery types
- [x] Design PaginatedQueryManager
- [x] Design Command APIs (query-open, query-next, query-close)
- [x] Update BaseEntity.getPaginationConfig() signature
- [x] Update ChatMessageEntity.getPaginationConfig()

### Phase 2: DataDaemon Integration 🚧
- [ ] Add PaginatedQueryManager to DataDaemon
- [ ] Implement query-open command (server/browser)
- [ ] Implement query-next command (server/browser)
- [ ] Implement query-close command (server/browser)
- [ ] Update DataDaemon to use entity pagination config

### Phase 3: Storage Adapter 🚧
- [ ] Ensure JsonFileStorageAdapter cursor logic is correct
- [ ] Add logging for cursor application
- [ ] Test cursor with DESC sort order
- [ ] Verify no duplicates in pagination

### Phase 4: Widget Migration 🔜
- [ ] Update ChatWidget to use query handles
- [ ] Update EntityScroller to support query handle pattern
- [ ] Test infinite scroll with query handles
- [ ] Remove old cursor management code

### Phase 5: Testing & Documentation 🔜
- [ ] Integration tests for query handle lifecycle
- [ ] Test pagination with different sort orders
- [ ] Test with multiple concurrent queries
- [ ] Update widget documentation

## Benefits

### For Developers

✅ **Simpler Widget Code** - No cursor math, just call next-page
✅ **Type Safety** - Query handle is opaque UUID, can't misuse
✅ **Consistency** - All pagination uses same pattern
✅ **Entity-Driven** - Configuration lives with the entity

### For the System

✅ **Centralized Logic** - Sorting/pagination in DataDaemon
✅ **Resource Management** - Can track/cleanup active queries
✅ **Performance** - Can optimize query caching
✅ **Debugging** - All pagination state visible in one place

## Migration Strategy

### Backward Compatibility

Keep existing `data/list` command working during migration:
- Old widgets continue using `data/list` with manual cursors
- New widgets use `data/query-open` → `data/query-next` pattern
- Gradually migrate widgets one at a time
- Remove `data/list` cursor parameter once all widgets migrated

### Entity Configuration Migration

All entities should override `getPaginationConfig()`:
- **ChatMessageEntity** - Sort by timestamp DESC ✅
- **UserEntity** - Sort by name ASC (TODO)
- **RoomEntity** - Sort by lastActivityAt DESC (TODO)
- **ArtifactEntity** - Sort by createdAt DESC (TODO)

## Technical Notes

### Query Handle Lifecycle

```
Client                DataDaemon                Storage
  |                       |                        |
  |-- query-open -------->|                        |
  |                       |-- get count ---------->|
  |                       |<----- 392 -------------|
  |                       |-- create state ------->|
  |<-- handle(uuid) ------|    (queryId, cursor=null, page=0)
  |                       |                        |
  |-- query-next -------->|                        |
  |  (uuid)               |-- query with cursor -->|
  |                       |<----- items[0..99] ----|
  |                       |-- update state ------->|
  |<-- page 0 ------------|    (cursor=ts, page=1) |
  |                       |                        |
  |-- query-next -------->|                        |
  |  (uuid)               |-- query with cursor -->|
  |                       |<----- items[100..199] -|
  |<-- page 1 ------------|                        |
  |                       |                        |
  |-- query-close ------->|                        |
  |                       |-- delete state ------->|
  |<-- success -----------|                        |
```

### Cursor Position Management

DataDaemon maintains cursor based on entity's `cursorField`:

```typescript
// Page 1: Get first 100 messages (newest first, DESC order)
// Returns messages: timestamp 22:26 → 20:22 (oldest)
// Store cursor: "2025-10-08T20:22:52.475Z"

// Page 2: Query with cursor
// Filter: timestamp < "2025-10-08T20:22:52.475Z"
// Returns messages: timestamp 20:15 → 18:30 (older than cursor)
// Store cursor: "2025-10-08T18:30:00.000Z"

// Page 3: Query with new cursor
// Filter: timestamp < "2025-10-08T18:30:00.000Z"
// Returns messages: timestamp 18:20 → 16:45 (older than cursor)
```

### Resource Cleanup

Query handles should be cleaned up:
- **Explicit** - Widget calls `query-close` when done
- **Timeout** - DataDaemon auto-closes queries after 5 minutes inactive
- **Session End** - All queries closed when session disconnects

## Related Documents

- `ENTITY-SYSTEM-ARCHITECTURE.md` - Entity design patterns
- `DATA-DAEMON-ARCHITECTURE.md` - DataDaemon responsibilities
- `WIDGET-PAGINATION-PATTERNS.md` - Widget infinite scroll patterns

## Questions & Decisions

### Q: Should query handles expire?
**A:** Yes, auto-close after 5 minutes of inactivity to prevent memory leaks.

### Q: Can multiple clients share a query handle?
**A:** No, query handles are session-specific. Each client opens their own query.

### Q: What if a client requests next-page after reaching end?
**A:** Return empty items array with `hasMore: false`. Don't error.

### Q: Should DataDaemon cache query results?
**A:** Not in initial implementation. Can add later for performance.

### Q: How to handle concurrent writes during pagination?
**A:** Cursor-based pagination is stable - won't skip or duplicate records even if new records are inserted. New records appear in future queries.

## References

- **Cursor Pagination** - https://slack.engineering/evolving-api-pagination-at-slack/
- **Stateful Pagination** - https://www.postgresql.org/docs/current/sql-declare.html (SQL cursors)
- **Opaque Handles** - Rust's type system pattern for hiding implementation
