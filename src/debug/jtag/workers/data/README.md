# Data Worker

**Status**: Architecture Complete, Implementation In Progress
**Type**: Rust Worker (Foundation Layer)
**Purpose**: All database operations off the Node.js main thread

---

## Overview

The **Data Worker** is the foundation of the Universal Worker Protocol architecture. It handles ALL database operations (SQLite) in a separate Rust process, enabling:

- True multi-core utilization (Node.js main thread freed)
- Independent fault boundaries (database crashes don't kill the system)
- Worker-to-worker communication (Chat Drain → Data Worker, AI Provider → Data Worker)
- AI-maintainable infrastructure (personas can monitor/restart)

**Critical**: This worker must be built FIRST since Chat Drain Worker, AI Provider Worker, and all other workers depend on database access.

---

## Architecture

### Queue-Based Processing
```
Node.js Main Thread (fast)
    ↓ Unix Socket
Data Worker (Rust process)
    ↓ Connection Handler (non-blocking)
    ↓ mpsc::channel (queue)
    ↓ Background Processor Thread
    ↓ SQLite Operations:
       - data/list (queries with filters/ordering)
       - data/read (single document by ID)
       - data/create (insert new documents)
       - data/update (modify existing documents)
```

### Universal Protocol Implementation

This worker implements all three required interfaces:

#### 1. Health Check (`ping`)
```json
Request:  { "type": "ping", "id": "uuid" }
Response: {
  "success": true,
  "data": {
    "uptime_ms": 123456,
    "queue_depth": 42,
    "processed_total": 15000,
    "errors_total": 3,
    "memory_mb": 18.5,
    "status": "healthy"
  }
}
```

#### 2. Graceful Shutdown (`shutdown`)
```json
Request:  { "type": "shutdown", "id": "uuid", "data": { "timeout_ms": 10000, "force": false } }
Response: {
  "success": true,
  "data": {
    "queue_drained": 25,
    "shutdown_time_ms": 3450
  }
}
```

#### 3. Status/Diagnostics (`status`)
```json
Request:  { "type": "status", "id": "uuid", "data": { "verbose": true } }
Response: {
  "success": true,
  "data": {
    "worker_type": "data",
    "version": "1.0.0",
    "pid": 12345,
    "uptime_ms": 3600000,
    "status": "healthy",
    "metrics": { "queue_depth": 5, "processed_total": 150000, ... },
    "resources": { "memory_mb": 28.5, "threads": 4, "connections": 3 }
  }
}
```

---

## Data Commands

### 1. data/list - Query with filters and ordering

```json
Request: {
  "type": "data/list",
  "id": "uuid",
  "data": {
    "collection": "users",
    "filter": { "role": "ai" },
    "orderBy": [{ "field": "lastActiveAt", "direction": "desc" }],
    "limit": 50,
    "offset": 0
  }
}

Response: {
  "success": true,
  "data": {
    "items": [ /* array of documents */ ],
    "total": 142,
    "limit": 50,
    "offset": 0
  }
}
```

### 2. data/read - Single document by ID

```json
Request: {
  "type": "data/read",
  "id": "uuid",
  "data": {
    "collection": "users",
    "id": "550e8400-e29b-41d4-a716-446655440000"
  }
}

Response: {
  "success": true,
  "data": {
    "document": { /* single document */ }
  }
}
```

### 3. data/create - Insert new document

```json
Request: {
  "type": "data/create",
  "id": "uuid",
  "data": {
    "collection": "chat_messages",
    "document": {
      "id": "new-uuid",
      "content": "Hello world",
      "senderId": "user-uuid",
      "roomId": "room-uuid",
      "timestamp": 1234567890
    }
  }
}

Response: {
  "success": true,
  "data": {
    "id": "new-uuid",
    "created": true
  }
}
```

### 4. data/update - Modify existing document

```json
Request: {
  "type": "data/update",
  "id": "uuid",
  "data": {
    "collection": "users",
    "id": "user-uuid",
    "updates": {
      "lastActiveAt": 1234567890,
      "status": "online"
    }
  }
}

Response: {
  "success": true,
  "data": {
    "updated": true,
    "modified_count": 2
  }
}
```

---

## File Structure

```
data/
├── src/
│   ├── main.rs               # Socket listener, queue setup, thread spawning
│   ├── connection_handler.rs # Message parsing and routing (universal + data)
│   ├── processor.rs          # Background data operations (SQLite)
│   ├── health.rs             # Universal protocol implementation (COMPLETE)
│   ├── messages.rs           # Data-specific types with ts-rs
│   └── database.rs           # SQLite connection pool and query builder
├── bindings/                 # Generated TypeScript types
│   ├── PingResult.ts
│   ├── ShutdownResult.ts
│   ├── StatusResult.ts
│   ├── DataListPayload.ts
│   ├── DataListResult.ts
│   ├── DataReadPayload.ts
│   ├── DataReadResult.ts
│   ├── DataCreatePayload.ts
│   ├── DataCreateResult.ts
│   ├── DataUpdatePayload.ts
│   └── DataUpdateResult.ts
├── Cargo.toml               # Dependencies (rusqlite, serde, ts-rs)
└── README.md                # This file
```

---

## Implementation Status

### ✅ Architecture Design Complete
- Universal Protocol specification inherited from chat-drain
- Data command interfaces defined (list/read/create/update)
- TypeScript bindings design complete
- SQLite integration strategy defined

### 🚧 In Progress
- SQLite connection pool
- Query builder for filters and ordering
- Background processor implementation
- TypeScript binding generation

### 📋 Planned
- Connection pooling optimization
- Transaction support
- Batch operations
- Database migrations

---

## Usage

### Building
```bash
cd workers/data
cargo build --release
```

### Running
```bash
cargo run --release -- /tmp/data-worker.sock
```

### Testing Protocol
```bash
# Health check
echo '{"type":"ping","id":"test-1"}' | nc -U /tmp/data-worker.sock

# Status (verbose)
echo '{"type":"status","id":"test-2","data":{"verbose":true}}' | nc -U /tmp/data-worker.sock

# List users
echo '{"type":"data/list","id":"test-3","data":{"collection":"users","limit":10}}' | nc -U /tmp/data-worker.sock

# Read single user
echo '{"type":"data/read","id":"test-4","data":{"collection":"users","id":"user-uuid"}}' | nc -U /tmp/data-worker.sock

# Graceful shutdown
echo '{"type":"shutdown","id":"test-5","data":{"timeout_ms":5000,"force":false}}' | nc -U /tmp/data-worker.sock
```

---

## Integration with Node.js

### TypeScript Client (To Be Created)

```typescript
// system/workers/data/DataClient.ts
export class DataClient {
  private socket: net.Socket;

  async ping(): Promise<PingResult> {
    return this.sendMessage({ type: 'ping', id: uuid() });
  }

  async list<T>(collection: string, options?: {
    filter?: Record<string, any>,
    orderBy?: Array<{ field: string, direction: 'asc' | 'desc' }>,
    limit?: number,
    offset?: number
  }): Promise<DataListResult<T>> {
    return this.sendMessage({
      type: 'data/list',
      id: uuid(),
      data: { collection, ...options }
    });
  }

  async read<T>(collection: string, id: string): Promise<DataReadResult<T>> {
    return this.sendMessage({
      type: 'data/read',
      id: uuid(),
      data: { collection, id }
    });
  }

  async create<T>(collection: string, document: T): Promise<DataCreateResult> {
    return this.sendMessage({
      type: 'data/create',
      id: uuid(),
      data: { collection, document }
    });
  }

  async update(collection: string, id: string, updates: Record<string, any>): Promise<DataUpdateResult> {
    return this.sendMessage({
      type: 'data/update',
      id: uuid(),
      data: { collection, id, updates }
    });
  }

  async shutdown(timeout = 10000): Promise<ShutdownResult> {
    return this.sendMessage({
      type: 'shutdown',
      id: uuid(),
      data: { timeout_ms: timeout, force: false }
    });
  }

  async status(verbose = false): Promise<StatusResult> {
    return this.sendMessage({
      type: 'status',
      id: uuid(),
      data: { verbose }
    });
  }
}
```

### Integration Points
- **DataDaemon**: Replace in-memory queries with Data Worker calls
- **Chat Drain Worker**: Query chat history via Data Worker
- **AI Provider Worker**: Store AI responses via Data Worker
- **All commands**: Replace direct database access with worker calls

---

## Performance Characteristics

### Fast Path (Non-Blocking)
- **Connection handling**: < 1ms (spawns thread)
- **Message queuing**: < 1ms (mpsc::send)
- **Response time**: < 5ms total (queue + ack)

### Slow Path (Background)
- **Simple read**: ~1-2ms (indexed lookup)
- **Filtered list**: ~5-20ms (depends on filter complexity)
- **Create**: ~2-5ms (insert + indexes)
- **Update**: ~2-5ms (update + indexes)

### Throughput
- **Queue depth**: Unbounded (monitor for backpressure)
- **Concurrent connections**: Thread-per-connection
- **Processing rate**: ~1000 ops/sec (depends on query complexity)
- **Connection pool**: 10 connections (configurable)

---

## Monitoring & Maintenance

### AI Persona Integration

AI personas can act as database mechanics:

```bash
# Check worker health
./jtag worker/health --worker=data

# View detailed status
./jtag worker/status --worker=data --verbose

# Restart if unhealthy
./jtag worker/restart --worker=data
```

**Via Tools**:
```xml
<tool_name="worker/health">
  <param name="worker">data</param>
</tool_name>
```

### Status Interpretation

**Healthy** (`status: "healthy"`):
- Queue depth < 800
- Error rate < 5%
- Processing normally
- All connections active

**Degraded** (`status: "degraded"`):
- Queue depth > 800 (backpressure building)
- Connection pool near capacity
- Still processing but falling behind
- Consider scaling (connection pool increase)

**Failing** (`status: "failing"`):
- Error rate > 5%
- Repeated database errors
- Connection pool exhausted
- Requires restart or investigation

---

## SQLite Strategy

### Database Location
```
.continuum/sessions/user/shared/*/databases/
├── continuum.db              # Main database (existing)
└── continuum.db-wal          # Write-ahead log
```

### Connection Pool
- **Size**: 10 connections (configurable)
- **Timeout**: 30 seconds for connection acquisition
- **Busy timeout**: 5 seconds for locked database
- **WAL mode**: Enabled for concurrent reads

### Query Builder
```rust
// Example: Build query from filters
let mut query = String::from("SELECT * FROM users WHERE 1=1");
if let Some(role) = filter.get("role") {
    query.push_str(&format!(" AND role = '{}'", role));
}
if let Some(order_by) = params.order_by {
    query.push_str(&format!(" ORDER BY {} {}", order_by.field, order_by.direction));
}
query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));
```

### Error Handling
- **SQLITE_BUSY**: Retry with exponential backoff (up to 3 attempts)
- **SQLITE_LOCKED**: Wait on connection pool
- **SQLITE_CORRUPT**: Log critical error, notify system
- **Schema errors**: Graceful degradation with error response

---

## Future Enhancements

### Phase 1: Core Operations (Current)
- Implement all four data commands
- Connection pooling
- Basic error handling

### Phase 2: Advanced Features
- **Transactions**: Multi-operation atomic commits
- **Batch operations**: Array of creates/updates in single message
- **Streaming results**: Large queries streamed via chunks
- **Full-text search**: SQLite FTS5 integration

### Phase 3: Performance Optimization
- **Query caching**: LRU cache for frequent queries
- **Prepared statements**: Reuse compiled queries
- **Connection affinity**: Thread-local connections
- **Metrics export**: Prometheus endpoint

### Phase 4: Worker-to-Worker Communication
- **Chat Drain → Data**: Direct Unix socket communication
- **AI Provider → Data**: Persist responses directly
- **Logger → Data**: Store logs via worker (not direct writes)

---

## Reference for Future Workers

This worker serves as the **foundation layer** for:
- **Chat Drain Worker**: Uses Data Worker for message queries/storage
- **AI Provider Worker**: Uses Data Worker for response persistence
- **Logger Worker**: Could use Data Worker for structured log storage

**Pattern**: All workers that need database access should use Data Worker instead of direct SQLite access.

---

## Philosophy

> "The foundation must be solid before building towers."

The Data Worker embodies this philosophy:
- **Isolated**: Database crashes don't affect Node.js or other workers
- **Observable**: Full visibility into query performance and health
- **Controllable**: Can be shut down gracefully (drains queue first)
- **Self-healing**: Can be restarted automatically without data loss
- **AI-maintainable**: AI personas can monitor and fix database issues
- **Universal interface**: Same protocol as all other workers

This is the foundation that enables an autonomous, self-healing, multi-worker architecture.

---

## Dependency Chain

```
Data Worker (foundation)
    ↓
Chat Drain Worker (uses Data Worker for RAG context)
    ↓
AI Provider Worker (uses Data Worker for responses)
    ↓
All other workers (use Data Worker for persistence)
```

**Critical**: Data Worker must be stable and complete before other workers can be fully implemented.
