# Archive Worker Design - Rust Worker Skeleton Prototype

## Goal
Prove Rust worker can receive archive tasks, manage queue, and call back to DataDaemon via Commands.execute().

## Architecture (Modeled on LoggerDaemon)

```
┌──────────────────────────────────────┐
│ ArchiveDaemonServer (TypeScript)     │
│ - Discovers @Archive entities        │
│ - Monitors table sizes               │
│ - Sends archive tasks to Rust        │
│ - Health checks, reconnection        │
└────────────┬─────────────────────────┘
             │ Unix Socket
             │ Message: { command: 'archive', collection, ... }
             ↓
┌──────────────────────────────────────┐
│ ArchiveWorker (Rust)                 │
│ - Unix socket server                 │
│ - FIFO task queue                    │
│ - Thread pool (process tasks)        │
│ - IPC client to call Commands        │
└────────────┬─────────────────────────┘
             │ IPC (via socket back to TypeScript)
             │ Commands.execute('data/list', {...})
             ↓
┌──────────────────────────────────────┐
│ DataDaemon (TypeScript)              │
│ - Entity CRUD operations             │
│ - Schema management                  │
└──────────────────────────────────────┘
```

## Skeleton Prototype Messages

### Phase 1: Basic Connectivity

**TypeScript → Rust**:
```json
{
  "command": "ping"
}
```

**Rust → TypeScript**:
```json
{
  "status": "success",
  "message": "pong"
}
```

### Phase 2: Task Queuing

**TypeScript → Rust** (queue archive task):
```json
{
  "command": "archive",
  "taskId": "task-001",
  "collection": "chat_messages",
  "sourceHandle": "primary",
  "destHandle": "archive",
  "maxRows": 10000,
  "batchSize": 1000
}
```

**Rust → TypeScript** (acknowledge):
```json
{
  "status": "queued",
  "taskId": "task-001",
  "queuePosition": 3
}
```

### Phase 3: Task Execution (Rust calls back to TypeScript)

**Rust → TypeScript** (execute data command):
```json
{
  "command": "execute",
  "requestId": "req-001",
  "targetCommand": "data/list",
  "params": {
    "collection": "chat_messages",
    "dbHandle": "primary",
    "limit": 1000,
    "orderBy": [{"field": "createdAt", "direction": "asc"}]
  }
}
```

**TypeScript → Rust** (command result):
```json
{
  "requestId": "req-001",
  "status": "success",
  "result": {
    "items": [...],
    "count": 1000
  }
}
```

### Phase 4: Progress Updates

**Rust → TypeScript** (progress update):
```json
{
  "command": "progress",
  "taskId": "task-001",
  "archived": 500,
  "total": 5000,
  "status": "in_progress"
}
```

### Phase 5: Task Completion

**Rust → TypeScript** (task complete):
```json
{
  "command": "complete",
  "taskId": "task-001",
  "archived": 5000,
  "duration": 1234,
  "status": "success"
}
```

## Component Files to Create

### TypeScript Side

1. **`daemons/archive-daemon/server/ArchiveWorkerClient.ts`**
   - Like LoggerWorkerClient
   - Unix socket client
   - Sends archive tasks to Rust
   - Handles command execution callbacks from Rust

2. **`daemons/archive-daemon/server/ArchiveDaemonServer.ts`** (refactor)
   - Connect to Rust worker on start
   - Queue tasks instead of synchronous execution
   - Handle progress updates
   - Health checks

### Rust Side

3. **`shared/ipc/archive-worker/src/main.rs`**
   - Unix socket server
   - Task queue (VecDeque)
   - Thread pool for concurrent processing
   - IPC client to call back to TypeScript

4. **`shared/ipc/archive-worker/src/queue.rs`**
   - FIFO task queue
   - Task priority (optional)

5. **`shared/ipc/archive-worker/src/executor.rs`**
   - Execute archive tasks
   - Call Commands.execute() via IPC
   - Copy-verify-delete pattern

## Skeleton Prototype Plan

### Test 1: Connectivity
- TypeScript connects to Rust worker
- Send ping, receive pong
- **Success**: Connection established

### Test 2: Task Queuing
- TypeScript queues 3 archive tasks
- Rust acknowledges and reports queue size
- **Success**: Queue working

### Test 3: Command Callback
- Rust calls Commands.execute('ping') via TypeScript
- TypeScript executes and returns result
- **Success**: Bidirectional communication working

### Test 4: Simple Archive
- Queue task: archive 10 rows from chat_messages
- Rust calls data/list, data/create, data/delete via TypeScript
- TypeScript executes through DataDaemon
- **Success**: End-to-end archive working

### Test 5: Concurrent Tasks
- Queue 3 archive tasks (different collections)
- Rust processes concurrently (thread pool)
- **Success**: Concurrent processing working

## Implementation Order

1. **Investigate LoggerDaemon** - Check if it actually works yet
2. **Create Rust skeleton** - Basic socket server + echo messages
3. **Create TypeScript client** - ArchiveWorkerClient connects to Rust
4. **Test connectivity** - Ping/pong
5. **Add task queue** - Queue tasks in Rust
6. **Add command callback** - Rust calls Commands.execute() via TypeScript
7. **Implement simple archive** - 10 rows only
8. **Add concurrency** - Thread pool
9. **Full integration** - Replace synchronous ArchiveDaemon logic

## Key Design Decisions

### 1. Rust Never Knows About Entities
- Rust only knows: collection name, handle names, batch size
- DataDaemon handles serialization/deserialization
- Clean separation of concerns

### 2. Bidirectional Communication
- TypeScript → Rust: Archive tasks
- Rust → TypeScript: Command execution requests
- This is different from LoggerDaemon (one-way)

### 3. FIFO Queue
- Simple VecDeque<ArchiveTask>
- No priority (for now)
- Process oldest task first

### 4. Thread Pool Size
- Start with 3 threads (3 concurrent collections)
- Configurable based on system resources

### 5. Health Checks
- Like LoggerDaemon
- Ping every 30 seconds
- Auto-reconnect on failure

## Success Criteria

**Skeleton is proven when**:
1. TypeScript can queue archive tasks to Rust
2. Rust can call Commands.execute() back through TypeScript
3. DataDaemon executes commands and returns results
4. Rust successfully archives 10 rows from chat_messages
5. No database thrashing (non-blocking)

## Open Questions

1. **Does LoggerDaemon actually work yet?**
   - Check if LoggerWorkerClient is implemented
   - Test if Logger.ts actually routes to Rust
   - Verify Unix socket communication works

2. **How does Logger.ts call Rust?**
   - Same pattern for ArchiveWorker
   - Reuse IPC client?

3. **Command callback architecture?**
   - How does Rust call Commands.execute()?
   - Same socket? Different socket?
   - Request/response pattern?

## Next Step

**Investigate LoggerDaemon** - Verify it's a working reference implementation before modeling ArchiveWorker on it.
