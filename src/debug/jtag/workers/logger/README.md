# Logger Rust Worker

**Production Rust worker for high-performance logging via Unix domain sockets.**

This is the first Rust worker integrated into JTAG, demonstrating the generic IPC protocol pattern that will be used for future workers (cognition, LoRA, etc.).

## What This Demonstrates

1. **Generic IPC Protocol** - Transport layer doesn't know about worker-specific types
2. **Worker-Owned Schemas** - Logger worker owns `WriteLogPayload`, not the IPC layer
3. **Type-Safe JSON** - serde (Rust) ↔ TypeScript round-trip serialization
4. **Unix Socket Communication** - Newline-delimited JSON over Unix domain sockets
5. **Request/Response Pattern** - Correlation IDs, success/error handling

## Project Structure

```
workers/logger/
├── Cargo.toml                      # Rust dependencies (serde, uuid, chrono)
├── src/
│   ├── main.rs                     # Rust logger worker (listens on socket)
│   └── messages.rs                 # Rust message types (mirrors TypeScript)
├── examples/
│   └── test-client.ts              # Example TypeScript client usage
└── README.md                       # This file
```

## Quick Start

### Step 1: Start the Rust Worker

```bash
cd workers/logger

# Build and run (this will listen on /tmp/logger-worker.sock)
cargo run -- /tmp/logger-worker.sock
```

**Expected output:**
```
🦀 Rust Logger Worker starting...
📡 Listening on: /tmp/logger-worker.sock
✅ Ready to accept connections
```

### Step 2: Run the TypeScript Client (in another terminal)

```bash
cd workers/logger

# Send test log messages
npx tsx examples/test-client.ts
```

**Expected output:**
```
📡 TypeScript Test Client Starting...
🔌 Connecting to: /tmp/logger-worker.sock
✅ Connected to Rust worker

📤 Sending message 1/4:
   Level: info
   Category: sql
   Message: Database connection established

📬 Response 1/4:
   ✅ Success: true
   📊 Bytes written: 67
   🔗 Request ID: a3b2c1d4...

...

✅ All tests passed! Communication working end-to-end.
```

## Message Format

### TypeScript → Rust (Request)

```typescript
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "type": "write-log",
  "timestamp": "2025-12-09T23:45:00.000Z",
  "userId": "test-user-id",
  "payload": {
    "category": "sql",
    "level": "info",
    "component": "DataDaemon",
    "message": "Database connection established"
  }
}
```

### Rust → TypeScript (Response)

```typescript
{
  "id": "660e9500-f39c-52e5-b827-557766551111",
  "type": "write-log",
  "timestamp": "2025-12-09T23:45:00.123Z",
  "requestId": "550e8400-e29b-41d4-a716-446655440000",
  "success": true,
  "payload": {
    "bytesWritten": 67
  }
}
```

## Key Design Principles

### 1. Generic Transport Layer

The IPC protocol (`WorkerMessage<T>`, `WorkerRequest<T>`, `WorkerResponse<T>`) doesn't know about logging, cognition, or LoRA. It just transports JSON with opaque payloads.

**TypeScript:**
```typescript
interface WorkerRequest<T = unknown> {
  id: string;
  type: string;          // Opaque to IPC layer
  payload: T;            // Generic
  // ...
}
```

**Rust:**
```rust
pub struct WorkerRequest<T> {
    pub id: String,
    pub r#type: String,    // Opaque to IPC layer
    pub payload: T,        // Generic
    // ...
}
```

### 2. Workers Own Their Types

Logger worker defines `WriteLogPayload` and `WriteLogResult`. Cognition worker would define `BuildRAGPayload`, etc. IPC layer never imports these.

**Usage:**
```typescript
const request: WorkerRequest<WriteLogPayload> = {
  // IPC layer fields
  id: uuid(),
  type: 'write-log',
  // Worker-specific payload
  payload: {
    category: 'sql',
    level: 'info',
    message: 'Hello'
  }
};
```

### 3. Newline-Delimited JSON

Messages are JSON objects separated by `\n`. This is simpler than length-prefixing and works well for text-based protocols.

```
{"id":"...","type":"write-log",...}\n
{"id":"...","type":"write-log",...}\n
```

## Next Steps (Production Integration)

To integrate into JTAG:

1. **Move types to main codebase**
   - `shared/ipc/WorkerMessages.ts` ✅ (already done)
   - `shared/ipc/logger/LoggerMessageTypes.ts` ✅ (already done)

2. **Create `workers/` directory in JTAG**
   ```
   src/debug/jtag/workers/
   ├── logger/                   # Logger worker
   │   ├── Cargo.toml
   │   ├── src/main.rs
   │   └── src/messages.rs
   ├── cognition/                # RAG/tool execution worker (future)
   └── lora/                     # LoRA training/paging worker (future)
   ```

3. **Integrate into Logger.ts**
   - Replace direct file writes with worker messages
   - Connect to Unix socket on daemon startup
   - Send `WorkerRequest<WriteLogPayload>` instead of writing files

4. **Add worker lifecycle management**
   - Start worker process on daemon startup
   - Monitor health (periodic heartbeat)
   - Restart on crash
   - Graceful shutdown

5. **Performance testing**
   - Benchmark throughput (messages/sec)
   - Measure latency overhead vs direct file I/O
   - Test under load (thousands of log messages)

## Troubleshooting

**Error: `ENOENT: no such file or directory`**
- Make sure the Rust worker is running first
- Check the socket path matches (`/tmp/logger-worker.sock`)

**Error: `ECONNREFUSED`**
- The Rust worker crashed or isn't listening
- Check Rust worker output for errors

**No response from Rust worker**
- Check that messages end with `\n`
- Verify JSON is valid (use `JSON.parse()` to test)
- Look for parse errors in Rust worker output

## Architecture Notes

See `/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/docs/architecture/RUST-WORKER-IPC-PROTOCOL.md` for the full specification of how this will integrate into the production system.

---

**Status:** ✅ Working end-to-end demo
**Next:** Integrate into JTAG Logger.ts
