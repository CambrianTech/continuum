# Multi-Database Handle System

## Problem

The genome system needs multiple databases:
- **Main DB**: `.continuum/jtag/data/database.sqlite` (users, rooms, messages)
- **Training DBs**: `/datasets/prepared/continuum-git.sqlite` (training examples)
- **Persona DBs**: `.continuum/personas/helper-ai/knowledge.sqlite` (per-persona data)
- **Adapter DBs**: `.continuum/adapters/typescript-expert/metadata.sqlite` (adapter metrics)

Currently, all `data/*` commands use a single hardcoded database path.

## Solution: Database Handles

### Architecture

**Key Insight**: A `DbHandle` is storage-adapter-agnostic. It can point to:
- SQLite database
- JSON file storage
- Vector database (Qdrant, Pinecone)
- Graph database (Neo4j)
- Any implementation of `DataStorageAdapter`

```typescript
// Database handle - opaque identifier for ANY storage adapter
type DbHandle = string;  // UUID or 'default'

// Default handle (uses DATABASE_PATHS.SQLITE with SqliteStorageAdapter)
const DEFAULT_HANDLE = 'default';

// Commands.execute with optional dbHandle parameter
// Works with ANY storage backend through the handle!
Commands.execute('data/create', {
  dbHandle: 'training-db-001',  // Could be SQLite, JSON, Vector DB, etc.
  collection: 'training_examples',
  data: example
});
```

### Key Design Principles

1. **Backward Compatible**: All existing code continues to work without changes
2. **Single Source of Truth**: `DATABASE_PATHS.SQLITE` remains the default
3. **Explicit Handles**: Must call `data/open` to get non-default handles
4. **Auto-cleanup**: Handles close after inactivity or on explicit `data/close`
5. **Thread-safe**: Handle registry is connection pool

### New Commands

#### `data/open` - Get Database Handle

```typescript
// Example 1: SQLite database
const result = await Commands.execute('data/open', {
  context,
  sessionId,
  adapter: 'sqlite',  // NEW: specify adapter type
  config: {
    path: '/datasets/prepared/continuum-git.sqlite',
    mode: 'readonly',  // 'readonly' | 'readwrite' | 'create'
    poolSize: 5        // Optional connection pool size
  }
});

// Example 2: JSON file storage
const result = await Commands.execute('data/open', {
  context,
  sessionId,
  adapter: 'json',
  config: {
    path: '.continuum/config/settings.json',
    pretty: true
  }
});

// Example 3: Vector database
const result = await Commands.execute('data/open', {
  context,
  sessionId,
  adapter: 'vector',
  config: {
    endpoint: 'http://localhost:6333',  // Qdrant
    collection: 'code-embeddings',
    apiKey: process.env.QDRANT_API_KEY
  }
});

// Returns: { dbHandle: 'uuid-1234-5678', adapter: 'sqlite', ... }
```

#### `data/close` - Close Database Handle

```typescript
await Commands.execute('data/close', {
  context,
  sessionId,
  dbHandle: 'uuid-1234-5678'
});
```

#### `data/list-handles` - List Open Handles

```typescript
const result = await Commands.execute('data/list-handles', {
  context,
  sessionId
});

// Returns:
// {
//   handles: [
//     { handle: 'default', path: '.continuum/jtag/data/database.sqlite', connections: 1 },
//     { handle: 'uuid-1234', path: '/datasets/prepared/continuum-git.sqlite', connections: 2 }
//   ]
// }
```

### Updated Data Commands

All existing `data/*` commands gain optional `dbHandle` parameter:

```typescript
// data/create
Commands.execute('data/create', {
  dbHandle: 'training-db',  // NEW - optional
  collection: 'training_examples',
  data: example,
  id: uuid
});

// data/read
Commands.execute('data/read', {
  dbHandle: 'training-db',  // NEW - optional
  collection: 'training_examples',
  id: uuid
});

// data/list
Commands.execute('data/list', {
  dbHandle: 'training-db',  // NEW - optional
  collection: 'training_examples',
  filter: { quality: { $gte: 0.8 } }
});

// data/update
Commands.execute('data/update', {
  dbHandle: 'training-db',  // NEW - optional
  collection: 'training_examples',
  id: uuid,
  updates: { status: 'used' }
});

// data/delete
Commands.execute('data/delete', {
  dbHandle: 'training-db',  // NEW - optional
  collection: 'training_examples',
  id: uuid
});
```

**If `dbHandle` is omitted**: Uses `'default'` handle → `DATABASE_PATHS.SQLITE`

### Implementation

#### DatabaseHandleRegistry.ts

```typescript
/**
 * Registry of open storage adapters
 * Maps handles to ANY DataStorageAdapter implementation
 *
 * CRITICAL: Storage-adapter-agnostic!
 * Can manage SQLite, JSON, Vector DB, Graph DB, or any adapter
 */
export class DatabaseHandleRegistry {
  private static instance: DatabaseHandleRegistry;
  private handles: Map<DbHandle, DataStorageAdapter>;  // ANY adapter type!
  private handleMetadata: Map<DbHandle, { adapter: string; config: any }>;

  private constructor() {
    this.handles = new Map();
    this.handleMetadata = new Map();

    // Initialize default handle (SQLite)
    const defaultAdapter = new SqliteStorageAdapter({
      filename: DATABASE_PATHS.SQLITE
    });
    this.handles.set('default', defaultAdapter);
    this.handleMetadata.set('default', {
      adapter: 'sqlite',
      config: { path: DATABASE_PATHS.SQLITE }
    });
  }

  static getInstance(): DatabaseHandleRegistry {
    if (!this.instance) {
      this.instance = new DatabaseHandleRegistry();
    }
    return this.instance;
  }

  /**
   * Open a new database connection and return handle
   */
  async open(dbPath: string, options?: SqliteOptions): Promise<DbHandle> {
    const handle = generateUUID();
    const adapter = new SqliteStorageAdapter({
      filename: dbPath,
      ...options
    });

    await adapter.initialize();

    this.handles.set(handle, adapter);
    this.handlePaths.set(handle, dbPath);

    return handle;
  }

  /**
   * Get adapter for handle (returns default if handle not found or omitted)
   */
  getAdapter(handle?: DbHandle): SqliteStorageAdapter {
    const actualHandle = handle || 'default';
    const adapter = this.handles.get(actualHandle);

    if (!adapter) {
      console.warn(`⚠️  Database handle '${actualHandle}' not found, using default`);
      return this.handles.get('default')!;
    }

    return adapter;
  }

  /**
   * Close database handle
   */
  async close(handle: DbHandle): Promise<void> {
    if (handle === 'default') {
      throw new Error('Cannot close default database handle');
    }

    const adapter = this.handles.get(handle);
    if (adapter) {
      await adapter.close();
      this.handles.delete(handle);
      this.handlePaths.delete(handle);
    }
  }

  /**
   * List all open handles
   */
  listHandles(): Array<{ handle: DbHandle; path: string; isDefault: boolean }> {
    const result: Array<{ handle: DbHandle; path: string; isDefault: boolean }> = [];

    for (const [handle, path] of this.handlePaths.entries()) {
      result.push({
        handle,
        path,
        isDefault: handle === 'default'
      });
    }

    return result;
  }
}
```

#### Updated DataDaemon.ts

```typescript
import { DatabaseHandleRegistry } from './DatabaseHandleRegistry';

export class DataDaemon {
  private handleRegistry: DatabaseHandleRegistry;

  constructor() {
    this.handleRegistry = DatabaseHandleRegistry.getInstance();
  }

  /**
   * Get storage adapter for command (with optional dbHandle)
   */
  private getAdapter(dbHandle?: DbHandle): SqliteStorageAdapter {
    return this.handleRegistry.getAdapter(dbHandle);
  }

  async create<T extends BaseEntity>(params: DataCreateParams): Promise<DataCreateResult<T>> {
    const adapter = this.getAdapter(params.dbHandle);  // NEW
    return await adapter.create(params);
  }

  async read<T extends BaseEntity>(params: DataReadParams): Promise<DataReadResult<T>> {
    const adapter = this.getAdapter(params.dbHandle);  // NEW
    return await adapter.read(params);
  }

  async list<T extends BaseEntity>(params: DataListParams): Promise<DataListResult<T>> {
    const adapter = this.getAdapter(params.dbHandle);  // NEW
    return await adapter.list(params);
  }

  // ... etc for update, delete, count, schema
}
```

### Usage Examples

#### Example 1: Training Data Pipeline

```typescript
// 1. Open training database
const { dbHandle } = await Commands.execute('data/open', {
  context,
  sessionId,
  dbPath: '/datasets/prepared/continuum-git.sqlite',
  mode: 'create'  // Create if doesn't exist
});

// 2. Import training examples from JSONL
for (const example of trainingExamples) {
  await Commands.execute('data/create', {
    dbHandle,  // Use training DB handle
    collection: 'training_examples',
    data: example,
    id: generateUUID()
  });
}

// 3. Query training data
const highQuality = await Commands.execute('data/list', {
  dbHandle,
  collection: 'training_examples',
  filter: { 'metadata.qualityScore': { $gte: 0.8 } },
  orderBy: [{ field: 'metadata.timestamp', direction: 'desc' }],
  limit: 100
});

// 4. Close when done
await Commands.execute('data/close', {
  context,
  sessionId,
  dbHandle
});
```

#### Example 2: Persona Knowledge Base

```typescript
// PersonaUser maintains its own knowledge DB
class PersonaUser extends AIUser {
  private knowledgeHandle?: DbHandle;

  async initialize() {
    // Open persona-specific database
    const { dbHandle } = await Commands.execute('data/open', {
      context: this.context,
      sessionId: this.sessionId,
      dbPath: `.continuum/personas/${this.userId}/knowledge.sqlite`,
      mode: 'create'
    });

    this.knowledgeHandle = dbHandle;
  }

  async rememberFact(fact: string, context: string) {
    await Commands.execute('data/create', {
      dbHandle: this.knowledgeHandle,  // Persona-specific DB
      collection: 'facts',
      data: {
        fact,
        context,
        learnedAt: Date.now(),
        confidence: 0.9
      },
      id: generateUUID()
    });
  }

  async recallFacts(query: string): Promise<Fact[]> {
    const result = await Commands.execute('data/list', {
      dbHandle: this.knowledgeHandle,
      collection: 'facts',
      filter: { fact: { $contains: query } },
      orderBy: [{ field: 'confidence', direction: 'desc' }],
      limit: 10
    });

    return result.items;
  }
}
```

#### Example 3: Default Handle (Backward Compatible)

```typescript
// This code doesn't change at all - uses default handle
const users = await Commands.execute('data/list', {
  context,
  sessionId,
  collection: 'users'  // No dbHandle = uses default
});

const rooms = await Commands.execute('data/list', {
  context,
  sessionId,
  collection: 'rooms'  // No dbHandle = uses default
});
```

### Database Naming Convention

For consistency across the system:

```
Main Database:
  .continuum/jtag/data/database.sqlite

Training Databases:
  /datasets/prepared/<dataset-name>.sqlite
  Examples:
    /datasets/prepared/continuum-git.sqlite
    /datasets/prepared/claude-conversations.sqlite
    /datasets/prepared/master-programmer.sqlite

Persona Databases:
  .continuum/personas/<persona-id>/<db-name>.sqlite
  Examples:
    .continuum/personas/helper-ai/knowledge.sqlite
    .continuum/personas/helper-ai/memories.sqlite

Adapter Databases:
  .continuum/adapters/<adapter-id>/<db-name>.sqlite
  Examples:
    .continuum/adapters/typescript-expert/metrics.sqlite
    .continuum/adapters/typescript-expert/training-history.sqlite
```

### Migration Strategy

**Phase 1**: Add handle support (backward compatible)
- Implement DatabaseHandleRegistry
- Add optional `dbHandle` parameter to all data commands
- Default to 'default' handle when omitted

**Phase 2**: Use in training pipeline
- Open training DB with `data/open`
- Import JSONL to SQLite
- Query training data

**Phase 3**: Persona knowledge bases
- PersonaUser opens persona-specific DB
- Store/retrieve facts, memories, preferences

**Phase 4**: Adapter metadata
- Track adapter usage, performance, training history
- Per-adapter databases

### Testing

```typescript
// Test multi-database operations
describe('Multi-Database Handles', () => {
  it('should use default handle when omitted', async () => {
    const users = await Commands.execute('data/list', {
      collection: 'users'
    });
    expect(users.items).toBeDefined();
  });

  it('should open and use training database', async () => {
    const { dbHandle } = await Commands.execute('data/open', {
      dbPath: '/tmp/test-training.sqlite',
      mode: 'create'
    });

    await Commands.execute('data/create', {
      dbHandle,
      collection: 'training_examples',
      data: { text: 'example', label: 'positive' },
      id: 'test-001'
    });

    const result = await Commands.execute('data/read', {
      dbHandle,
      collection: 'training_examples',
      id: 'test-001'
    });

    expect(result.item.text).toBe('example');

    await Commands.execute('data/close', { dbHandle });
  });
});
```

---

## Summary

**What changes**:
- Add `DatabaseHandleRegistry` to manage multiple SQLite connections
- Add `data/open`, `data/close`, `data/list-handles` commands
- Add optional `dbHandle` parameter to all `data/*` commands
- Update `DataDaemon` to route commands to correct adapter

**What stays the same**:
- All existing code works without modification
- `DATABASE_PATHS.SQLITE` remains single source of truth for default
- Same entity system, same decorators, same query API

**Mission critical for**:
- Training data pipelines (JSONL → SQLite → MLX)
- Persona knowledge bases (per-AI learning)
- Adapter metadata (tracking usage, performance)
- Multi-tenant scenarios (isolation per user/persona)
