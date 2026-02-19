# Rust Worker Dual-Path Pattern

**Universal pattern for ALL Rust workers**: Two communication paths for different purposes.

## The Two Paths

### Path 1: Data Operations (Fast Path)
**Module**: `data_adapter.rs`

**Purpose**: High-performance data operations (SQL)

**Phase 1 (NOW)**: Direct rusqlite
```rust
let adapter = DirectSqliteAdapter::new(primary_db, archive_db)?;
let rows = adapter.list_rows("chat_messages", "primary", 1000, "createdAt")?;
```

**Phase 3 (FUTURE)**: Shared Rust DataDaemon crate
```rust
let adapter = RustDataDaemonAdapter::new(config)?;
let rows = adapter.list_rows("chat_messages", "primary", 1000, "createdAt")?;
// Zero IPC overhead - direct library call!
```

### Path 2: Command Operations (Universal Path)
**Module**: `command_client.rs`

**Purpose**: Non-SQL operations (events, config, session, etc.)

**Always**: Calls TypeScript Commands.execute()
```rust
let commands = CommandClient::connect("/tmp/command-router.sock")?;

// Emit progress event
commands.emit_event("archive:progress", json!({
    "collection": "chat_messages",
    "archived": 500
}))?;

// Get configuration
let config = commands.get_config("archive.batchSize")?;

// Get session state
let session = commands.get_session(session_id)?;
```

## Complete Worker Example

```rust
use data_adapter::{DataAdapter, DirectSqliteAdapter};
use command_client::CommandClient;

struct ArchiveWorker {
    data: Box<dyn DataAdapter>,      // Fast SQL operations
    commands: CommandClient,          // Universal command access
}

impl ArchiveWorker {
    async fn archive_task(&mut self, task: ArchiveTask) -> Result<usize, String> {
        // 1. Get rows from database (FAST PATH - direct SQL)
        let rows = self.data.list_rows(
            &task.collection,
            &task.source_handle,
            task.batch_size,
            "createdAt"
        )?;

        let mut archived = 0;

        for row in rows {
            let id = row.get("id")
                .and_then(|v| v.as_str())
                .ok_or("Missing id")?;

            // 2. Copy to archive (FAST PATH - direct SQL)
            self.data.insert_row(&task.collection, &task.dest_handle, &row)?;

            // 3. Verify copied (FAST PATH - direct SQL)
            // ... verification logic ...

            // 4. Delete from primary (FAST PATH - direct SQL)
            self.data.delete_row(&task.collection, &task.source_handle, id)?;

            archived += 1;

            // 5. Emit progress every 100 rows (UNIVERSAL PATH - command)
            if archived % 100 == 0 {
                self.commands.emit_event("archive:progress", json!({
                    "taskId": task.task_id,
                    "collection": task.collection,
                    "archived": archived,
                    "total": rows.len()
                }))?;
            }
        }

        // 6. Emit completion (UNIVERSAL PATH - command)
        self.commands.emit_event("archive:complete", json!({
            "taskId": task.task_id,
            "collection": task.collection,
            "archived": archived,
            "duration": task.elapsed_ms()
        }))?;

        Ok(archived)
    }
}
```

## Why Two Paths?

### Fast Path (DataAdapter)
**Optimize for throughput**:
- SQL operations are 80% of work
- No IPC overhead (direct or shared library)
- Batch operations efficient
- Type-safe at compile time

**Evolution**:
- Phase 1: Direct rusqlite (good enough)
- Phase 3: Shared Rust DataDaemon (better - single source of truth)

### Universal Path (CommandClient)
**Optimize for flexibility**:
- Events, config, session are 20% of work
- Full system access (any command)
- TypeScript orchestration layer
- Backward compatible

**Stable**:
- Always calls TypeScript Commands.execute()
- Works regardless of DataDaemon implementation
- No changes needed when DataDaemon migrates to Rust

## Use Cases by Daemon

### ArchiveDaemon
```rust
// Data operations (90% of work) - FAST PATH
data.list_rows()     // Get rows to archive
data.insert_row()    // Copy to archive
data.delete_row()    // Delete from primary

// Events (10% of work) - UNIVERSAL PATH
commands.emit_event("archive:progress")   // Progress updates
commands.emit_event("archive:complete")   // Completion notification
```

### TrainingDaemon
```rust
// Data operations (50% of work) - FAST PATH
data.list_rows("training_examples")  // Get training data
data.insert_row("lora_adapters")     // Save trained model

// Commands (50% of work) - UNIVERSAL PATH
commands.emit_event("training:progress")  // Progress updates
commands.execute("ai/generate")           // Test model after training
commands.get_config("training.batchSize") // Get configuration
```

### InferenceDaemon
```rust
// Data operations (30% of work) - FAST PATH
data.list_rows("chat_messages")      // Get conversation context
data.insert_row("ai_generations")    // Save cost tracking

// Commands (70% of work) - UNIVERSAL PATH
commands.emit_event("inference:complete")  // Response ready
commands.get_session(session_id)           // Get user context
commands.execute("events/emit")            // Notify UI
```

### EmbeddingDaemon
```rust
// Data operations (80% of work) - FAST PATH
data.list_rows("documents")          // Get documents to embed
data.update_row("documents")         // Save embeddings

// Commands (20% of work) - UNIVERSAL PATH
commands.emit_event("embedding:complete")  // Batch complete
commands.get_config("embedding.model")     // Get model config
```

## Benefits of Dual-Path Pattern

### 1. Performance
- **Hot path optimized**: SQL operations fast (no IPC)
- **Cold path flexible**: Commands access entire system
- **Best of both worlds**: Speed + flexibility

### 2. Evolution-Friendly
- **DataAdapter can evolve**: Direct SQL → Rust DataDaemon
- **CommandClient stays stable**: Always calls TypeScript
- **No breaking changes**: Workers swap adapters, that's it

### 3. Testable
```rust
// Mock DataAdapter for testing
struct MockDataAdapter { /* ... */ }
impl DataAdapter for MockDataAdapter { /* ... */ }

// Test with mock (no database needed)
let worker = ArchiveWorker {
    data: Box::new(MockDataAdapter::new()),
    commands: CommandClient::mock(),
};
```

### 4. Reusable
- **Every Rust worker** uses same pattern
- **DataAdapter trait** shared across workers
- **CommandClient** shared across workers
- **Consistent architecture** easy to understand

## File Structure

```
workers/archive/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Worker orchestration
│   ├── messages.rs                # Protocol types
│   ├── data_adapter.rs            # FAST PATH (trait + implementations)
│   │   ├── DataAdapter trait      # Abstract interface
│   │   ├── DirectSqliteAdapter    # Phase 1 implementation
│   │   └── RustDataDaemonAdapter  # Phase 3 implementation
│   └── command_client.rs          # UNIVERSAL PATH (commands)
│       ├── CommandClient          # IPC to TypeScript
│       ├── emit_event()           # Events
│       ├── get_config()           # Config
│       └── get_session()          # Session
```

## Migration Path

### Phase 1: Archive Worker Skeleton (NOW)
```rust
// Use DirectSqliteAdapter
let data_adapter = DirectSqliteAdapter::new(primary_db, archive_db)?;

// CommandClient stubbed (not yet implemented)
let command_client = CommandClient::stub(); // TODO: Implement
```

**Goal**: Prove archive worker works, queue-based, concurrent.

### Phase 2: Add CommandClient
```rust
// Still DirectSqliteAdapter for data
let data_adapter = DirectSqliteAdapter::new(primary_db, archive_db)?;

// Real CommandClient for events
let command_client = CommandClient::connect("/tmp/command-router.sock")?;

// Emit progress events
command_client.emit_event("archive:progress", ...)?;
```

**Goal**: Prove bidirectional communication works.

### Phase 3: Migrate to Rust DataDaemon
```rust
// ONE LINE CHANGE!
let data_adapter = RustDataDaemonAdapter::new(config)?;

// CommandClient unchanged
let command_client = CommandClient::connect("/tmp/command-router.sock")?;
```

**Goal**: Zero-copy data access, single source of truth.

## Code Reuse Across Workers

```rust
// Shared crate: rust/jtag-worker-common
pub mod data_adapter;    // DataAdapter trait + implementations
pub mod command_client;  // CommandClient implementation

// workers/archive/Cargo.toml
[dependencies]
jtag-worker-common = { path = "../../rust/jtag-worker-common" }

// workers/training/Cargo.toml
[dependencies]
jtag-worker-common = { path = "../../rust/jtag-worker-common" }

// workers/inference/Cargo.toml
[dependencies]
jtag-worker-common = { path = "../../rust/jtag-worker-common" }
```

**Every worker** imports same helpers:
- `DataAdapter` trait
- `DirectSqliteAdapter` (Phase 1)
- `RustDataDaemonAdapter` (Phase 3)
- `CommandClient` (always)

## Success Metrics

**Pattern proven when**:
1. ✅ Archive worker uses both paths
2. ✅ Data operations fast (direct SQL)
3. ✅ Commands work (events emitted)
4. ✅ Other workers adopt same pattern
5. ✅ Migration to Rust DataDaemon is one-line change

**System scales when**:
1. ✅ New workers easy to add (copy pattern)
2. ✅ Performance bottlenecks identified (profile once, optimize once)
3. ✅ TypeScript becomes orchestration layer (lightweight)
4. ✅ Rust becomes execution layer (heavyweight)

---

**This is the foundation**: Two-path pattern unlocks Rust workers as first-class citizens while maintaining gradual migration path.
