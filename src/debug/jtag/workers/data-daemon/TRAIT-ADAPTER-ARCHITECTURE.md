# Rust Data Daemon - Trait-Based Adapter Architecture

## Overview

The Rust data daemon now uses a **trait-based adapter architecture** that mirrors the TypeScript `DataStorageAdapter` pattern. This allows multiple storage backends (SQLite, PostgreSQL, JSON, etc.) to implement the same interface while the handlers remain backend-agnostic.

## Architecture Goals

1. **Mirror TypeScript patterns** - Rust adapter trait matches TypeScript `DataStorageAdapter` abstract class
2. **Backend independence** - Handlers use trait methods, not direct database connections
3. **Easy extensibility** - New adapters just implement the `StorageAdapter` trait
4. **Config-driven selection** - `DATA_DAEMON_TYPE` env var controls which adapter is used

## Component Structure

```
workers/data-daemon/src/
├── main.rs                    # Handlers use StorageAdapter trait
├── storage/
│   ├── mod.rs                 # Module exports
│   ├── adapter.rs             # StorageAdapter trait definition
│   └── sqlite.rs              # SqliteAdapter implementation
└── entities.rs                # Generated from TypeScript decorators
```

## The StorageAdapter Trait

**File**: `src/storage/adapter.rs`

Defines the interface that all storage adapters must implement:

```rust
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    async fn initialize(&mut self, config: Value) -> Result<(), Box<dyn Error>>;
    async fn create(&self, collection: &str, record: Value) -> Result<Value, Box<dyn Error>>;
    async fn read(&self, collection: &str, id: &str) -> Result<Option<Value>, Box<dyn Error>>;
    async fn query(&self, query: Value) -> Result<Vec<Value>, Box<dyn Error>>;
    async fn update(&self, collection: &str, id: &str, data: Value) -> Result<Value, Box<dyn Error>>;
    async fn delete(&self, collection: &str, id: &str) -> Result<bool, Box<dyn Error>>;
    async fn ensure_schema(&self, collection: &str, schema: Option<Value>) -> Result<bool, Box<dyn Error>>;
    async fn list_collections(&self) -> Result<Vec<String>, Box<dyn Error>>;
    async fn get_collection_stats(&self, collection: &str) -> Result<Value, Box<dyn Error>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error>>;
}
```

## SqliteAdapter Implementation

**File**: `src/storage/sqlite.rs`

### Current Implementation Status

| Method | Status | Notes |
|--------|--------|-------|
| `initialize()` | ✅ Implemented | Opens connection, sets WAL mode, pragmas |
| `create()` | ✅ Implemented | Dynamic INSERT based on record fields |
| `read()` | ✅ Implemented | SELECT * with JSON building from columns |
| `query()` | ✅ Implemented | Full query support (filter, sort, limit, offset) |
| `update()` | ❌ todo!() | Not yet implemented |
| `delete()` | ❌ todo!() | Not yet implemented |
| `ensure_schema()` | ✅ Implemented | No-op (tables managed by TypeScript) |
| `list_collections()` | ✅ Implemented | Query sqlite_master |
| `get_collection_stats()` | ✅ Implemented | COUNT(*) query |
| `close()` | ✅ Implemented | Drops connection |

### Key Implementation Details

1. **Connection Storage**: Uses `Arc<Mutex<Option<Connection>>>` for thread-safe access
2. **Type Detection**: Uses `ValueRef` for efficient type detection (no trial-and-error)
3. **camelCase → snake_case**: Automatic conversion for field names
4. **Scoped Locking**: Releases mutex before building final JSON to avoid deadlocks

## Handler Architecture

**File**: `src/main.rs`

### Database Handle Registry

```rust
struct DatabaseHandle {
    adapter: Box<dyn StorageAdapter>,  // Trait object, not raw Connection
    path: String,
    opened_at: String,
}
```

### Handler Pattern

All handlers now follow this pattern:

1. **Get adapter from registry** (not raw Connection)
2. **Call trait method** via `tokio::runtime::Runtime::block_on()`
3. **Return result** in JTAG protocol format

**Example** (`handle_query_records`):

```rust
// Get adapter from registry
let reg = registry.lock().unwrap();
let db_handle = match reg.get(&query_req.handle) {
    Some(h) => h,
    None => return error_response("Database handle not found"),
};

// Call adapter.query() via tokio runtime
let records = tokio::runtime::Runtime::new()
    .unwrap()
    .block_on(db_handle.adapter.query(query_json));

// Return results
match records {
    Ok(r) => success_response(r),
    Err(err) => error_response(err),
}
```

## TypeScript Integration

### Config-Driven Backend Selection

**File**: `daemons/data-daemon/server/DataDaemonServer.ts`

```typescript
// Read DATA_DAEMON_TYPE from config.env (rust | sqlite)
const secrets = SecretManager.getInstance();
const backend = secrets.get('DATA_DAEMON_TYPE') || 'sqlite';  // Default to SQLite

const storageConfig: StorageStrategyConfig = {
  strategy: 'sql',
  backend: backend as 'rust' | 'sqlite',  // Controlled by DATA_DAEMON_TYPE env var
  // ...
};
```

**File**: `daemons/data-daemon/server/DefaultStorageAdapterFactory.ts`

```typescript
createAdapter(config: StorageAdapterConfig): DataStorageAdapter {
  switch (config.type) {
    case 'sqlite':
      return new SqliteStorageAdapter();  // TypeScript SQLite
    case 'rust':
      return new RustAdapter();            // Rust worker via socket
    default:
      throw new Error(`Unsupported storage adapter type: ${config.type}`);
  }
}
```

### How Backend Selection Works

1. **~/.continuum/config.env**: Set `DATA_DAEMON_TYPE=sqlite` or `DATA_DAEMON_TYPE=rust`
2. **System Startup**: SecretManager loads config.env
3. **DataDaemonServer Construction**: Reads `DATA_DAEMON_TYPE` from SecretManager
4. **Factory Selection**: DefaultStorageAdapterFactory creates appropriate adapter
5. **Runtime Behavior**:
   - `sqlite` → TypeScript SqliteStorageAdapter (no Rust worker needed)
   - `rust` → RustAdapter → connects to Rust worker via socket

## Entity Generation Flow

**Single Source of Truth**: TypeScript entities with `@Field()` decorators

```
TypeScript Entities (@Field decorators)
    ↓
generator/generate-rust-types.ts
    ↓
workers/data-daemon/src/entities.rs
```

TypeScript decorators define the schema, Rust types are auto-generated. This ensures schema consistency across TypeScript and Rust.

## Testing & Verification

### Verify Current Backend

```bash
# Check config
grep DATA_DAEMON_TYPE ~/.continuum/config.env

# Expected: DATA_DAEMON_TYPE=sqlite (or rust)
```

### Test with SQLite Backend (TypeScript)

```bash
# 1. Set config
echo "DATA_DAEMON_TYPE=sqlite" >> ~/.continuum/config.env

# 2. Restart system (config loaded at startup)
npm start

# 3. Test data operations
./jtag data/list --collection=users --limit=5
./jtag ping
```

### Test with Rust Backend (Rust Worker)

```bash
# 1. Start Rust workers
./workers/start-workers.sh

# 2. Set config
sed -i '' 's/DATA_DAEMON_TYPE=sqlite/DATA_DAEMON_TYPE=rust/' ~/.continuum/config.env

# 3. Restart system
npm start

# 4. Test data operations
./jtag data/list --collection=users --limit=5
```

**Note**: Rust backend requires workers to be running BEFORE system starts, otherwise RustAdapter initialization fails and system falls back to sqlite.

## Implementation Notes

### Why Traits Instead of Concrete Types?

Mirrors TypeScript's inheritance pattern where `SqliteStorageAdapter`, `MemoryStorageAdapter`, etc. all extend `DataStorageAdapter`. Rust uses traits for polymorphism.

### Why tokio::runtime::Runtime in Handlers?

The `StorageAdapter` trait methods are async (to match TypeScript patterns), but the handlers run in sync context (Unix socket message loop). We use `Runtime::block_on()` to bridge sync/async boundary.

### Why Not Use Rust Worker for Everything?

- **Development velocity**: TypeScript is faster to iterate on
- **Proven stability**: TypeScript adapter is battle-tested
- **Gradual migration**: Can test Rust backend in isolation before full cutover
- **Fallback safety**: System can fall back to TypeScript if Rust worker crashes

## Known Limitations

1. **update() and delete() not implemented** - Still `todo!()` in SqliteAdapter
2. **No connection pooling** - Single connection per handle (TODO: use deadpool-sqlite)
3. **Sync/async bridge overhead** - `Runtime::block_on()` adds latency
4. **Config requires restart** - Changing `DATA_DAEMON_TYPE` needs full system restart

## Future Improvements

1. **Implement update/delete** in SqliteAdapter
2. **Add PostgresAdapter** implementing StorageAdapter trait
3. **Add JsonFileAdapter** for file-based storage
4. **Connection pooling** via deadpool-sqlite
5. **Native async handlers** to eliminate Runtime::block_on() overhead
6. **Hot config reload** to switch backends without restart

## Commit Status

- ✅ StorageAdapter trait mirrors TypeScript DataStorageAdapter
- ✅ SqliteAdapter implements query, read, create, ensure_schema
- ✅ Handlers use trait instead of raw Connection
- ✅ DATA_DAEMON_TYPE config switch working
- ✅ System verified working with DATA_DAEMON_TYPE=sqlite
- ❌ Rust backend (DATA_DAEMON_TYPE=rust) needs more testing
- ❌ update() and delete() methods still todo!()

**Current State**: System is **production-ready with sqlite backend**. Rust backend architecture is in place but needs additional testing and worker lifecycle integration.
