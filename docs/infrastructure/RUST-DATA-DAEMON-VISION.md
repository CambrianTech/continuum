# Rust DataDaemon Vision - Unified Data Layer

**Goal**: Rust DataDaemon becomes the core data layer, called by BOTH TypeScript and Rust workers.

## Architecture Evolution

### Phase 1: Current State (ArchiveWorker Skeleton)

```
┌─────────────────────────────────┐
│ TypeScript DataDaemon           │
│ - Entity registry               │
│ - Schema management             │
│ - SQL operations                │
└────────────┬────────────────────┘
             ↓ Direct SQL
        ┌────────────┐
        │  SQLite    │
        └────────────┘
             ↑ Direct rusqlite (bypass TS)
┌────────────┴────────────────────┐
│ Rust ArchiveWorker              │
│ - Uses DirectSqliteAdapter      │
│ - Temporary direct access       │
└─────────────────────────────────┘
```

**Status**: Archive worker bypasses TypeScript DataDaemon for performance.

**Problem**: Duplicates data logic, no entity validation in Rust.

---

### Phase 2: TypeScript Calls Rust Adapter (Migration)

```
┌─────────────────────────────────┐
│ TypeScript DataDaemon           │
│ - Entity registry (TS)          │
│ - Calls Rust for SQL            │
└────────────┬────────────────────┘
             ↓ IPC
┌────────────┴────────────────────┐
│ Rust Data Adapter               │
│ - SQL operations                │
│ - Performance layer             │
└────────────┬────────────────────┘
             ↓ rusqlite
        ┌────────────┐
        │  SQLite    │
        └────────────┘
             ↑ Still direct (temporary)
┌────────────┴────────────────────┐
│ Rust ArchiveWorker              │
│ - Uses DirectSqliteAdapter      │
└─────────────────────────────────┘
```

**Status**: TypeScript gets Rust performance, workers still direct.

**Benefits**: TypeScript DataDaemon faster, but workers still separate.

---

### Phase 3: UNIFIED - Rust DataDaemon Core (Future)

```
┌─────────────────────────────────┐
│ TypeScript DataDaemon (wrapper) │
│ - Thin API layer                │
│ - Backward compatibility        │
└────────────┬────────────────────┘
             ↓ Calls Rust library
┌────────────┴────────────────────┐
│ Rust DataDaemon Core (SHARED)  │◄──────┐
│ - Entity registry (Rust)        │       │ Direct library call
│ - Schema management             │       │ (NO IPC!)
│ - SQL operations                │       │
│ - Multi-handle support          │       │
│ - Archive management            │       │
└────────────┬────────────────────┘       │
             ↓ rusqlite                   │
        ┌────────────┐                    │
        │  SQLite    │                    │
        └────────────┘                    │
                                          │
┌─────────────────────────────────────────┤
│ Rust ArchiveWorker                      │
│ - Uses RustDataDaemonAdapter            │
│ - Shared crate with DataDaemon          │
│ - Zero IPC overhead                     │
└─────────────────────────────────────────┘
```

**Status**: Single source of truth, called by both TS and Rust.

**Benefits**:
- TypeScript: Fast (Rust backend)
- Rust workers: Zero-copy access (shared library)
- No code duplication
- Single entity registry
- Consistent validation

---

## Implementation Strategy

### Step 1: Archive Worker Skeleton (NOW)
**Goal**: Prove queue-based Rust worker pattern

```rust
// workers/archive/src/data_adapter.rs
pub trait DataAdapter {
    fn list_rows(&self, ...) -> Result<Vec<Value>, String>;
    fn insert_row(&self, ...) -> Result<(), String>;
    fn delete_row(&self, ...) -> Result<(), String>;
}

// Phase 1 implementation
pub struct DirectSqliteAdapter { /* rusqlite */ }

// Phase 3 placeholder
pub struct RustDataDaemonAdapter { /* TODO */ }
```

**Status**: Use DirectSqliteAdapter, design trait for future.

### Step 2: Extract Rust Data Crate
**Goal**: Create `jtag-data-daemon` crate

```rust
// New crate: jtag-data-daemon/src/lib.rs
pub struct DataDaemon {
    entity_registry: EntityRegistry,
    handles: HashMap<String, DatabaseHandle>,
}

impl DataDaemon {
    pub fn list(&self, collection: &str, handle: &str, limit: usize) -> Result<Vec<Value>> {
        // SQL operations
    }

    pub fn create(&self, collection: &str, handle: &str, data: &Value) -> Result<()> {
        // Validation + SQL
    }

    pub fn delete(&self, collection: &str, handle: &str, id: &str) -> Result<()> {
        // SQL operations
    }
}
```

**Dependencies**:
- `rusqlite` - SQL operations
- `serde_json` - JSON handling
- Entity schemas (migrate from TypeScript)

### Step 3: TypeScript Wrapper
**Goal**: TypeScript DataDaemon calls Rust crate

```typescript
// daemons/data-daemon/server/DataDaemonServer.ts
import { DataDaemonRust } from '../../../rust-bindings/data-daemon';

export class DataDaemonServer extends DataDaemon {
  private rustDaemon: DataDaemonRust;

  async list(params: DataListParams): Promise<DataListResult> {
    // Call Rust via N-API bindings
    return this.rustDaemon.list(params);
  }
}
```

**Technology**: neon-bindings or napi-rs for Rust ↔ Node.js

### Step 4: Archive Worker Uses Shared Crate
**Goal**: Rust workers call Rust DataDaemon directly

```rust
// workers/archive/Cargo.toml
[dependencies]
jtag-data-daemon = { path = "../../rust/jtag-data-daemon" }

// workers/archive/src/main.rs
use jtag_data_daemon::DataDaemon;

let data_daemon = DataDaemon::new(config)?;

// Direct function call - no IPC!
let rows = data_daemon.list("chat_messages", "primary", 1000)?;
```

**Benefits**:
- Zero IPC overhead (shared memory)
- Type-safe at compile time
- Same validation logic as TypeScript
- No code duplication

---

## Advantages of Unified Rust Core

### 1. Performance
- **TypeScript**: Rust backend = 10-100x faster SQL
- **Rust workers**: No IPC overhead (direct library calls)
- **Memory**: Zero-copy data access between workers and daemon

### 2. Consistency
- **Single entity registry** (Rust)
- **Single validation logic** (Rust)
- **Single SQL layer** (Rust)
- TypeScript and Rust workers see same data

### 3. Maintainability
- **One codebase** for data operations
- **Type-safe** at compile time
- **Easier to test** (Rust has better testing story)
- **Easier to optimize** (profile Rust, optimize once, all benefit)

### 4. Scalability
- **Add new Rust workers** easily (import crate)
- **Add new entities** in one place
- **Optimize SQL** once, all benefit
- **Archive logic** shared between daemon and workers

---

## Migration Path (No Breaking Changes)

### Phase 2a: Gradual TypeScript → Rust Migration

```typescript
// daemons/data-daemon/server/DataDaemonServer.ts

// Toggle per operation
const USE_RUST_FOR_LIST = true;
const USE_RUST_FOR_CREATE = false;  // Not migrated yet

async list(params: DataListParams): Promise<DataListResult> {
  if (USE_RUST_FOR_LIST) {
    return this.rustDaemon.list(params);  // New path
  } else {
    return this.sqliteQuery.list(params);  // Old path (fallback)
  }
}
```

**Benefits**:
- Migrate operation by operation
- A/B test performance
- Roll back if issues
- No system downtime

### Phase 2b: Rust Workers Still Use Direct SQL

```rust
// workers/archive/src/main.rs

// Phase 2: Still uses DirectSqliteAdapter (no changes to worker)
let adapter = DirectSqliteAdapter::new(primary_db, archive_db)?;

// Phase 3: Switch to RustDataDaemonAdapter (one line change!)
let adapter = RustDataDaemonAdapter::new(config)?;
```

**Benefits**: Worker code unchanged, just swap adapter.

---

## Code Organization

```
rust/
├── jtag-data-daemon/          # Shared core crate
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs             # Public API
│   │   ├── entity.rs          # Entity registry
│   │   ├── schema.rs          # Schema management
│   │   ├── query.rs           # SQL operations
│   │   └── handle.rs          # Database handles
│   └── tests/
│       └── integration.rs     # End-to-end tests
│
├── jtag-data-daemon-node/     # Node.js bindings
│   ├── Cargo.toml
│   ├── src/
│   │   └── lib.rs             # napi-rs bindings
│   └── package.json
│
workers/
├── archive/                    # Archive worker (uses shared crate)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   └── data_adapter.rs    # Trait + implementations
│   └── [dependencies: jtag-data-daemon]
│
├── training/                   # Training worker (uses shared crate)
│   └── [dependencies: jtag-data-daemon]
│
└── inference/                  # Inference worker (uses shared crate)
    └── [dependencies: jtag-data-daemon]
```

---

## Timeline

### Phase 1: Archive Worker Skeleton (This Week)
- ✅ Design data_adapter.rs trait
- ✅ Implement DirectSqliteAdapter
- ⏳ Test with 10 rows
- ⏳ Verify concurrent processing

### Phase 2: Rust Data Crate (Next Sprint)
- Extract SQL operations to `jtag-data-daemon` crate
- Implement entity registry in Rust
- Create Node.js bindings (napi-rs)
- TypeScript DataDaemon calls Rust backend

### Phase 3: Unified Core (Future Sprint)
- Archive worker uses `RustDataDaemonAdapter`
- Verify performance (should be same or better)
- Migrate other workers (training, inference)
- Remove DirectSqliteAdapter (deprecated)

---

## Success Criteria

**Phase 1 Complete When**:
- ✅ Archive worker queues tasks from TypeScript
- ✅ Rust archives 10 rows (copy-verify-delete)
- ✅ Concurrent processing works (3 tasks simultaneously)
- ✅ DataAdapter trait designed for future

**Phase 2 Complete When**:
- ✅ `jtag-data-daemon` crate exists
- ✅ TypeScript DataDaemon calls Rust for SQL
- ✅ Performance improved (benchmark vs old)
- ✅ No breaking changes to API

**Phase 3 Complete When**:
- ✅ Archive worker uses `RustDataDaemonAdapter`
- ✅ Zero IPC overhead verified
- ✅ All workers migrated
- ✅ Single source of truth for data operations

---

## Why This is the Right Architecture

**Gradual migration**:
- No big-bang rewrite
- Test each phase independently
- Roll back if needed
- System stays running

**Performance wins**:
- TypeScript gets Rust speed (Phase 2)
- Rust workers get zero-copy (Phase 3)
- Archive worker proves pattern (Phase 1)

**Future-proof**:
- More Rust workers easy to add
- Data layer optimized once, all benefit
- TypeScript gradually becomes orchestration layer
- Core logic moves to Rust (type-safe, fast)

**This is E=mc² for daemons**: Unified data layer unlocks performance for entire system.
