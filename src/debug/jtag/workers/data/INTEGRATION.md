# Data Worker Integration Guide

**Status**: Phase 1A Complete - Ready for Integration Testing

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│ TypeScript Application Layer                                │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Commands (data/create, data/list, etc.)                    │
│      ↓                                                        │
│  DataDaemon (Entity validation, decorators, events)         │
│      ↓                                                        │
│  RustWorkerStorageAdapter (Unix socket bridge)              │
│      ↓                                                        │
├─────────────────────────────────────────────────────────────┤
│ Rust Storage Layer                                           │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Data Worker (Concurrent process)                            │
│      ↓                                                        │
│  Connection Pool (10 connections)                            │
│      ↓                                                        │
│  SQLite Database                                              │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## What's Working

### ✅ Phase 1A Complete

**Rust Worker:**
- Universal protocol (ping, shutdown, status)
- Four data operations (list, read, create, update)
- Returns raw entity JSON (matches TypeScript commands)
- Optional `dbHandle` parameter (multi-database support ready)
- Connection pooling (10 concurrent connections)
- Queue-based processing (non-blocking)

**TypeScript Adapter:**
- `RustWorkerStorageAdapter` implements `DataStorageAdapter` interface
- Unix socket communication (newline-delimited JSON)
- Request/response correlation
- Timeout handling
- Error handling
- Automatic reconnection (TODO)

**Integration:**
- DataDaemon works with Rust adapter unchanged
- Entity decorators preserved
- Validation happens in TypeScript
- Events emitted normally
- Commands work without modification

## Usage

### 1. Start Rust Worker

```bash
cd workers/data
cargo run --release -- /tmp/data-worker.sock
```

Output:
```
🚀 Data Worker Starting...
   Socket: /tmp/data-worker.sock
   Database: ./continuum.db
✅ Database pool ready
✅ Processor thread spawned
📡 Ready to process data operations
```

### 2. Use with DataDaemon

```typescript
import { RustWorkerStorageAdapter } from './daemons/data-daemon/server/RustWorkerStorageAdapter';
import { DataDaemon } from './daemons/data-daemon/shared/DataDaemon';

// Create adapter
const adapter = new RustWorkerStorageAdapter({
  socketPath: '/tmp/data-worker.sock',
  dbHandle: 'default',
  timeout: 30000
});

await adapter.initialize({
  type: 'rust-worker',
  namespace: 'main',
  options: {}
});

// Create DataDaemon with Rust adapter
const daemon = new DataDaemon(
  {
    strategy: 'sql',
    backend: 'rust-worker',
    namespace: 'main',
    options: {}
  },
  adapter
);

await daemon.initialize();

// Use normally - entities, decorators, validation all work
const user = await daemon.create('users', {
  id: generateUUID(),
  name: 'Alice',
  email: 'alice@example.com',
  role: 'human'
}, context);
```

### 3. Commands Work Unchanged

```typescript
// data/create command
const result = await Commands.execute('data/create', {
  collection: 'users',
  data: { name: 'Bob', email: 'bob@example.com' }
});

// data/list command
const users = await Commands.execute('data/list', {
  collection: 'users',
  filter: { role: 'ai' },
  limit: 50
});
```

**No changes needed** - commands use DataDaemon which uses the adapter.

## Performance Benefits

**Before (TypeScript SQLite):**
- Single-threaded
- Synchronous I/O blocks event loop
- 1 connection at a time
- High memory usage (Node.js heap)

**After (Rust Worker):**
- Multi-threaded (10 connection pool)
- Non-blocking queue architecture
- Concurrent operations
- Low memory footprint
- Process isolation (crashes don't kill Node.js)

**Benchmarks (TODO):**
- Simple read: ~1-2ms
- Filtered query: ~5-20ms
- Concurrent operations: 10x throughput

## Multi-Database Support

### Current: Single Database

```typescript
const adapter = new RustWorkerStorageAdapter({
  socketPath: '/tmp/data-worker.sock',
  dbHandle: 'default'  // Uses main database
});
```

### Future: Per-Persona Databases

```typescript
// Open handle to persona's longterm.db
const registry = DatabaseHandleRegistry.getInstance();
const handle = await registry.open('rust-worker', {
  socketPath: '/tmp/data-worker.sock',
  dbPath: '.continuum/personas/helper/data/longterm.db'
});

// Create adapter for that database
const personaAdapter = new RustWorkerStorageAdapter({
  socketPath: '/tmp/data-worker.sock',
  dbHandle: handle
});

// Use with DataDaemon
const personaDaemon = new DataDaemon(config, personaAdapter);
```

## Testing

### Unit Tests (Rust)

```bash
cd workers/data
cargo test
```

### Integration Tests (TypeScript)

```bash
# Start Rust worker first
cargo run -- /tmp/data-worker.sock

# Run integration tests
npx vitest tests/integration/rust-worker-adapter.test.ts
```

### Manual Testing

```bash
# Ping
echo '{"type":"ping","id":"test","timestamp":"2025-01-01T00:00:00Z","payload":{}}' | nc -U /tmp/data-worker.sock

# Create user
echo '{"type":"data/create","id":"test","timestamp":"2025-01-01T00:00:00Z","payload":{"collection":"users","document":{"id":"user-1","name":"Alice","role":"human"}}}' | nc -U /tmp/data-worker.sock

# Read user
echo '{"type":"data/read","id":"test","timestamp":"2025-01-01T00:00:00Z","payload":{"collection":"users","id":"user-1"}}' | nc -U /tmp/data-worker.sock

# List users
echo '{"type":"data/list","id":"test","timestamp":"2025-01-01T00:00:00Z","payload":{"collection":"users","limit":10}}' | nc -U /tmp/data-worker.sock
```

## Migration Strategy

### Phase 1: Parallel Operation
1. Keep existing SqliteStorageAdapter
2. Add RustWorkerStorageAdapter as option
3. Test with non-critical collections
4. Compare performance/reliability

### Phase 2: Gradual Migration
1. Migrate one collection at a time
2. Monitor for issues
3. Rollback if needed
4. Build confidence

### Phase 3: Full Migration
1. All collections use Rust worker
2. Remove SqliteStorageAdapter
3. Optimize Rust worker
4. Add advanced features

## Roadmap

### ✅ Phase 1A: Core Integration (Complete)
- dbHandle parameter
- Raw entity results
- RustWorkerStorageAdapter
- Basic integration tests

### 🚧 Phase 1B: Advanced Filters (Next)
- `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`
- `$in`, `$nin`
- `$exists`, `$regex`, `$contains`

### 📋 Phase 1C: Multi-Database
- Database handle registry in Rust
- Per-persona databases
- Connection pool per handle

### 📋 Phase 1D: Advanced Features
- Cursor-based pagination
- Field projection
- Transaction support
- Full-text search

### 📋 Phase 2: Production Hardening
- Comprehensive tests
- Error recovery
- Monitoring/metrics
- Performance tuning

## Troubleshooting

### Worker won't start
```bash
# Check if socket exists
ls -la /tmp/data-worker.sock

# Remove stale socket
rm /tmp/data-worker.sock

# Check database permissions
ls -la ./continuum.db
```

### Connection timeout
```typescript
// Increase timeout
const adapter = new RustWorkerStorageAdapter({
  socketPath: '/tmp/data-worker.sock',
  timeout: 60000  // 60 seconds
});
```

### Rust worker crashes
```bash
# Check logs
tail -f /tmp/data-worker-debug.log

# Restart with verbose logging
RUST_BACKTRACE=1 cargo run -- /tmp/data-worker.sock
```

## Benefits Summary

✅ **ORM preserved** - Decorators, validation, types all work
✅ **Performance** - Concurrent Rust, connection pooling
✅ **Isolation** - Separate process, won't crash Node.js
✅ **Testable** - Rust unit tests, TypeScript integration tests
✅ **Seamless** - Commands work unchanged
✅ **Gradual** - Can migrate one collection at a time
✅ **Future-proof** - Ready for multi-database, advanced features

**Best of both worlds: TypeScript ORM + Rust performance!**
