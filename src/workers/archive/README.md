# Archive Worker - Minimal Skeleton Template

**Purpose**: Prove bidirectional TypeScript ↔ Rust communication pattern.

**This is a TEMPLATE** - kept minimal and clean for generator to copy.

## What This Proves

1. **TypeScript → Rust**: Queue tasks via Unix socket
2. **Rust → TypeScript**: Call Commands.execute() for data/config/events
3. **Rust processes**: Worker thread pool, FIFO queue
4. **TypeScript receives**: Results via socket responses

## Flow Example

```
TypeScript ArchiveDaemon:
  "Please archive chat_messages collection"
    ↓ Send via socket
Rust ArchiveWorker:
  Receives task → Queues in FIFO
  Worker thread picks up task
  "Let me check how many rows exist first"
    ↓ Call Commands.execute('data/list', {limit: 0})
TypeScript DataDaemon:
  Executes query → Returns count: 1000
    ↓ Return via socket
Rust ArchiveWorker:
  "OK, 1000 rows to archive"
  Returns success to TypeScript
    ↓ Send via socket
TypeScript ArchiveDaemon:
  "Task queued successfully"
```

## Key Files (Template)

### Rust Side
- `main.rs` - Socket server, worker pool, queue (TEMPLATE)
- `messages.rs` - Protocol types (TEMPLATE)
- `command_client.rs` - Calls TypeScript commands (TEMPLATE)

### TypeScript Side
- `ArchiveWorkerClient.ts` - Sends tasks to Rust (TEMPLATE)
- `CommandRouter.ts` - Handles Rust command callbacks (NEW - TEMPLATE)

## Build & Test

```bash
# Build Rust worker
cd workers/archive && cargo build --release

# Start worker
./target/release/archive-worker /tmp/archive-worker.sock

# Test from TypeScript
npx tsx tests/archive-worker-skeleton-test.ts
```

## For Generator

Copy this pattern for new Rust workers:
1. Replace "archive" with worker name
2. Update messages.rs with worker-specific types
3. Keep socket/queue/thread pattern identical
4. Keep CommandClient pattern identical

**Simple, clean, working code = easy to generate.**
