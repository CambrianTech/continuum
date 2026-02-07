# Rust ORM Architecture

## Overview

Unified data access layer where Rust handles all database operations. TypeScript becomes a thin IPC wrapper.

## Current State (Problems)

```
Code paths today (FRAGMENTED):
├── DataDaemon.query() → adapter.query() → SQLite
├── adapter.query() directly (dbHandle cases)
├── DatabaseHandleRegistry.getAdapter() → adapter.query()
└── ~30 files with direct DataDaemon.* calls (violations)
```

**Issues:**
- Mixed paths, no single entry point
- TS single-threaded, can't parallelize
- Violations bypass intended architecture
- Concurrency handled poorly

## Target State

```
All code → ORM.execute(params) → IPC → Rust ConnectionManager → SQLite
```

**Single entry point. Single boundary. Rust handles parallelism.**

## Scale Requirements

- 13 personas × 2-3 DBs each = ~30-40 persona databases
- Shared DBs: users, rooms, messages, etc.
- High concurrent query load during active cognition
- Lots of data movement

## Rust Architecture

### ConnectionManager

```rust
pub struct ConnectionManager {
    /// Pool per database file - lazy initialized
    pools: DashMap<PathBuf, Pool<SqliteConnection>>,

    /// Config
    max_pools: usize,           // LRU eviction after this
    conns_per_pool: usize,      // 2-3 for SQLite
    idle_timeout: Duration,     // Close idle pools
}

impl ConnectionManager {
    /// Single entry point for all queries
    pub async fn execute(&self, params: QueryParams) -> Result<Value> {
        let db_path = self.resolve_path(&params.db_handle)?;
        let pool = self.get_or_create_pool(&db_path).await?;
        let conn = pool.acquire().await?;

        match params.operation {
            Op::Query(q) => conn.query(q).await,
            Op::Create(r) => conn.insert(r).await,
            Op::Update(r) => conn.update(r).await,
            Op::Delete(id) => conn.delete(id).await,
            // ... all operations
        }
    }

    /// Lazy pool creation
    async fn get_or_create_pool(&self, path: &Path) -> Result<&Pool> {
        if let Some(pool) = self.pools.get(path) {
            return Ok(pool);
        }

        // LRU eviction if at capacity
        if self.pools.len() >= self.max_pools {
            self.evict_lru().await?;
        }

        // Create new pool
        let pool = Pool::builder()
            .max_size(self.conns_per_pool)
            .build(path)
            .await?;

        self.pools.insert(path.to_owned(), pool);
        Ok(self.pools.get(path).unwrap())
    }
}
```

### Pool Design (per DB file)

```
┌─────────────────────────────────────────────────────────────┐
│                    ConnectionManager                         │
│                                                              │
│  pools: HashMap<DbPath, Pool>                               │
│                                                              │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐              │
│  │  main.db   │ │ persona1/  │ │ persona2/  │   ...        │
│  │  Pool(3)   │ │ memory.db  │ │ memory.db  │              │
│  │ ┌──┬──┬──┐ │ │  Pool(2)   │ │  Pool(2)   │              │
│  │ │C1│C2│C3│ │ │ ┌──┬──┐   │ │ ┌──┬──┐   │              │
│  │ └──┴──┴──┘ │ │ │C1│C2│   │ │ │C1│C2│   │              │
│  └────────────┘ └─┴──┴──┴───┘ └─┴──┴──┴───┘              │
│                                                              │
│  40+ pools, 80-100 total connections                        │
│  Lazy init, LRU eviction, idle timeout                      │
└─────────────────────────────────────────────────────────────┘
```

**Why pool per DB:**
- SQLite locks are per-file
- Persona 1's queries don't block Persona 2
- Natural isolation

**Why small pools (2-3):**
- SQLite WAL: concurrent reads, serialized writes
- More connections = diminishing returns
- Memory efficiency

### IPC Design

Single Unix socket with multiplexed async requests:

```
TypeScript                          Rust
    │                                 │
    ├─── send(req1, id=1) ──────────►│
    ├─── send(req2, id=2) ──────────►│──► tokio::spawn(handle(req1))
    ├─── send(req3, id=3) ──────────►│──► tokio::spawn(handle(req2))
    │                                 │──► tokio::spawn(handle(req3))
    │◄─── response(id=2) ────────────┤
    │◄─── response(id=1) ────────────┤    (responses out of order)
    │◄─── response(id=3) ────────────┤
```

- Request ID tagging for response matching
- Non-blocking sends from TS
- Tokio spawns task per request
- Responses matched by ID

**Single socket is fine because:**
- IPC overhead (~0.1ms) << query time (~1-100ms)
- Tokio handles concurrent tasks
- Real parallelism in Rust, not socket

### Operations Supported

Must match TypeScript DataStorageAdapter + VectorSearchAdapter:

**CRUD:**
- create, read, update, delete
- query, queryWithJoin, count
- batch, batchDelete

**Schema:**
- ensureSchema, listCollections, collectionStats

**Maintenance:**
- truncate, clear, clearAll, cleanup

**Vector (critical for RAG):**
- vectorSearch
- generateEmbedding (→ fastembed)
- indexVector
- backfillVectors
- getVectorIndexStats

**Pagination:**
- openPaginatedQuery
- getNextPage
- closePaginatedQuery

## TypeScript Architecture

### Before (Current Mess)

```typescript
// 30+ files doing this:
await DataDaemon.query<T>({ collection, filter });
await DataDaemon.store(collection, entity);

// Commands doing this for dbHandle:
if (params.dbHandle) {
    const adapter = registry.getAdapter(params.dbHandle);
    result = await adapter.query(q);
} else {
    result = await DataDaemon.query(q);
}
```

### After (Clean)

```typescript
// Single ORM class - thin IPC wrapper
export class ORM {
    private static socket: IPCClient;

    static async execute<T>(params: {
        operation: 'query' | 'create' | 'update' | 'delete' | ...;
        collection: string;
        dbHandle?: string;
        data?: T;
        filter?: Filter;
    }): Promise<Result<T>> {
        return this.socket.send(params);
    }

    // Convenience methods
    static query<T>(q: Query): Promise<Result<T[]>> {
        return this.execute({ operation: 'query', ...q });
    }

    static create<T>(collection: string, data: T, dbHandle?: string): Promise<Result<T>> {
        return this.execute({ operation: 'create', collection, data, dbHandle });
    }

    // ... etc
}
```

### Migration Path

1. Create `ORM` class with same interface as DataDaemon static methods
2. Initially, ORM calls DataDaemon (TS-only, no Rust)
3. Fix violations one file at a time: `DataDaemon.query()` → `ORM.query()`
4. When all violations fixed, swap ORM internals to IPC → Rust
5. Remove old DataDaemon code

## File Changes Required

### Rust (New)

```
workers/continuum-core/src/
├── orm/
│   ├── mod.rs
│   ├── connection_manager.rs   ← Pool management
│   ├── adapter.rs              ← StorageAdapter trait
│   ├── sqlite.rs               ← SQLite implementation
│   ├── query.rs                ← Query building
│   ├── types.rs                ← Shared types (ts-rs)
│   └── vector.rs               ← Vector operations
└── modules/
    └── data.rs                 ← ServiceModule for data/*
```

### TypeScript (Modify)

```
daemons/data-daemon/
├── shared/
│   ├── ORM.ts                  ← NEW: Single entry point
│   └── DataDaemon.ts           ← Eventually deprecated
└── server/
    └── ORMRustClient.ts        ← NEW: IPC to Rust
```

### Violations to Fix (~30 files)

```
system/genome/fine-tuning/server/TrainingDatasetBuilder.ts
system/rag/builders/ChatRAGBuilder.ts
system/rag/builders/CodebaseRAGBuilder.ts
system/rag/sources/ConversationHistorySource.ts
system/rag/sources/PersonaIdentitySource.ts
system/rag/sources/SocialMediaRAGSource.ts
system/user/server/CallerDetector.ts
system/user/server/modules/cognitive/memory/PersonaMemory.ts
system/user/server/modules/PersonaAutonomousLoop.ts
system/user/server/modules/PersonaMessageEvaluator.ts
system/user/server/modules/PersonaResponseGenerator.ts
system/user/server/modules/PersonaTaskExecutor.ts
commands/data/list/server/DataListServerCommand.ts (dbHandle path)
... and more
```

## Implementation Order

### Phase 1: Rust ORM (Disconnected) ✅ COMPLETE
- [x] ConnectionManager with pool-per-db
- [x] All CRUD operations
- [x] Vector operations (integrate fastembed)
- [x] Unit tests with in-memory SQLite

### Phase 2: TypeScript ORM Wrapper ✅ COMPLETE
- [x] Create ORM.ts with same interface as DataDaemon
- [x] Initially delegates to DataDaemon (no behavior change)
- [x] Add feature flag: `FORCE_TYPESCRIPT_BACKEND`

### Phase 3: Fix Violations (Incremental) ✅ COMPLETE
- [x] Migrated 21+ files from DataDaemon.* to ORM.*
- [x] All persona modules now use ORM
- [x] All RAG/embedding code migrated
- [x] DataDaemon.jtagContext preserved for event context

### Phase 4: Wire Together ✅ COMPLETE
- [x] Implement ORMRustClient (IPC to /tmp/continuum-core.sock)
- [x] Flip `FORCE_TYPESCRIPT_BACKEND=false`
- [x] All collections now route to Rust DataModule
- [ ] Remove old DataDaemon code (Phase 5 cleanup)

### Phase 5: Cleanup (IN PROGRESS)
- [x] Removed dead DataDaemon fallback paths from ORM CRUD methods
- [x] Removed debug console.log spam from ORM.ts
- [x] Updated ORM header comments to reflect Rust-first architecture
- [ ] Move batch operations to Rust
- [ ] Move paginated queries to Rust
- [ ] Move vector operations to Rust
- [ ] Remove DataDaemon once all ops migrated
- [ ] Remove FORCE_TYPESCRIPT_BACKEND kill switch once stable

## Success Criteria

1. **Single entry point**: All data access through `ORM.execute()`
2. **No violations**: Zero direct DataDaemon/adapter calls
3. **Parallel**: 40 concurrent queries from different personas execute in parallel
4. **Fast**: P99 query latency < 50ms for simple queries
5. **Fallback**: Can switch back to TS-only via flag
