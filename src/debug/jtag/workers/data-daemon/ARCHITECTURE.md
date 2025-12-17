# RustDataDaemon - Storage-Aware Concurrent Data Layer

## Problem Solved

**Before**: Database crashes due to uncoordinated SQLite access + WAL mode on SD card
- Multiple processes holding locks
- WAL mode terrible on SD cards (high write latency, weak flush guarantees)
- APFS + SD card + WAL = stalls, missing commits, apparent corruption

**After**: Automatic storage detection + appropriate concurrency strategies
- Single coordinator for all database adapters
- Storage-aware pragma configuration
- No lock contention, no crashes

---

## Architecture Overview

```
TypeScript DataDaemon → Unix Socket → RustDataDaemon
                                       ├── detect_storage_type()
                                       ├── get_sqlite_pragmas()
                                       └── AdapterRegistry
                                            ├── SqliteStrategy (storage-aware)
                                            ├── PostgresStrategy (connection pool)
                                            └── JsonStrategy (file locks)
```

---

## Storage Detection (The Key Innovation)

**Automatic configuration based on storage characteristics:**

```rust
fn detect_storage_type(path: &Path) -> StorageType {
    // Check if on external volume (macOS)
    if path.starts_with("/Volumes/") {
        // Use diskutil to check if removable
        let info = Command::new("diskutil")
            .args(&["info", volume_name])
            .output();

        // Check for SD card
        if info.contains("Removable Media: Removable") {
            return StorageType::SDCard;
        }

        // Check for external SSD
        if info.contains("Solid State: Yes") {
            return StorageType::ExternalSSD;
        }
    }

    // Internal drive
    StorageType::InternalSSD
}
```

**Result**: User moves DB anywhere, system automatically uses correct mode!

---

## Pragma Configuration by Storage Type

### Internal SSD (`$HOME/.continuum/data/`)
```sql
PRAGMA journal_mode=WAL;           -- Fast + concurrent
PRAGMA synchronous=NORMAL;         -- Balance safety/speed
PRAGMA temp_store=MEMORY;          -- Reduce disk I/O
PRAGMA locking_mode=EXCLUSIVE;     -- Single writer
PRAGMA busy_timeout=5000;          -- Wait for locks
```
**Why**: Internal SSDs are fast, can handle WAL's frequent fsyncs and concurrent reads/writes.

### External SSD
```sql
PRAGMA journal_mode=WAL;           -- Still OK for external SSD
PRAGMA synchronous=NORMAL;
PRAGMA wal_autocheckpoint=1000;    -- More aggressive checkpointing
PRAGMA temp_store=MEMORY;
PRAGMA busy_timeout=5000;
```
**Why**: External SSDs are slower but still reliable for WAL with checkpointing.

### SD Card / HDD / Unknown
```sql
PRAGMA journal_mode=DELETE;        -- Rollback journal (reliable)
PRAGMA synchronous=NORMAL;         -- Not FULL (too many fsyncs)
PRAGMA temp_store=MEMORY;          -- Keep temp off slow media
PRAGMA locking_mode=EXCLUSIVE;     -- Single writer
PRAGMA busy_timeout=5000;
```
**Why**: SD cards/HDDs are terrible for WAL:
- High write latency → stalls
- Poor random I/O → slow checkpoints
- Weak flush guarantees → data loss risk
- APFS copy-on-write → metadata overhead

DELETE mode is slower but **reliable** on weak storage.

---

## Recommended Database Locations

### Primary Database
**Location**: `$HOME/.continuum/data/database.sqlite`
- Internal SSD → WAL mode
- Multi-writer support
- Fast concurrent access
- Reliable

### Archive Databases
**Default**: `$HOME/.continuum/data/archives/database-001.sqlite`
**Override**: `config.env` DATASETS path (can point to SD card)

**Example**:
- Internal archives: WAL mode (fast)
- SD card archives: DELETE mode (reliable)
- System detects automatically!

---

## Concurrency Strategy

### SQLite (Single Writer Queue)
```rust
struct SqliteStrategy {
    writer_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
    connection: Arc<Mutex<rusqlite::Connection>>,
}

// Writes are serialized (prevents lock contention)
fn execute_write(&self, query: &str) -> Result<Value, String> {
    let mut queue = self.writer_queue.lock().unwrap();
    queue.push_back(write_op);

    // Process serially - NO LOCK CONTENTION!
    while let Some(op) = queue.pop_front() {
        self.execute_immediate(op).await?;
    }
}

// Reads can run in parallel (WAL mode allows this on SSD)
fn execute_read(&self, query: &str) -> Result<Value, String> {
    // Many readers can run simultaneously
    self.execute_immediate(read_op).await?;
}
```

### Postgres (Connection Pool - Future)
```rust
struct PostgresStrategy {
    pool: deadpool_postgres::Pool,  // Full concurrency
}
```

### JSON (File-Level Locking)
```rust
struct JsonStrategy {
    file_locks: HashMap<PathBuf, Arc<Mutex<()>>>,
}
```

---

## Handle-Based API (Like TextureId)

**Opaque handles for resource management:**

```rust
#[derive(Serialize, Deserialize)]
struct AdapterHandle(Uuid);
```

**Pattern**:
1. Client: `adapter/open` → get handle
2. Client: Use handle for all operations
3. RustDataDaemon: Manages actual connections + concurrency

**Benefits**:
- Separation of concerns (never bypass coordination)
- Future optimization: handle → direct Rust access
- Clean resource lifecycle

---

## Communication Pattern

**Unix socket + JSON lines (same as ArchiveWorker)**:

```typescript
// TypeScript
const handle = await daemonClient.send({
  command: 'adapter/open',
  config: {
    adapter_type: 'sqlite',
    connection_string: '$HOME/.continuum/data/database.sqlite'
  }
});
```

```rust
// Rust receives, detects storage, opens with correct pragmas
fn handle_request(&self, request: Request) -> Response {
    match request {
        Request::AdapterOpen { config } => {
            let storage_type = detect_storage_type(&config.connection_string);
            let pragmas = get_sqlite_pragmas(storage_type, false);
            // Open and configure automatically!
        }
    }
}
```

---

## Migration Path

### Phase 1: Standalone Testing (Current)
- RustDataDaemon runs independently
- Test storage detection
- Verify pragma configuration
- No integration with DataDaemon yet

### Phase 2: Parallel Deployment
- TypeScript DataDaemon calls RustDataDaemon for specific operations
- Flag: `USE_RUST_DATA_DAEMON=true` for testing
- Migrate ArchiveWorker to use RustDataDaemon
- Monitor stability

### Phase 3: Full Migration
- All database operations through RustDataDaemon
- TypeScript DataDaemon becomes thin facade
- Type safety via ts-rs
- Full concurrency control in Rust

---

## Testing

### Storage Detection Test
```bash
# SD card
$ ./test-data-daemon.ts
🔍 Detected storage type: SDCard
✅ SQLite adapter opened (DELETE mode - SD card/HDD reliable)

# Internal SSD
$ ./test-data-daemon.ts
🔍 Detected storage type: InternalSSD
✅ SQLite adapter opened (WAL mode - internal SSD optimized)
```

### Verification
```bash
$ diskutil info /Volumes/SlimGordon | grep "Removable"
Removable Media: Removable  ← SD card detected correctly
```

---

## Key Learnings

1. **Never hardcode storage assumptions** - detect and adapt
2. **WAL mode is NOT always better** - depends on storage type
3. **SD cards are weak storage** - reliability over performance
4. **APFS + SD + WAL = disaster** - use DELETE mode
5. **Separation of concerns is sacred** - never bypass coordination layer
6. **Handle pattern scales** - from TextureId to database handles

---

## References

- **ArchiveWorker**: `workers/archive/src/main.rs` (same communication pattern)
- **User's AR experience**: Handle-based pattern from Unity ↔ C++ video frames
- **SQLite docs**: https://www.sqlite.org/pragma.html
- **WAL mode gotchas**: https://www.sqlite.org/wal.html

---

## Future Enhancements

1. **Multi-writer detection**: Pass `multi_writer: bool` to `get_sqlite_pragmas()`
2. **Postgres adapter**: Connection pooling for true concurrent writes
3. **Manual checkpointing**: `PRAGMA wal_checkpoint(TRUNCATE)` before shutdown
4. **Backpressure**: Reject requests when queue > MAX_QUEUE_SIZE
5. **Metrics**: Track queue depth, operation latency per storage type
6. **Config overrides**: Allow manual pragma specification in `config.env`

---

**Bottom line**: Storage-aware concurrency prevents database crashes and adapts automatically to wherever the user puts their data.
