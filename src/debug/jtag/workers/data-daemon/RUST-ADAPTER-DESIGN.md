# Rust Data Adapter - Complete Design

## Architecture Overview

```
TypeScript DataDaemon (unchanged - orchestration, decorators, events)
└── RustAdapter.ts (NEW - one of many storage adapters)
    ├── Extends DataStorageAdapter (drop-in replacement)
    ├── Uses WorkerClient pattern (like LoggerWorkerClient)
    └── Unix Socket → RustDataWorker
        ├── Handle Registry (multi-database support)
        │   ├── Handle #1 → /Users/joel/.continuum/data/database.sqlite (main)
        │   ├── Handle #2 → /Users/joel/.continuum/data/persona-helper.sqlite
        │   ├── Handle #3 → /Volumes/SlimGordon/archive/old-data.sqlite
        │   └── Each handle: independent connection pool + storage detection
        ├── Per-Handle Adapters
        │   ├── SqliteStrategy (storage-aware pragmas)
        │   ├── PostgresStrategy (connection pool)
        │   └── JsonStrategy (file locks)
        └── Massive Concurrency (100+ handles, each with pool)
```

## Why This Pattern Works

**Proven Pattern**: LoggerDaemon → LoggerWorkerClient → Unix Socket → Rust Logger Worker

**Key Success Factors**:
1. TypeScript keeps high-level orchestration (decorators, validation, events)
2. Rust does heavy lifting (I/O, threading, connection pooling)
3. Unix socket = low overhead, high throughput
4. Clean separation: TypeScript ↔ Rust protocol via ts-rs types
5. Graceful degradation: Falls back to TS adapter if Rust worker not running

## Critical Insight: Multi-Handle Architecture

**NOT**: One database connection shared by all operations
**YES**: Many handles, each managing different database with own config

```typescript
// Each data/open creates a new handle in Rust
const handle1 = await DataDaemon.open({ path: '/Users/joel/.continuum/data/database.sqlite' });
const handle2 = await DataDaemon.open({ path: '/Users/joel/.continuum/data/persona-helper.sqlite' });
const handle3 = await DataDaemon.open({ path: '/Volumes/SlimGordon/archive/archive-001.sqlite' });

// Rust worker manages ALL handles concurrently
// handle1: InternalSSD → WAL mode, connection pool, high concurrency
// handle2: InternalSSD → WAL mode, independent pool
// handle3: SD card → DELETE mode, safe for removable media
```

**Why This Matters**:
- **Persona databases**: Each AI gets dedicated database, no contention
- **Archive databases**: Different storage types, automatic optimization
- **Multi-tenant**: Different users/contexts get isolated databases
- **Massive parallelism**: 100+ handles active simultaneously

## Phase 1: RustAdapter.ts (TypeScript Side)

### File Structure

```
daemons/data-daemon/server/
├── RustAdapter.ts (NEW - implements DataStorageAdapter)
└── adapters/
    ├── SqliteAdapter.ts (existing TS implementation)
    ├── PostgresAdapter.ts (existing TS implementation)
    └── RustAdapter.ts → symlink to parent for clarity
```

### RustAdapter.ts Implementation

```typescript
import { DataStorageAdapter } from '../shared/DataStorageAdapter';
import { DataWorkerClient } from '@shared/ipc/data-worker/DataWorkerClient';

export class RustAdapter extends DataStorageAdapter {
  private workerClient: DataWorkerClient;
  private handle: string | null = null;

  async initialize(config: StorageAdapterConfig): Promise<void> {
    // Create client (like LoggerWorkerClient pattern)
    this.workerClient = new DataWorkerClient({
      socketPath: '/tmp/jtag-data-worker.sock',
      timeout: 10000
    });

    await this.workerClient.connect();

    // Open database handle in Rust worker
    const openResult = await this.workerClient.openDatabase({
      path: config.options.filename || getDatabasePath(),
      adapterType: 'sqlite',
      storageType: 'auto-detect' // Rust detects InternalSSD/ExternalSSD/SDCard
    });

    this.handle = openResult.handle;
    console.log(`✅ Rust database handle: ${this.handle}`);
  }

  async create<T>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    return this.workerClient.create({
      handle: this.handle!,
      collection: record.collection,
      data: record.data
    });
  }

  async read<T>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    return this.workerClient.read({
      handle: this.handle!,
      collection,
      id
    });
  }

  // ... all other DataStorageAdapter methods delegate to workerClient
}
```

### DataWorkerClient.ts (NEW)

```typescript
// shared/ipc/data-worker/DataWorkerClient.ts
import { WorkerClient } from '../WorkerClient';
import type {
  OpenDatabaseRequest,
  OpenDatabaseResponse,
  CreateRecordRequest,
  CreateRecordResponse,
  ReadRecordRequest,
  ReadRecordResponse
  // ... all message types
} from './DataWorkerMessageTypes';

export class DataWorkerClient extends WorkerClient {
  async openDatabase(req: OpenDatabaseRequest): Promise<OpenDatabaseResponse> {
    return this.send('open-database', req);
  }

  async create<T>(req: CreateRecordRequest<T>): Promise<CreateRecordResponse<T>> {
    return this.send('create-record', req);
  }

  async read<T>(req: ReadRecordRequest): Promise<ReadRecordResponse<T>> {
    return this.send('read-record', req);
  }

  // ... all data operations
}
```

## Phase 2: Rust Data Worker

### Project Structure

```
workers/data-daemon/
├── Cargo.toml
├── src/
│   ├── main.rs (entry point, socket handling)
│   ├── handle_registry.rs (manages multiple database handles)
│   ├── storage_detection.rs (InternalSSD/ExternalSSD/SDCard detection)
│   ├── adapters/
│   │   ├── mod.rs
│   │   ├── sqlite_strategy.rs (storage-aware pragmas + connection pool)
│   │   ├── postgres_strategy.rs (connection pool)
│   │   └── json_strategy.rs (file locks)
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── messages.rs (ts-rs types, exported to TypeScript)
│   │   └── handler.rs (message routing)
│   └── concurrency/
│       ├── write_queue.rs (per-handle write queueing)
│       └── read_pool.rs (concurrent reads)
```

### Handle Registry (Key Innovation)

```rust
// handle_registry.rs
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct HandleRegistry {
    handles: Arc<Mutex<HashMap<Uuid, DatabaseHandle>>>,
}

struct DatabaseHandle {
    handle_id: Uuid,
    db_path: PathBuf,
    storage_type: StorageType,
    adapter: Box<dyn ConcurrencyStrategy>,
    connection_pool: SqlitePool, // rusqlite pool
    write_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
}

impl HandleRegistry {
    pub fn open(&self, path: PathBuf, adapter_type: AdapterType) -> Result<Uuid, String> {
        let handle_id = Uuid::new_v4();

        // Detect storage type ONCE per handle
        let storage_type = detect_storage_type(&path);
        println!("🔍 Handle {}: {:?} detected for {}", handle_id, storage_type, path.display());

        // Create adapter with storage-aware config
        let adapter = match adapter_type {
            AdapterType::Sqlite => {
                SqliteStrategy::new(
                    path.clone(),
                    storage_type,
                    get_sqlite_pragmas(storage_type, false) // Single-writer per handle
                )?
            }
            // ... other adapter types
        };

        let handle = DatabaseHandle {
            handle_id,
            db_path: path,
            storage_type,
            adapter: Box::new(adapter),
            connection_pool: create_pool(&path, storage_type)?,
            write_queue: Arc::new(Mutex::new(VecDeque::new())),
        };

        self.handles.lock().unwrap().insert(handle_id, handle);
        Ok(handle_id)
    }

    pub fn get(&self, handle_id: Uuid) -> Result<DatabaseHandle, String> {
        self.handles.lock().unwrap()
            .get(&handle_id)
            .cloned()
            .ok_or_else(|| format!("Handle not found: {}", handle_id))
    }
}
```

### SqliteStrategy (Storage-Aware)

```rust
// adapters/sqlite_strategy.rs
struct SqliteStrategy {
    db_path: PathBuf,
    storage_type: StorageType,
    pool: SqlitePool,
    write_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
}

impl SqliteStrategy {
    fn new(path: PathBuf, storage_type: StorageType, pragmas: String) -> Result<Self, String> {
        // Create connection pool (5-10 connections per handle)
        let pool = SqlitePoolBuilder::new()
            .max_connections(match storage_type {
                StorageType::InternalSSD => 10, // WAL allows concurrent readers
                StorageType::ExternalSSD => 5,
                _ => 1 // DELETE mode, single connection
            })
            .connection_customizer(Box::new(move |conn| {
                conn.execute_batch(&pragmas)?;
                Ok(())
            }))
            .build()?;

        Ok(Self {
            db_path: path,
            storage_type,
            pool,
            write_queue: Arc::new(Mutex::new(VecDeque::new())),
        })
    }
}

impl ConcurrencyStrategy for SqliteStrategy {
    fn execute_read(&self, query: &str, params: &[Value]) -> Result<Vec<Row>, String> {
        // Get connection from pool (concurrent reads in WAL mode!)
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(query)?;

        let rows = stmt.query_map(params, |row| {
            // ... map to Row struct
        })?;

        Ok(rows.collect()?)
    }

    fn execute_write(&self, query: &str, params: &[Value]) -> Result<WriteResult, String> {
        // Queue write for serial processing (prevents lock contention)
        let mut queue = self.write_queue.lock().unwrap();
        queue.push_back(WriteOperation { query, params });

        // Process queue serially
        let conn = self.pool.get()?;
        while let Some(op) = queue.pop_front() {
            conn.execute(&op.query, &op.params)?;
        }

        Ok(WriteResult { rows_affected: 1 })
    }
}
```

## Phase 3: Message Protocol (ts-rs Types)

```rust
// protocol/messages.rs
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/ipc/data-worker/")]
pub struct OpenDatabaseRequest {
    pub path: String,
    pub adapter_type: AdapterType,
    pub storage_type: String, // "auto-detect" or explicit
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/ipc/data-worker/")]
pub struct OpenDatabaseResponse {
    pub handle: String, // UUID as string
    pub storage_type: StorageType,
    pub pragma_mode: String, // "WAL" or "DELETE"
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/ipc/data-worker/")]
pub struct CreateRecordRequest {
    pub handle: String,
    pub collection: String,
    pub data: serde_json::Value,
}

// ... all CRUD operations with ts-rs export
```

## Migration Strategy

### Phase 1: Standalone Testing (Week 1)
- Build Rust data worker with handle registry
- Test storage detection with multiple paths
- Test concurrent operations (100+ handles)
- Verify pragma configuration per storage type

### Phase 2: TypeScript Integration (Week 2)
- Create `RustAdapter.ts` implementing `DataStorageAdapter`
- Create `DataWorkerClient.ts` (like LoggerWorkerClient)
- Wire into DataDaemon as new adapter type
- Flag: `USE_RUST_DATA_ADAPTER=true` for testing

### Phase 3: Production Migration (Week 3)
- Migrate main database to RustAdapter
- Test with real workload (personas, chat, state)
- Monitor performance, stability, resource usage
- Gradually migrate archive databases

### Phase 4: Multi-Database Support (Week 4)
- Test persona databases (one per AI)
- Test archive databases (SD card vs internal SSD)
- Benchmark 100+ concurrent handles
- Verify storage detection across all mount points

## Success Criteria

1. **Performance**: 10x faster than TS adapter for concurrent workloads
2. **Reliability**: Zero crashes, automatic storage adaptation
3. **Scalability**: 100+ concurrent database handles
4. **Graceful Degradation**: Falls back to TS adapter if Rust worker unavailable
5. **Developer Experience**: Drop-in replacement, no breaking changes to DataDaemon

## Key Learnings from Past Failures

**DON'T**:
- Own database connections directly in Rust (bypass coordination)
- Complex adapter registry with borrowing issues
- Dual ownership (both TS and Rust managing same connection)

**DO**:
- Use WorkerClient pattern (proven with LoggerDaemon)
- Handle-based API (texture ID pattern from graphics)
- Storage detection per handle (automatic optimization)
- Clean separation: TS orchestrates, Rust executes
- Graceful fallback when worker unavailable

## Next Steps

1. **Read existing code**:
   - `shared/ipc/WorkerClient.ts` (base class)
   - `shared/ipc/logger/LoggerWorkerClient.ts` (reference implementation)
   - `workers/logger/src/main.rs` (working Rust worker)

2. **Create message types**:
   - `shared/ipc/data-worker/DataWorkerMessageTypes.ts`
   - `workers/data-daemon/src/protocol/messages.rs` (with ts-rs)

3. **Implement RustAdapter.ts**:
   - Extends `DataStorageAdapter`
   - Uses `DataWorkerClient` for all operations
   - Test with single handle first

4. **Build Rust worker**:
   - Handle registry
   - Storage detection
   - SqliteStrategy with pooling
   - Test with multiple concurrent handles

5. **Integration testing**:
   - Compare TS adapter vs Rust adapter
   - Benchmark concurrent operations
   - Verify storage detection accuracy
   - Test graceful degradation
