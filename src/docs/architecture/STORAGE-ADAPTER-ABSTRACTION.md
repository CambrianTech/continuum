# Storage Adapter Abstraction Pattern

## Core Principle

**The adapter layer is completely invisible to the application layer.**

Only the DataDaemon knows adapters exist. Commands, entities, widgets, and all application code interact purely through the DataDaemon interface without any knowledge of the underlying storage mechanism (SQLite, Postgres, JSON, Graph, etc.).

## The Three-Layer Architecture

```
┌─────────────────────────────────────────────────┐
│  APPLICATION LAYER                              │
│  (Commands, Entities, Widgets)                  │
│                                                  │
│  Knows: DataDaemon interface only               │
│  Doesn't know: ANY storage details              │
└──────────────────┬──────────────────────────────┘
                   │
                   │ Commands.execute('data/create', ...)
                   │ Commands.execute('data/list', ...)
                   │
┌──────────────────▼──────────────────────────────┐
│  DAEMON LAYER                                   │
│  (DataDaemon)                                   │
│                                                  │
│  Knows: Adapters exist (abstract interface)    │
│  Emits: Events for data operations              │
│  Doesn't know: Specific adapter implementation  │
└──────────────────┬──────────────────────────────┘
                   │
                   │ adapter.create(...)
                   │ adapter.list(...)
                   │
┌──────────────────▼──────────────────────────────┐
│  ADAPTER LAYER                                  │
│  (SqliteStorageAdapter, PostgresAdapter, etc.) │
│                                                  │
│  Knows: ONLY its storage mechanism              │
│  Doesn't know: Events, commands, wider system   │
└─────────────────────────────────────────────────┘
```

## Critical Boundaries

### ✅ CORRECT: Command calls DataDaemon

```typescript
// In TrainingImportServerCommand.ts
const createResult = await Commands.execute('data/create', {
  collection: 'training_examples',
  data: {
    messages: data.messages,
    messageCount,
    totalTokens: tokenCount,
    metadata: { ... }
  },
  dbHandle
});
```

### ❌ WRONG: Command bypasses to adapter

```typescript
// NEVER DO THIS
import { SqliteStorageAdapter } from '../../daemons/data-daemon/server/adapters/sqlite/SqliteStorageAdapter';

const adapter = new SqliteStorageAdapter();
await adapter.bulkInsert(records);  // ❌ Breaks abstraction
```

**Why wrong:**
- Command now coupled to SQLite
- Cannot swap storage backend
- Events not emitted
- Violates separation of concerns

### ✅ CORRECT: DataDaemon emits events

```typescript
// In DataDaemon.ts
async create<T extends BaseEntity>(data: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>> {
  // Adapter does the work (doesn't know about events)
  const result = await this.adapter.create(data, context);

  // DAEMON emits event at the boundary
  if (result.success && result.data) {
    Events.emit('data:created', {
      collection: context.collection,
      data: result.data
    });
  }

  return result;
}
```

### ❌ WRONG: Adapter emits events

```typescript
// NEVER DO THIS - In SqliteStorageAdapter.ts
async create<T extends BaseEntity>(data: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>> {
  const result = await this.db.insert(data);
  Events.emit('data:created', { ... });  // ❌ Adapter shouldn't know about events
  return result;
}
```

**Why wrong:**
- Adapter knows about event system (violates abstraction)
- Every adapter must duplicate event logic
- Can't swap adapters cleanly

## Common Denominator Abstraction

The `DataStorageAdapter` interface defines operations that work across **ALL** storage types:

### Core Operations (Universal)

```typescript
interface DataStorageAdapter {
  // CRUD - works on SQLite, Postgres, JSON, Graph, etc.
  create<T extends RecordData>(data: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>>;
  read<T extends RecordData>(id: UUID, context: DataOperationContext): Promise<StorageResult<T>>;
  update<T extends RecordData>(id: UUID, data: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>>;
  delete(id: UUID, context: DataOperationContext): Promise<StorageResult<void>>;

  // Queries - abstracted to work everywhere
  list<T extends RecordData>(
    filters?: FilterCondition[],
    options?: QueryOptions,
    context?: DataOperationContext
  ): Promise<StorageResult<T[]>>;

  // Batch operations - each adapter implements using its native mechanism
  batch(operations: StorageOperation[]): Promise<StorageResult<any[]>>;

  // Transactions - abstractable concept
  transaction<T>(callback: () => Promise<T>): Promise<T>;
}
```

### Adapter-Specific Implementations

#### SQLite: Native transactions

```typescript
async batch(ops: StorageOperation[]): Promise<StorageResult<any[]>> {
  return this.db.transaction(() => {
    return ops.map(op => this.executeOperation(op));
  });
}
```

#### Postgres: ACID transactions

```typescript
async batch(ops: StorageOperation[]): Promise<StorageResult<any[]>> {
  const client = await this.pool.connect();
  try {
    await client.query('BEGIN');
    const results = await Promise.all(ops.map(op => this.executeOperation(op, client)));
    await client.query('COMMIT');
    return { success: true, data: results };
  } catch (error) {
    await client.query('ROLLBACK');
    return { success: false, error: error.message };
  } finally {
    client.release();
  }
}
```

#### JSON: File locking

```typescript
async batch(ops: StorageOperation[]): Promise<StorageResult<any[]>> {
  await this.acquireFileLock();
  try {
    const results = ops.map(op => this.executeOperation(op));
    await this.writeToFile(this.data);
    return { success: true, data: results };
  } finally {
    await this.releaseFileLock();
  }
}
```

#### Graph: Native batch API

```typescript
async batch(ops: StorageOperation[]): Promise<StorageResult<any[]>> {
  return this.graphClient.batchExecute(ops.map(op => this.toGraphQuery(op)));
}
```

**Key Insight**: Each adapter implements the SAME interface using its native capabilities. The application doesn't know or care which one is being used.

## When Adapters Cannot Comply

**Rare edge case**: A feature is genuinely impossible in certain storage types.

### Example: Advanced geospatial queries

```typescript
// In DataDaemon (NOT adapter)
async geospatialQuery(params: GeospatialQueryParams): Promise<StorageResult<any[]>> {
  // Check if adapter supports this feature
  if (!this.adapter.supportsFeature('geospatial')) {
    return {
      success: false,
      error: 'Current storage adapter does not support geospatial queries. Please use PostgreSQL adapter with PostGIS extension.'
    };
  }

  return this.adapter.geospatialQuery(params);
}
```

**Implementation pattern**:

```typescript
// In DataStorageAdapter interface
supportsFeature(feature: 'geospatial' | 'fulltext' | 'graph' | ...): boolean;

// In SqliteStorageAdapter
supportsFeature(feature: string): boolean {
  return ['fulltext'].includes(feature);  // SQLite supports FTS, not geospatial
}

// In PostgresStorageAdapter
supportsFeature(feature: string): boolean {
  return ['geospatial', 'fulltext', 'jsonb'].includes(feature);
}
```

**Error handling at DataDaemon level** - NOT in commands, NOT in adapters.

## Event Emission Layers

Different layers emit different kinds of events:

### DataDaemon: Generic data events

```typescript
// After adapter completes operation
Events.emit('data:created', { collection, data });
Events.emit('data:updated', { collection, id, data });
Events.emit('data:deleted', { collection, id });
```

**These are storage operations** - domain-agnostic.

### Commands: Semantic domain events

```typescript
// In TrainingImportServerCommand
if (processedCount % 100 === 0) {
  Events.emit('training:import:progress', {
    processedCount,
    totalTokens,
    percentage: (processedCount / maxExamples) * 100
  });
}

// When complete
Events.emit('training:import:complete', {
  totalExamples,
  importedExamples,
  dbHandle,
  dbPath
});
```

**These are business logic events** - high-level semantics about what the system is doing.

### Adapters: NO events

**Adapters NEVER emit events.** They don't know events exist.

## Adding New Storage Backends

To add a new storage adapter (e.g., MongoDB, Redis, CouchDB):

1. **Implement `DataStorageAdapter` interface**
   ```typescript
   export class MongoStorageAdapter extends DataStorageAdapter {
     async create<T>(data: Partial<T>, context: DataOperationContext): Promise<StorageResult<T>> {
       // MongoDB-specific implementation
     }
     // ... implement all interface methods
   }
   ```

2. **No changes to DataDaemon** (just swap adapter instance)
   ```typescript
   // In DataDaemon initialization
   const adapter = config.storageType === 'mongo'
     ? new MongoStorageAdapter(config)
     : new SqliteStorageAdapter(config);
   ```

3. **No changes to any commands** (they don't know storage changed)

4. **No changes to any entities** (storage-agnostic by design)

5. **All events still work** (DataDaemon emits them, not adapter)

## Real-World Example: Training Data Import

### ✅ Correct architecture (current implementation)

```typescript
// TrainingImportServerCommand.ts
async importJSONL(params: TrainingImportParams, dbHandle: DbHandle) {
  for await (const line of rl) {
    const data = JSON.parse(line);

    // Call DataDaemon through command system (correct!)
    const createResult = await Commands.execute('data/create', {
      collection: 'training_examples',
      data: { messages: data.messages, ... },
      dbHandle
    });

    // Command emits semantic progress event
    if (processedCount % 100 === 0) {
      Events.emit('training:import:progress', { processedCount, totalTokens });
    }
  }
}
```

**Why correct:**
- ✅ Storage-agnostic (works with SQLite, Postgres, JSON, etc.)
- ✅ Respects abstraction layers
- ✅ Events at appropriate levels
- ✅ Can swap storage backend without touching this code

### ❌ Wrong approach (anti-pattern)

```typescript
// DON'T DO THIS
async importJSONL(params: TrainingImportParams) {
  const adapter = new SqliteStorageAdapter();  // ❌ Directly instantiating adapter

  await adapter.bulkInsert(records);  // ❌ Bypassing daemon
  Events.emit('data:created', { ... });  // ❌ Manually emitting low-level events
}
```

**Why wrong:**
- ❌ Hardcoded to SQLite (can't swap storage)
- ❌ Breaks abstraction (command knows about adapter)
- ❌ Event emission duplicated (should be in daemon)
- ❌ Not using command system

## Testing Strategy

### Unit tests: Adapter implementation

```typescript
describe('SqliteStorageAdapter', () => {
  it('should create records with proper transaction', async () => {
    const adapter = new SqliteStorageAdapter({ filename: ':memory:' });
    const result = await adapter.create({ name: 'test' }, context);
    expect(result.success).toBe(true);
  });
});
```

### Integration tests: DataDaemon + Adapter

```typescript
describe('DataDaemon with SqliteAdapter', () => {
  it('should emit events after successful create', async () => {
    const daemon = new DataDaemon(new SqliteStorageAdapter());

    const eventPromise = new Promise(resolve => {
      Events.subscribe('data:created', resolve);
    });

    await daemon.create({ name: 'test' }, context);
    const event = await eventPromise;
    expect(event.data.name).toBe('test');
  });
});
```

### System tests: Full stack

```typescript
describe('Training import workflow', () => {
  it('should import JSONL to any storage backend', async () => {
    // Could be SQLite, Postgres, JSON, etc. - test doesn't know!
    const result = await Commands.execute('training/import', {
      jsonlPath: 'test-data.jsonl',
      datasetName: 'test',
      targetSkill: 'general'
    });

    expect(result.success).toBe(true);
  });
});
```

## Key Takeaways

1. **Adapters are invisible** - only DataDaemon knows they exist
2. **Commands never touch adapters** - always go through DataDaemon
3. **Adapters never emit events** - that's the daemon's job
4. **Common denominator interface** - works across all storage types
5. **Feature detection for edge cases** - handle at daemon level, not commands
6. **Storage-agnostic by design** - swap backends with zero application code changes

## Related Documents

- [Entity Adapter Architecture](./entity-adapter-architecture.md) - Entity-level abstraction
- [CRUD Event Test Architecture](./CRUD-EVENT-TEST-ARCHITECTURE.md) - Testing data operations
- [Elegant CRUD Architecture](./ELEGANT-CRUD-ARCHITECTURE.md) - Design philosophy
