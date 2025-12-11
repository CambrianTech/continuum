# Rust Worker Architecture
## Universal IPC Framework for JTAG System

**Version**: 1.0
**Date**: 2025-12-11
**Status**: Design Complete, Implementation Starting

---

## Table of Contents
1. [Overview](#overview)
2. [Communication Patterns](#communication-patterns)
3. [Message Protocol](#message-protocol)
4. [Worker Types](#worker-types)
5. [Project Structure](#project-structure)
6. [Implementation Phases](#implementation-phases)
7. [Examples](#examples)

---

## Overview

The JTAG system is transitioning from pure TypeScript to a hybrid TS/Rust architecture to overcome single-threaded performance limitations. With 13+ AI personas generating high load, we need:

- **True parallelism** for CPU-intensive work
- **Non-blocking I/O** for LLM API calls
- **Worker-to-worker communication** for composable services
- **Bidirectional streaming** for real-time responses

### Design Principles

1. **Universal Protocol**: One message format (`JTAGProtocol`) for ALL IPC
2. **Backwards Compatible**: Logger worker keeps working during migration
3. **Composable Workers**: Workers can call other workers
4. **Language Agnostic**: Protocol works TS↔Rust, Rust↔Rust, future: Python↔Rust

---

## Communication Patterns

### Pattern 1: Simple Request/Response (Logger)
```
TS → [write-log request] → Logger Worker
                             ↓
                        [writes to file]
                             ↓
TS ← [response: bytes written] ←
```

**Characteristics**:
- One request, one response
- No streaming
- No callbacks
- Synchronous worker logic (threads OK)

### Pattern 2: Bidirectional Streaming (Cognition)
```
TS → [inference request] → Cognition Worker
                             ↓
                        [starts LLM call]
                             ↓
TS ← [token stream chunk 1] ←
TS ← [token stream chunk 2] ←
TS ← [token stream chunk 3] ←
                             ↓
                        [detects tool use]
                             ↓
TS ← [callback: execute tool] ←
  ↓
[executes tool in TS]
  ↓
TS → [tool result] → Cognition Worker
                             ↓
                        [continues inference]
                             ↓
TS ← [token stream chunk 4] ←
TS ← [final response] ←
```

**Characteristics**:
- One request, many streaming responses
- Bidirectional (callbacks from worker to TS)
- Asynchronous worker logic (tokio required)
- Long-lived connection

### Pattern 3: Worker-to-Worker (Events + Logging)
```
TS → [inference] → Cognition Worker
                      ↓
                 [needs logging]
                      ↓
                   Logger Worker
                      ↓
                 [writes log]
                      ↓
                   Cognition Worker
                      ↓
TS ← [response] ←
```

**Characteristics**:
- Worker acts as both server (accepts TS connections) and client (calls Logger)
- Chain of responsibility
- Enables service composition
- Each worker can log without blocking main logic

### Pattern 4: Event Broadcasting (Future)
```
TS → [event] → Event Worker
                 ↓
            [broadcasts to all subscribers]
                 ↓
        ┌────────┼────────┐
        ↓        ↓        ↓
   Cognition  Logger   RAG Worker
   Worker     Worker
```

**Characteristics**:
- One-to-many
- Fire-and-forget (no response)
- Pub/sub pattern
- Decoupled components

---

## Message Protocol

### Base Message Envelope

All IPC uses `JTAGMessage` envelope with tagged union for different patterns:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "messageType")]
pub enum JTAGMessage<T> {
    /// Standard request (expects single response)
    #[serde(rename = "request")]
    Request(JTAGRequest<T>),

    /// Standard response (completes request)
    #[serde(rename = "response")]
    Response(JTAGResponse<T>),

    /// Streaming chunk (partial response, more coming)
    #[serde(rename = "stream")]
    Stream(JTAGStream<T>),

    /// Callback from worker to caller (worker needs caller to do something)
    #[serde(rename = "callback")]
    Callback(JTAGCallback<T>),

    /// Event (fire-and-forget, no response expected)
    #[serde(rename = "event")]
    Event(JTAGEvent<T>),
}
```

### Request (Pattern 1 & 2)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGRequest<T> {
    pub id: String,              // UUID v4
    pub type: String,            // "write-log", "inference", etc.
    pub timestamp: String,       // ISO 8601
    pub payload: T,              // Worker-specific data
    pub user_id: Option<String>,
    pub session_id: Option<String>,
}
```

### Response (Pattern 1)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGResponse<T> {
    pub id: String,
    pub type: String,
    pub timestamp: String,
    pub payload: T,
    pub request_id: String,      // Links to request
    pub success: bool,
    pub error: Option<String>,
    pub error_type: Option<JTAGErrorType>,
    pub stack: Option<String>,
}
```

### Stream (Pattern 2)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGStream<T> {
    pub id: String,
    pub request_id: String,      // Links to original request
    pub sequence: u64,           // Order (0, 1, 2, ...)
    pub timestamp: String,
    pub chunk: T,                // Partial data (e.g., LLM token)
    pub is_final: bool,          // True for last chunk
}
```

### Callback (Pattern 2)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGCallback<T> {
    pub id: String,
    pub request_id: String,      // Original request this relates to
    pub callback_type: String,   // "execute-tool", "get-context", etc.
    pub timestamp: String,
    pub payload: T,
}
```

### Event (Pattern 4)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGEvent<T> {
    pub id: String,
    pub event_type: String,
    pub timestamp: String,
    pub payload: T,
}
```

---

## Worker Types

### Logger Worker (Synchronous, Simple)
- **Purpose**: High-performance log file management
- **Pattern**: Request/Response only
- **Runtime**: Multi-threaded (std::thread)
- **Concurrency**: Per-file locking (Arc<Mutex<File>>)
- **Performance**: 1,300 writes/second, 50+ concurrent files
- **Status**: ✅ Production ready

### Cognition Worker (Asynchronous, Complex)
- **Purpose**: LLM API calls, tool execution, inference streaming
- **Pattern**: Bidirectional streaming + callbacks
- **Runtime**: Tokio async
- **Features**:
  - Stream tokens as they arrive
  - Call back to TS for tool execution
  - Log to Logger Worker (worker-to-worker)
- **Status**: 🚧 Next to implement

### RAG Worker (Asynchronous, Specialized)
- **Purpose**: Embeddings, vector search, semantic retrieval
- **Pattern**: Request/Response + Event emission
- **Runtime**: Tokio async
- **Features**:
  - Generate embeddings (CPU-intensive)
  - Vector similarity search
  - Emit events on new content indexed
- **Status**: 📋 Planned

### Event Worker (Asynchronous, Hub)
- **Purpose**: Central event bus, pub/sub coordination
- **Pattern**: Event broadcasting
- **Runtime**: Tokio async
- **Features**:
  - Subscribe/unsubscribe
  - Topic filtering
  - Delivery guarantees
- **Status**: 📋 Planned

---

## Project Structure

### Workspace Organization
```
workers/
├── Cargo.toml                           # Workspace root
│
├── jtag-protocol/                       # Shared protocol library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                       # Re-exports
│       ├── protocol.rs                  # JTAGMessage enum
│       ├── request.rs                   # JTAGRequest
│       ├── response.rs                  # JTAGResponse
│       ├── stream.rs                    # JTAGStream
│       ├── callback.rs                  # JTAGCallback
│       ├── event.rs                     # JTAGEvent
│       ├── error.rs                     # JTAGErrorType
│       └── health.rs                    # WorkerStats, PingPayload
│
├── jtag-ipc/                            # IPC utilities library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── sync/                        # For simple workers (logger)
│       │   ├── mod.rs
│       │   ├── server.rs
│       │   └── client.rs
│       └── async/                       # For complex workers (cognition)
│           ├── mod.rs
│           ├── server.rs
│           ├── client.rs
│           ├── stream.rs
│           └── callback.rs
│
├── logger-worker/                       # Binary crate (sync)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── file_manager.rs
│       ├── connection_handler.rs
│       ├── health.rs
│       └── messages.rs                  # Logger-specific types
│
├── cognition-worker/                    # Binary crate (async)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── llm_client.rs                # LLM API streaming
│       ├── tool_executor.rs             # Tool callbacks
│       ├── logger_client.rs             # Calls Logger Worker
│       └── messages.rs                  # Cognition-specific types
│
├── rag-worker/                          # Binary crate (async, future)
│   └── ...
│
└── event-worker/                        # Binary crate (async, future)
    └── ...
```

### Workspace Cargo.toml
```toml
[workspace]
members = [
    "jtag-protocol",
    "jtag-ipc",
    "logger-worker",
    "cognition-worker",
    "rag-worker",
    "event-worker",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = "0.4"
uuid = { version = "1.0", features = ["v4"] }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
thiserror = "1.0"
```

### Worker Dependencies

**Simple Worker (Logger)**:
```toml
[dependencies]
jtag-protocol = { path = "../jtag-protocol" }
jtag-ipc = { path = "../jtag-ipc", features = ["sync"] }
# No tokio needed!
```

**Complex Worker (Cognition)**:
```toml
[dependencies]
jtag-protocol = { path = "../jtag-protocol" }
jtag-ipc = { path = "../jtag-ipc", features = ["async"] }
tokio = { workspace = true }
reqwest = { version = "0.11", features = ["stream"] }
```

---

## Implementation Phases

### Phase 1: Protocol Extension ✅ DONE
- [x] Design JTAGMessage enum with all patterns
- [x] Document bidirectional streaming
- [x] Document worker-to-worker communication
- [x] Write this architecture doc

### Phase 2: Create Workspace 🚧 NEXT
1. Create `workers/Cargo.toml` workspace root
2. Create `jtag-protocol/` crate
   - Move `shared/jtag_protocol.rs` → `jtag-protocol/src/protocol.rs`
   - Add Stream, Callback, Event types
   - Add comprehensive tests
3. Create `jtag-ipc/` crate
   - Extract common patterns from logger
   - Create sync module (for logger)
   - Create async module (for cognition)
4. Migrate logger to workspace
   - Rename `logger/` → `logger-worker/`
   - Update to use `jtag-protocol` crate
   - Update to use `jtag-ipc::sync`
   - Verify all tests still pass

### Phase 3: Cognition Worker 📋 PLANNED
1. Create `cognition-worker/` binary crate
2. Implement LLM API client with streaming
3. Implement tool callback pattern
4. Implement logger client (worker-to-worker)
5. Integration tests with TS daemon
6. Load testing (13+ personas)

### Phase 4: Production Deployment 📋 PLANNED
1. Update npm scripts for multi-worker startup
2. Health monitoring for all workers
3. Graceful restart on failure
4. Performance profiling
5. Documentation

---

## Examples

### Example 1: Logger Worker (Simple)
```rust
// logger-worker/src/main.rs
use jtag_protocol::{JTAGRequest, JTAGResponse};
use jtag_ipc::sync::Server;

fn main() {
    let server = Server::new("/tmp/logger-worker.sock");
    server.run(|request: JTAGRequest<WriteLogPayload>| {
        let bytes = file_manager::write_log(&request.payload)?;
        Ok(JTAGResponse::success(
            request.id,
            request.type,
            WriteLogResult { bytes_written: bytes }
        ))
    });
}
```

### Example 2: Cognition Worker (Streaming)
```rust
// cognition-worker/src/main.rs
use jtag_protocol::{JTAGMessage, JTAGRequest, JTAGStream};
use jtag_ipc::async::{Server, Sender};

#[tokio::main]
async fn main() {
    let server = Server::new("/tmp/cognition-worker.sock");
    server.run_streaming(handle_inference).await;
}

async fn handle_inference(
    request: JTAGRequest<InferencePayload>,
    tx: Sender<JTAGMessage>
) -> Result<()> {
    let mut sequence = 0;

    // Stream LLM tokens
    let mut stream = llm_api_client.stream_inference(&request.payload).await?;

    while let Some(token) = stream.next().await {
        tx.send(JTAGMessage::Stream(JTAGStream {
            id: Uuid::new_v4().to_string(),
            request_id: request.id.clone(),
            sequence,
            chunk: TokenChunk { text: token },
            is_final: false,
        })).await?;

        sequence += 1;
    }

    // Send final
    tx.send(JTAGMessage::Stream(JTAGStream {
        // ... is_final: true
    })).await?;

    Ok(())
}
```

### Example 3: Worker-to-Worker (Cognition → Logger)
```rust
// cognition-worker/src/logger_client.rs
use jtag_protocol::{JTAGRequest, JTAGResponse};
use jtag_ipc::async::Client;

pub struct LoggerClient {
    client: Client,
}

impl LoggerClient {
    pub async fn new() -> Result<Self> {
        let mut client = Client::new("/tmp/logger-worker.sock");
        client.connect().await?;
        Ok(Self { client })
    }

    pub async fn log(&self, message: &str) -> Result<()> {
        let request = JTAGRequest {
            id: Uuid::new_v4().to_string(),
            type: "write-log".to_string(),
            payload: WriteLogPayload {
                category: "cognition-worker".to_string(),
                level: LogLevel::Info,
                component: "CognitionWorker".to_string(),
                message: message.to_string(),
                args: None,
            },
            // ...
        };

        self.client.send(request).await?;
        Ok(())
    }
}

// Usage in cognition worker
async fn handle_inference(...) -> Result<()> {
    logger.log("Starting inference").await?;

    // ... do work ...

    logger.log("Inference complete").await?;
    Ok(())
}
```

### Example 4: TypeScript Client (Streaming)
```typescript
// shared/ipc/CognitionWorkerClient.ts
export class CognitionWorkerClient extends WorkerClient {
  async *streamInference(
    prompt: string,
    onToolCallback: (tool: ToolRequest) => Promise<ToolResult>
  ): AsyncGenerator<string> {
    const requestId = randomUUID();

    await this.send({
      messageType: 'request',
      id: requestId,
      type: 'inference',
      payload: { prompt },
    });

    for await (const message of this.listen(requestId)) {
      if (message.messageType === 'stream') {
        yield message.chunk.text;
        if (message.is_final) break;
      }

      if (message.messageType === 'callback') {
        const result = await onToolCallback(message.payload);
        await this.send({
          messageType: 'response',
          request_id: message.id,
          payload: result,
        });
      }
    }
  }
}
```

---

## Performance Goals

| Worker | Throughput | Latency | Concurrency |
|--------|-----------|---------|-------------|
| Logger | 1,000+ writes/sec | <1ms avg | 160+ files |
| Cognition | 10+ inferences/sec | <100ms first token | 13+ personas |
| RAG | 100+ queries/sec | <50ms | Unlimited |
| Event | 10,000+ events/sec | <1ms | 1,000+ subscribers |

---

## Testing Strategy

### Unit Tests
- Each crate has `tests/` directory
- Protocol serialization/deserialization
- Message routing logic
- Error handling

### Integration Tests
- TS ↔ Rust communication
- Rust ↔ Rust communication
- Streaming behavior
- Callback handling

### Load Tests
- 1,000 concurrent writes (logger)
- 13 concurrent inferences (cognition)
- Worker restart resilience
- Memory leak detection

### Validation Commands
```bash
# Build all workers
cargo build --release --workspace

# Test all
cargo test --workspace

# Run logger worker
cargo run --bin logger-worker -- /tmp/logger-worker.sock

# Run cognition worker
cargo run --bin cognition-worker -- /tmp/cognition-worker.sock

# Stress test
npx tsx test-concurrent-load.ts
```

---

## Migration Checklist

- [ ] Phase 1: Protocol Extension (design)
- [ ] Phase 2: Workspace Setup
  - [ ] Create workspace Cargo.toml
  - [ ] Create jtag-protocol crate
  - [ ] Create jtag-ipc crate
  - [ ] Migrate logger to workspace
  - [ ] All logger tests pass
- [ ] Phase 3: Cognition Worker
  - [ ] Basic streaming works
  - [ ] Callback pattern works
  - [ ] Worker-to-worker works
  - [ ] 13 personas don't timeout
- [ ] Phase 4: Production
  - [ ] npm scripts updated
  - [ ] Health monitoring works
  - [ ] Graceful restart works
  - [ ] Performance validated

---

## Future Extensions

### Python Workers
Use same JTAGProtocol over Unix sockets:
```python
import json
import socket

sock = socket.socket(socket.AF_UNIX)
sock.connect('/tmp/embeddings-worker.sock')
request = {"messageType": "request", "type": "embed", ...}
sock.send(json.dumps(request).encode() + b'\n')
```

### GPU Workers
Cognition worker could dispatch to GPU-accelerated workers for local inference.

### Distributed Workers
Replace Unix sockets with TCP for multi-machine deployment.

---

**End of Architecture Document**

This design enables the JTAG system to scale from 13 personas to 100+, with blazing-fast Rust workers handling the heavy lifting while TypeScript orchestrates the user experience.
