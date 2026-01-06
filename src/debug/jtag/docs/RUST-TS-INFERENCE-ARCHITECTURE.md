# Rust ↔ TypeScript Inference Architecture

## Problem Statement

Current implementation is broken:
- `socket.write()` with no acknowledgment - fire and forget
- Promises hang forever or timeout arbitrarily
- One stuck request blocks all future requests (mutex held forever)
- No cancellation - TypeScript times out but Rust keeps working
- No backpressure - can flood Rust with requests it can't handle
- Response matching breaks under concurrent load

**Result**: Helper AI has been stuck at "PHASE 3.3" for 2+ days.

---

## Requirements

### Functional
1. **Text generation** - Send prompt, receive generated text
2. **Model management** - Load, unload, list models
3. **LoRA adapters** - Load, apply, compose multiple adapters
4. **Concurrent requests** - Multiple personas generating simultaneously
5. **Video/audio inference** - Future: heavy compute workloads

### Non-Functional
1. **Reliability** - One failed request cannot poison the system
2. **Observability** - Know what's happening at all times
3. **Cancellation** - TypeScript can cancel, Rust stops working
4. **Timeouts** - Bounded wait times with proper cleanup
5. **Backpressure** - Rust can signal "I'm overloaded"
6. **Recovery** - Automatic reconnection after failures

---

## Architecture Options

### Option A: tonic (gRPC)

**Pros:**
- Industry standard, battle-tested
- Built-in streaming, cancellation, deadlines
- Strong typing via protobuf
- HTTP/2 multiplexing (multiple requests on one connection)
- Automatic reconnection
- Load balancing support

**Cons:**
- Requires protobuf schema management
- HTTP/2 overhead (minimal)
- Slightly more complex setup

**TypeScript client**: `@grpc/grpc-js` or `nice-grpc`

### Option B: tarpc (Rust-native RPC)

**Pros:**
- Pure Rust, no protobuf
- Async-native
- Simpler than gRPC

**Cons:**
- No official TypeScript client (would need custom)
- Less ecosystem support
- Less battle-tested

### Option C: Cap'n Proto

**Pros:**
- Zero-copy serialization (fast)
- RPC built-in

**Cons:**
- Complex schema language
- Smaller ecosystem
- TypeScript support is weak

### Option D: ZeroMQ + MessagePack

**Pros:**
- Very fast
- Flexible patterns (req/rep, pub/sub, etc.)
- Good TypeScript support

**Cons:**
- Lower-level, need to build RPC semantics ourselves
- No built-in cancellation/deadlines

### Option E: Fix Current Unix Socket

**Pros:**
- No new dependencies
- Already partially working

**Cons:**
- We've been failing at this for days
- Would need to reinvent RPC semantics
- No ecosystem support for the hard problems

---

## Recommendation: tonic (gRPC)

**Rationale:**
1. Cancellation is BUILT-IN - when TypeScript drops the call, Rust knows immediately
2. Deadlines propagate - timeout set in TS automatically enforced in Rust
3. Streaming - can send progress updates during long generation
4. Battle-tested - millions of production deployments
5. TypeScript ecosystem is mature (`nice-grpc` is excellent)
6. Future-proof - handles video/audio streaming when we need it

---

## Proposed Design

### Protocol Definition (protobuf)

```protobuf
syntax = "proto3";
package inference;

service InferenceService {
  // Unary RPCs
  rpc Ping(PingRequest) returns (PingResponse);
  rpc LoadModel(LoadModelRequest) returns (LoadModelResponse);
  rpc UnloadModel(UnloadModelRequest) returns (UnloadModelResponse);
  rpc ListModels(ListModelsRequest) returns (ListModelsResponse);

  // Server streaming - generation with progress
  rpc Generate(GenerateRequest) returns (stream GenerateResponse);

  // LoRA management
  rpc LoadAdapter(LoadAdapterRequest) returns (LoadAdapterResponse);
  rpc UnloadAdapter(UnloadAdapterRequest) returns (UnloadAdapterResponse);
}

message GenerateRequest {
  string model_id = 1;
  string prompt = 2;
  int32 max_tokens = 3;
  float temperature = 4;
  repeated string adapter_names = 5;  // LoRA adapters to apply
  int32 deadline_ms = 6;              // Max time for this request
}

message GenerateResponse {
  oneof response {
    GenerateProgress progress = 1;    // Streaming progress
    GenerateComplete complete = 2;    // Final result
    GenerateError error = 3;          // Error
  }
}

message GenerateProgress {
  int32 tokens_generated = 1;
  int32 tokens_total = 2;
  string partial_text = 3;            // Text so far (optional)
}

message GenerateComplete {
  string text = 1;
  int32 prompt_tokens = 2;
  int32 generated_tokens = 3;
  int32 duration_ms = 4;
}

message GenerateError {
  string code = 1;
  string message = 2;
  bool retryable = 3;
}
```

### Rust Server (tonic)

```rust
// Simplified - actual implementation in workers/inference/src/grpc.rs

#[tonic::async_trait]
impl InferenceService for InferenceServer {
    type GenerateStream = ReceiverStream<Result<GenerateResponse, Status>>;

    async fn generate(
        &self,
        request: Request<GenerateRequest>,
    ) -> Result<Response<Self::GenerateStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(32);

        // Spawn generation task
        let state = self.state.clone();
        tokio::spawn(async move {
            // Check cancellation via tx.is_closed()
            // This is THE KEY - when TypeScript cancels, tx closes automatically

            let result = generate_with_progress(
                &state,
                &req,
                |progress| {
                    if tx.is_closed() {
                        return Err("Cancelled");  // STOP WORKING
                    }
                    tx.send(Ok(progress)).await
                }
            ).await;

            match result {
                Ok(complete) => tx.send(Ok(complete)).await,
                Err(e) => tx.send(Ok(error(e))).await,
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
```

### TypeScript Client

```typescript
// Simplified - actual implementation in system/core/services/InferenceClient.ts

import { createChannel, createClient, Metadata } from 'nice-grpc';
import { InferenceServiceClient } from './generated/inference';

class InferenceClient {
  private client: InferenceServiceClient;

  constructor(address: string) {
    const channel = createChannel(address);
    this.client = createClient(InferenceServiceClient, channel);
  }

  async generate(
    request: GenerateRequest,
    options?: { signal?: AbortSignal; timeoutMs?: number }
  ): Promise<GenerateComplete> {
    const deadline = new Date(Date.now() + (options?.timeoutMs ?? 60000));

    // Cancellation via AbortSignal - native browser/Node pattern
    const abortController = new AbortController();
    if (options?.signal) {
      options.signal.addEventListener('abort', () => abortController.abort());
    }

    // Stream responses with automatic cancellation
    for await (const response of this.client.generate(request, {
      deadline,
      signal: abortController.signal,
    })) {
      if (response.progress) {
        // Optional: emit progress events
        this.emit('progress', response.progress);
      } else if (response.complete) {
        return response.complete;
      } else if (response.error) {
        throw new InferenceError(response.error);
      }
    }

    throw new Error('Stream ended without completion');
  }
}
```

### Key Behaviors

#### 1. Cancellation Flow
```
TypeScript                          Rust
    |                                 |
    |-- Generate(prompt) ------------>|
    |                                 |-- Start generating
    |                                 |
    |<--- Progress(10/100) -----------|
    |                                 |
    | [User navigates away]           |
    | [AbortController.abort()]       |
    |                                 |
    |-- [HTTP/2 RST_STREAM] --------->|
    |                                 |-- tx.is_closed() == true
    |                                 |-- STOP generating immediately
    |                                 |-- Release model mutex
    X                                 X
```

#### 2. Timeout Flow
```
TypeScript                          Rust
    |                                 |
    |-- Generate(deadline=30s) ------>|
    |                                 |-- Start generating
    |                                 |
    |<--- Progress(10/100) -----------|
    |                                 |
    | [30 seconds pass]               |
    | [Deadline exceeded]             |
    |                                 |
    |-- [gRPC DEADLINE_EXCEEDED] ---->|
    |                                 |-- Context cancelled
    |                                 |-- STOP generating
    X                                 X
```

#### 3. Concurrent Requests
```
TypeScript                          Rust
    |                                 |
    |-- Generate(model=A) ----------->|-- Spawns task 1
    |-- Generate(model=B) ----------->|-- Spawns task 2 (different model, no blocking)
    |-- Generate(model=A) ----------->|-- Spawns task 3 (queued behind task 1)
    |                                 |
    |<--- Complete(task 2) -----------|-- B finishes first (smaller model)
    |<--- Complete(task 1) -----------|-- A finishes
    |<--- Complete(task 3) -----------|-- Second A request finishes
```

### Concurrency Model

**Per-model queuing:**
- Each model has its own work queue
- Requests for different models run in parallel
- Requests for same model are serialized (GPU can only run one at a time)
- Queue depth limit (e.g., 3) with backpressure response

**Backpressure:**
```protobuf
message GenerateResponse {
  oneof response {
    // ...existing...
    BackpressureSignal backpressure = 4;  // "I'm overloaded"
  }
}

message BackpressureSignal {
  int32 queue_depth = 1;
  int32 estimated_wait_ms = 2;
  bool should_retry_later = 3;
}
```

TypeScript can then:
- Route to a different provider (cloud API)
- Wait and retry
- Fail fast with user-friendly message

---

## Migration Path

### Phase 1: Parallel Implementation
- Add tonic gRPC server alongside existing Unix socket
- Both run simultaneously
- New code uses gRPC, old code still works

### Phase 2: TypeScript Client
- Create new `InferenceClient` using `nice-grpc`
- Update `CandleAdapter` to use new client
- Test with Helper AI

### Phase 3: Validation
- Stress test: 10+ concurrent requests
- Cancellation test: abort mid-generation
- Timeout test: slow generation with deadline
- Chaos test: kill Rust mid-request, verify TS recovers

### Phase 4: Cleanup
- Remove Unix socket code
- Remove old `InferenceWorkerClient`
- Update documentation

---

## File Structure

```
workers/inference/
├── Cargo.toml              # Add tonic, prost dependencies
├── build.rs                # Protobuf compilation
├── proto/
│   └── inference.proto     # Service definition
└── src/
    ├── main.rs             # Entry point
    ├── grpc.rs             # tonic service implementation
    ├── models.rs           # Model loading/management
    └── generation.rs       # Text generation with progress

system/core/services/
├── InferenceClient.ts      # NEW: gRPC client
└── InferenceWorkerClient.ts # DEPRECATED: Unix socket client

daemons/ai-provider-daemon/adapters/candle/
└── shared/CandleAdapter.ts # Updated to use InferenceClient
```

---

## Dependencies

### Rust (Cargo.toml)
```toml
[dependencies]
tonic = "0.11"
prost = "0.12"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"

[build-dependencies]
tonic-build = "0.11"
```

### TypeScript (package.json)
```json
{
  "dependencies": {
    "nice-grpc": "^2.1.0",
    "@grpc/grpc-js": "^1.9.0",
    "protobufjs": "^7.2.0"
  }
}
```

---

## Success Criteria

1. **Helper AI responds to chat messages** - The whole point
2. **No stuck requests** - Cancellation works, timeouts work
3. **Concurrent generation** - Multiple personas at once
4. **Observable** - Can see queue depth, in-progress requests
5. **Recoverable** - Rust crash doesn't require full system restart

---

## Open Questions

1. **Unix socket vs TCP for gRPC?**
   - tonic supports Unix sockets via `tonic::transport::server::UdsConnectInfo`
   - Keeps same deployment model (no port management)
   - Recommendation: Unix socket for local, TCP option for future distributed

2. **Streaming granularity?**
   - Token-by-token (high overhead, real-time feel)
   - Every N tokens (balanced)
   - Chunks (lower overhead, less responsive)
   - Recommendation: Every 10 tokens or 500ms, whichever first

3. **Proto file management?**
   - Checked into repo (simple)
   - Separate proto repo (if sharing with other services)
   - Recommendation: Checked in, we're not at multi-repo scale

---

## Timeline Estimate

Not providing time estimates per CLAUDE.md guidelines. Work breakdown:

1. Proto definition + Rust server skeleton
2. Rust generation with cancellation
3. TypeScript client
4. CandleAdapter integration
5. Testing + stress testing
6. Cleanup old code
