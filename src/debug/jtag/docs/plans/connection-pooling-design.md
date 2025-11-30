# Connection Pooling Design - Phase 1

**Status**: Design proposal - awaiting user confirmation

**Goal**: Implement connection pooling for SQLite to prevent query blocking and improve throughput by 20-30%.

---

## Problem Statement

Currently, `SqliteStorageAdapter` maintains a single `sqlite3.Database` connection:
```typescript
private db: sqlite3.Database | null = null;
```

**Issues**:
1. **Blocking**: All queries execute sequentially on one connection
2. **Query Queuing**: Concurrent requests wait for connection availability
3. **Throughput**: Single connection = single thread of execution

**From profiling** (docs/plans/bottleneck-removal.md):
- 3.2GB memory usage
- 1312 samples in RowToJS → MigrateToMap
- Main thread blocking during query execution

---

## Architectural Decision

**User Guidance** (from conversation):
> "the idea was that the datadaemon does this, and you can still use the adapter, but you want a protected abstract or even pure (if it reduces burden)"

**Interpretation**:
1. **DataDaemon** owns and manages the connection pool
2. **SqliteStorageAdapter** exposes protected abstract/pure methods for getting/releasing connections
3. **Minimal changes** to adapter implementation (reduce burden)

---

## Proposed Architecture

### Option A: DataDaemon-Managed Pool (User's Preference)

```
DataDaemonServer
  └── SqliteConnectionPool (new class)
      ├── Manages N connections to DATABASE_PATHS.SQLITE
      ├── LRU eviction for idle connections
      ├── Health checks
      └── Connection acquisition/release

SqliteStorageAdapter
  ├── No longer stores `this.db`
  ├── Receives connection via protected abstract method
  └── Returns connection after use
```

**Flow**:
```typescript
// SqliteStorageAdapter.ts
protected abstract acquireConnection(): Promise<sqlite3.Database>;
protected abstract releaseConnection(db: sqlite3.Database): void;

async query<T>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
  const db = await this.acquireConnection();  // Get from pool
  try {
    const result = await this.executeQuery(db, query);
    return result;
  } finally {
    this.releaseConnection(db);  // Return to pool
  }
}
```

**DataDaemonServer implements**:
```typescript
// DataDaemonServer.ts
private connectionPool: SqliteConnectionPool;

constructor() {
  this.connectionPool = new SqliteConnectionPool({
    path: DATABASE_PATHS.SQLITE,
    poolSize: 5,  // From SqliteConfig
    idleTimeout: 60000,
    healthCheckInterval: 30000
  });

  this.dataDaemon = new DataDaemon(storageConfig, adapter);

  // Inject pool methods into adapter
  adapter.setConnectionPool(
    () => this.connectionPool.acquire(),
    (db) => this.connectionPool.release(db)
  );
}
```

**Benefits**:
- DataDaemon owns pool lifecycle
- SqliteStorageAdapter remains storage-adapter-agnostic (just needs connection)
- Pool can be shared across multiple adapters (if needed)
- Minimal changes to adapter logic

**Drawbacks**:
- Requires dependency injection pattern
- Adapter needs setter for pool methods (breaks pure abstract pattern slightly)

---

### Option B: Adapter-Internal Pool (Simpler Implementation)

```
SqliteStorageAdapter
  └── SqliteConnectionPool (internal)
      ├── Created during initialize()
      ├── Manages connections
      └── Cleaned up during close()
```

**Benefits**:
- Self-contained (no dependency injection)
- Easier to implement
- Each adapter has own pool

**Drawbacks**:
- Pool lifecycle tied to adapter (not DataDaemon)
- Doesn't match user's architectural guidance
- Can't share pool across adapters

---

## Recommended Approach: Option A (DataDaemon-Managed)

Following user's architectural guidance and principle of separation of concerns.

### Implementation Steps

#### 1. Create SqliteConnectionPool Class

```typescript
// daemons/data-daemon/server/SqliteConnectionPool.ts
export interface ConnectionPoolConfig {
  path: string;
  poolSize: number;
  idleTimeout?: number;
  healthCheckInterval?: number;
}

export class SqliteConnectionPool {
  private connections: sqlite3.Database[] = [];
  private available: sqlite3.Database[] = [];
  private inUse: Set<sqlite3.Database> = new Set();
  private waitQueue: Array<(db: sqlite3.Database) => void> = [];

  constructor(private config: ConnectionPoolConfig) {}

  async initialize(): Promise<void> {
    // Create initial pool of connections
    for (let i = 0; i < this.config.poolSize; i++) {
      const db = await this.createConnection();
      this.connections.push(db);
      this.available.push(db);
    }
  }

  async acquire(): Promise<sqlite3.Database> {
    if (this.available.length > 0) {
      const db = this.available.pop()!;
      this.inUse.add(db);
      return db;
    }

    // Wait for connection to become available
    return new Promise((resolve) => {
      this.waitQueue.push(resolve);
    });
  }

  release(db: sqlite3.Database): void {
    this.inUse.delete(db);

    if (this.waitQueue.length > 0) {
      const waiter = this.waitQueue.shift()!;
      this.inUse.add(db);
      waiter(db);
    } else {
      this.available.push(db);
    }
  }

  async close(): Promise<void> {
    for (const db of this.connections) {
      await new Promise<void>((resolve, reject) => {
        db.close((err) => err ? reject(err) : resolve());
      });
    }
  }

  private async createConnection(): Promise<sqlite3.Database> {
    return new Promise((resolve, reject) => {
      const db = new sqlite3.Database(
        this.config.path,
        sqlite3.OPEN_READWRITE | sqlite3.OPEN_CREATE,
        (err) => err ? reject(err) : resolve(db)
      );
    });
  }
}
```

#### 2. Add Protected Abstract Methods to SqliteStorageAdapter

```typescript
// SqliteStorageAdapter.ts
export abstract class SqliteStorageAdapter extends SqlStorageAdapterBase {
  // Remove: private db: sqlite3.Database | null = null;

  // Add: Connection pool methods (to be injected)
  private acquireConnectionImpl?: () => Promise<sqlite3.Database>;
  private releaseConnectionImpl?: (db: sqlite3.Database) => void;

  /**
   * Set connection pool methods (called by DataDaemon)
   */
  setConnectionPool(
    acquire: () => Promise<sqlite3.Database>,
    release: (db: sqlite3.Database) => void
  ): void {
    this.acquireConnectionImpl = acquire;
    this.releaseConnectionImpl = release;
  }

  /**
   * Acquire connection from pool
   */
  protected async acquireConnection(): Promise<sqlite3.Database> {
    if (!this.acquireConnectionImpl) {
      throw new Error('Connection pool not configured - call setConnectionPool() first');
    }
    return this.acquireConnectionImpl();
  }

  /**
   * Release connection back to pool
   */
  protected releaseConnection(db: sqlite3.Database): void {
    if (!this.releaseConnectionImpl) {
      throw new Error('Connection pool not configured - call setConnectionPool() first');
    }
    this.releaseConnectionImpl(db);
  }

  /**
   * Execute operation with connection from pool
   */
  protected async withConnection<T>(
    operation: (db: sqlite3.Database) => Promise<T>
  ): Promise<T> {
    const db = await this.acquireConnection();
    try {
      return await operation(db);
    } finally {
      this.releaseConnection(db);
    }
  }
}
```

#### 3. Update SqliteRawExecutor to Accept Connection

```typescript
// SqliteRawExecutor.ts
export class SqliteRawExecutor {
  // Before: constructor(private db: sqlite3.Database)
  // After: No stored connection

  async runSql(db: sqlite3.Database, sql: string, params: SqlValue[]): Promise<Record<string, unknown>[]> {
    return new Promise((resolve, reject) => {
      db.all(sql, params, (err, rows) => {
        if (err) reject(err);
        else resolve(rows);
      });
    });
  }

  async runStatement(db: sqlite3.Database, sql: string, params: SqlValue[]): Promise<{ lastID?: number; changes: number }> {
    return new Promise((resolve, reject) => {
      db.run(sql, params, function(err) {
        if (err) reject(err);
        else resolve({ lastID: this.lastID, changes: this.changes });
      });
    });
  }
}
```

#### 4. Update DataDaemonServer to Create Pool

```typescript
// DataDaemonServer.ts
export class DataDaemonServer extends DataDaemonBase {
  private connectionPool: SqliteConnectionPool;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Create connection pool
    this.connectionPool = new SqliteConnectionPool({
      path: DATABASE_PATHS.SQLITE,
      poolSize: 5,  // TODO: Get from config
      idleTimeout: 60000,
      healthCheckInterval: 30000
    });

    // Create adapter
    const adapter = factory.createAdapter(adapterConfig);

    // Inject connection pool
    (adapter as SqliteStorageAdapter).setConnectionPool(
      () => this.connectionPool.acquire(),
      (db) => this.connectionPool.release(db)
    );

    this.dataDaemon = new DataDaemon(storageConfig, adapter);
  }

  protected async initialize(): Promise<void> {
    // Initialize connection pool FIRST
    await this.connectionPool.initialize();

    // Then initialize DataDaemon
    await this.dataDaemon.initialize();

    // ... rest of initialization
  }

  async close(): Promise<void> {
    await this.dataDaemon.close();
    await this.connectionPool.close();
  }
}
```

#### 5. Update All Query Execution to Use Pool

Every method in SqliteStorageAdapter that currently uses `this.db` needs to change to `this.withConnection()`:

```typescript
// Before
async query<T>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
  const sql = this.buildQuery(query);
  const rows = await this.executor.runSql(sql, params);
  return { success: true, data: rows };
}

// After
async query<T>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
  return this.withConnection(async (db) => {
    const sql = this.buildQuery(query);
    const rows = await this.executor.runSql(db, sql, params);
    return { success: true, data: rows };
  });
}
```

---

## Performance Impact

**Expected improvements** (from bottleneck-removal.md):
- **20-30% throughput increase** with 5-connection pool
- **Reduced query latency** for concurrent operations
- **Better CPU utilization** (multiple queries can run simultaneously)

**Before**:
- Query 1: Wait 50ms
- Query 2: Wait for Query 1 + Wait 50ms = 100ms total
- Query 3: Wait for Query 2 + Wait 50ms = 150ms total

**After (with pool)**:
- Query 1, 2, 3: All wait ~50ms (parallel execution)

---

## Testing Strategy

### Unit Tests
```bash
npx vitest tests/unit/SqliteConnectionPool.test.ts
```

Tests:
- Pool initialization (creates N connections)
- Acquire/release cycle
- Queueing when pool exhausted
- Connection health checks
- Pool cleanup

### Integration Tests
```bash
npx vitest tests/integration/connection-pooling.test.ts
```

Tests:
- Concurrent queries execute in parallel
- Query performance with 1 vs 5 connections
- Connection leaks (acquire without release)
- Pool recovery after errors

### Load Tests
```bash
npx vitest tests/load/concurrent-queries.test.ts
```

Tests:
- 100 concurrent queries
- Throughput measurement (queries/second)
- Latency distribution (p50, p95, p99)

---

## Configuration

Add to `SqliteConfig` in `DatabaseHandleRegistry.ts`:
```typescript
export interface SqliteConfig {
  path: string;
  mode?: OpenMode;
  poolSize?: number;  // Already exists (line 57)
  foreignKeys?: boolean;
  wal?: boolean;
  // New:
  idleTimeout?: number;      // Close idle connections after N ms
  healthCheckInterval?: number;  // Check connection health every N ms
}
```

Default values:
- `poolSize`: 5 (good balance for most workloads)
- `idleTimeout`: 60000 (1 minute)
- `healthCheckInterval`: 30000 (30 seconds)

---

## Migration Path

### Phase 1: Foundation (This PR)
- Create `SqliteConnectionPool` class
- Add `acquireConnection/releaseConnection` to `SqliteStorageAdapter`
- Update `DataDaemonServer` to create and inject pool
- Update a FEW methods to use pool (prove concept)

### Phase 2: Complete Migration
- Update ALL methods in `SqliteStorageAdapter` to use pool
- Remove `this.db` field entirely
- Update all manager classes (SqliteRawExecutor, SqliteQueryExecutor, etc.)

### Phase 3: Testing & Tuning
- Add comprehensive tests
- Benchmark with different pool sizes
- Tune default configuration

---

## Open Questions

1. **Pool size**: Should we use a fixed 5 or make it configurable via environment variable?
   - Recommendation: Start with 5, make configurable later

2. **Transaction handling**: How do transactions work with pooled connections?
   - Recommendation: Acquire connection for transaction duration, release after commit/rollback

3. **Better-sqlite3 migration**: Should we wait for Phase 3 (better-sqlite3) before implementing pooling?
   - Recommendation: No - pooling provides immediate 20-30% improvement, better-sqlite3 is Phase 3

4. **Error handling**: What happens when all connections are busy and queue is full?
   - Recommendation: Add `maxQueueSize` config, reject with error if exceeded

---

## Next Steps

**Awaiting user confirmation on**:
- Is Option A (DataDaemon-managed pool) the correct approach?
- Should we use dependency injection via `setConnectionPool()` or is there a better pattern?
- Are protected methods acceptable or does user prefer pure abstract methods?

**After confirmation**:
1. Implement `SqliteConnectionPool` class
2. Add connection pool methods to `SqliteStorageAdapter`
3. Update `DataDaemonServer` to create pool
4. Migrate a few methods to prove concept
5. Run tests to verify 20-30% improvement
