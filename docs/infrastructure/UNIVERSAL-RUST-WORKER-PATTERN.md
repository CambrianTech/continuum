# Universal Rust Worker Pattern - First-Class Daemon Citizens

**Vision**: Rust workers that can call `Commands.execute()` become **first-class citizens** in the JTAG system, able to access all daemons and services just like TypeScript code.

## The Breakthrough

**Current State** (LoggerDaemon):
- Rust does specialized task (file I/O)
- TypeScript sends commands → Rust executes → responds
- **One-way communication**
- Rust isolated from rest of system

**Future State** (ArchiveWorker proves this):
- Rust can call `Commands.execute()` back to TypeScript
- Rust accesses DataDaemon, SessionDaemon, AIProviderDaemon, etc.
- **Bidirectional communication**
- Rust integrated with entire command system

## Universal Architecture Pattern

```
┌─────────────────────────────────────────────────────────────┐
│ TypeScript Daemon (Orchestration)                           │
│ - Discovery (entities, models, tasks)                       │
│ - Monitoring (triggers, health checks)                      │
│ - Queue management (what to process)                        │
└────────────────┬────────────────────────────────────────────┘
                 │ Unix Socket
                 │ Task: { type, params }
                 ↓
┌─────────────────────────────────────────────────────────────┐
│ Rust Worker (Heavy Lifting)                                 │
│ - FIFO queue                                                │
│ - Thread pool (concurrent processing)                       │
│ - IPC Client: Commands.execute() ←─ BREAKTHROUGH            │
└────────────────┬────────────────────────────────────────────┘
                 │ IPC via Commands.execute()
                 │ (Rust acts like TypeScript client)
                 ↓
┌─────────────────────────────────────────────────────────────┐
│ DataDaemon / AIProviderDaemon / SessionDaemon / etc.        │
│ - Entity operations                                         │
│ - AI inference                                              │
│ - Session state                                             │
│ - NO KNOWLEDGE of Rust workers                              │
└─────────────────────────────────────────────────────────────┘
```

**Key Insight**: Once Rust can call `Commands.execute()`, it can access THE ENTIRE SYSTEM.

## Daemons That Would Benefit

### 1. ArchiveDaemon (Proof-of-Concept)
**Problem**: 700K rows to archive, synchronous TypeScript blocking system

**Rust Solution**:
```rust
async fn archive_task(task: ArchiveTask) {
    // Call DataDaemon for CRUD operations
    let rows = Commands::execute("data/list", params).await?;

    for row in rows {
        Commands::execute("data/create", copy_params).await?;
    }

    Commands::execute("data/delete", delete_params).await?;
}
```

**Benefits**:
- Concurrent archiving (3-5 collections at once)
- Non-blocking (main thread free)
- Queue-based (FIFO)

### 2. TrainingDaemon
**Problem**: Fine-tuning blocks for hours, needs GPU management

**Rust Solution**:
```rust
async fn train_adapter(task: TrainingTask) {
    // Get training examples from DataDaemon
    let examples = Commands::execute("data/list", json!({
        "collection": "training_examples",
        "filter": { "domain": task.domain }
    })).await?;

    // Train model (GPU-intensive)
    let adapter = train_lora_adapter(&examples, &task.config)?;

    // Save adapter via DataDaemon
    Commands::execute("data/create", json!({
        "collection": "lora_adapters",
        "data": adapter
    })).await?;

    // Emit completion event
    Commands::execute("events/emit", json!({
        "event": "training:complete",
        "data": { "adapterId": adapter.id }
    })).await?;
}
```

**Benefits**:
- Train multiple adapters concurrently
- GPU pool management
- Non-blocking TypeScript
- Access to training data via DataDaemon

### 3. InferenceDaemon
**Problem**: Ollama calls block for 2-10 seconds, TypeScript single-threaded

**Rust Solution**:
```rust
async fn inference_task(task: InferenceTask) {
    // Get conversation context from DataDaemon
    let messages = Commands::execute("data/list", json!({
        "collection": "chat_messages",
        "filter": { "roomId": task.room_id },
        "limit": 50
    })).await?;

    // Run inference (blocking Ollama HTTP call)
    let response = ollama_generate(&task.model, &messages).await?;

    // Save AI generation for cost tracking
    Commands::execute("data/create", json!({
        "collection": "ai_generations",
        "data": {
            "model": task.model,
            "inputTokens": response.tokens_input,
            "outputTokens": response.tokens_output
        }
    })).await?;

    return response.text;
}
```

**Benefits**:
- 10+ concurrent inference requests
- No event loop blocking
- Cost tracking via DataDaemon

### 4. EmbeddingDaemon
**Problem**: Vector generation CPU-intensive, batch processing needed

**Rust Solution**:
```rust
async fn embed_documents(task: EmbeddingTask) {
    // Get documents from DataDaemon
    let docs = Commands::execute("data/list", json!({
        "collection": "documents",
        "filter": { "embedded": false }
    })).await?;

    // Generate embeddings (CPU-intensive, batch processing)
    let embeddings = batch_embed(&docs, task.batch_size)?;

    // Save vectors via DataDaemon
    for (doc_id, vector) in embeddings {
        Commands::execute("data/update", json!({
            "collection": "documents",
            "id": doc_id,
            "data": { "embedding": vector, "embedded": true }
        })).await?;
    }
}
```

**Benefits**:
- Batch processing (100 docs at once)
- SIMD optimizations
- Non-blocking TypeScript

### 5. ImageProcessingDaemon
**Problem**: Screenshot analysis, OCR, image generation slow

**Rust Solution**:
```rust
async fn process_screenshot(task: ImageTask) {
    // Get user context from SessionDaemon via Commands
    let session = Commands::execute("session/get", json!({
        "sessionId": task.session_id
    })).await?;

    // Process image (image decoding, OCR, analysis)
    let analysis = analyze_screenshot(&task.image_data)?;

    // Store results via DataDaemon
    Commands::execute("data/create", json!({
        "collection": "screenshot_analyses",
        "data": analysis
    })).await?;
}
```

### 6. AudioProcessingDaemon
**Problem**: Transcription, voice synthesis CPU-intensive

**Rust Solution**:
```rust
async fn transcribe_audio(task: AudioTask) {
    // Transcribe audio (Whisper model, CPU-intensive)
    let transcript = whisper_transcribe(&task.audio_data)?;

    // Save to DataDaemon
    Commands::execute("data/create", json!({
        "collection": "transcripts",
        "data": {
            "audioId": task.audio_id,
            "text": transcript.text,
            "confidence": transcript.confidence
        }
    })).await?;
}
```

## The Universal Pattern

**Every daemon follows same structure**:

### TypeScript Daemon (Coordinator)
```typescript
export class XyzDaemonServer extends XyzDaemon {
  private workerClient: XyzWorkerClient | null = null;

  protected override async onStart(): Promise<void> {
    // Connect to Rust worker
    this.workerClient = new XyzWorkerClient('/tmp/xyz-worker.sock');
    await this.workerClient.connect();
  }

  async queueTask(task: XyzTask): Promise<void> {
    // Queue task to Rust
    await this.workerClient.send({
      command: 'process',
      task
    });
  }
}
```

### Rust Worker (Executor)
```rust
// src/main.rs
fn main() {
    let listener = UnixListener::bind("/tmp/xyz-worker.sock")?;
    let (task_tx, task_rx) = mpsc::channel();

    // Spawn worker threads
    for _ in 0..num_cpus::get() {
        let rx = task_rx.clone();
        thread::spawn(move || {
            for task in rx.iter() {
                process_task(task).await;
            }
        });
    }

    // Accept connections
    for stream in listener.incoming() {
        handle_connection(stream, task_tx.clone());
    }
}

async fn process_task(task: Task) {
    // Call Commands.execute() to access system
    let data = commands::execute("data/list", params).await?;

    // Heavy computation here
    let result = heavy_computation(data)?;

    // Save results via Commands
    commands::execute("data/create", result).await?;
}
```

### Rust IPC Client (Universal Module)
```rust
// shared/ipc/rust-command-client/src/lib.rs
pub struct CommandClient {
    socket: UnixStream,
}

impl CommandClient {
    pub async fn execute(
        &mut self,
        command: &str,
        params: serde_json::Value
    ) -> Result<serde_json::Value> {
        // Send command execution request to TypeScript
        let request = json!({
            "command": command,
            "params": params
        });

        self.send_json(&request)?;
        let response = self.read_json()?;

        Ok(response)
    }
}
```

## Implementation Strategy

### Phase 1: Prove with ArchiveWorker
**Goal**: Demonstrate Rust can call Commands.execute() and access DataDaemon

**Tasks**:
1. Create ArchiveWorker Rust skeleton
2. Implement IPC client for Commands.execute()
3. Archive 10 rows end-to-end
4. Verify non-blocking, concurrent processing

**Success**: ArchiveWorker replaces synchronous ArchiveDaemon, system stable

### Phase 2: Extract Universal Pattern
**Goal**: Create reusable Rust worker template

**Tasks**:
1. Extract `RustCommandClient` as reusable crate
2. Create `rust-worker-template` with:
   - Unix socket server boilerplate
   - Thread pool boilerplate
   - Task queue boilerplate
   - Health check boilerplate
3. Document pattern in ARCHITECTURE-RULES.md

**Success**: New daemon can be created by copying template

### Phase 3: Migrate TrainingDaemon
**Goal**: Prove pattern works for different use case (training vs archiving)

**Tasks**:
1. Copy rust-worker-template → `workers/training/`
2. Implement training logic (LoRA fine-tuning)
3. Use RustCommandClient to access DataDaemon for training examples
4. Test concurrent training (2-3 adapters simultaneously)

**Success**: Multiple adapters training concurrently, non-blocking system

### Phase 4: Migrate InferenceDaemon
**Goal**: Prove pattern scales to high-throughput use case

**Tasks**:
1. Copy rust-worker-template → `workers/inference/`
2. Implement Ollama HTTP client in Rust
3. Use RustCommandClient for context retrieval and cost tracking
4. Test 10+ concurrent inference requests

**Success**: 10+ AIs responding simultaneously, no blocking

### Phase 5: Create New Daemons
**Goal**: Demonstrate pattern enables new capabilities

**Candidates**:
- EmbeddingDaemon (vector generation)
- ImageProcessingDaemon (OCR, analysis)
- AudioProcessingDaemon (transcription, synthesis)
- VideoProcessingDaemon (frame extraction, encoding)

## Benefits of Universal Pattern

### 1. Performance
- True parallelism (not event loop concurrency)
- CPU-intensive tasks don't block main thread
- GPU utilization (training, inference)

### 2. Simplicity
- TypeScript: Coordination and discovery (what it's good at)
- Rust: Heavy computation (what it's good at)
- Clean separation of concerns

### 3. Incremental Adoption
- Start with TypeScript daemon (works immediately)
- Add Rust worker when performance needed
- Graceful fallback if Rust unavailable

### 4. Reusability
- `RustCommandClient` used by all workers
- `rust-worker-template` scaffolds new daemons
- Shared health check, queue, thread pool code

### 5. First-Class Integration
- Rust calls Commands.execute() like TypeScript
- Access to all daemons (Data, Session, AI, etc.)
- Participate in event system
- No special treatment needed

## Architectural Impact

**Before**:
```
TypeScript Daemon (does everything)
  ↓ slow, blocks, single-threaded
```

**After**:
```
TypeScript Daemon (orchestrates)
  ↓ fast, non-blocking
Rust Worker (executes)
  ↓ parallel, concurrent
Commands System (unified interface)
  ↓ all daemons accessible
```

## Why ArchiveWorker is Critical

**ArchiveWorker is not just about archiving** - it's the **proof-of-concept** that:
1. Rust can call Commands.execute() bidirectionally
2. Rust can access DataDaemon (most critical daemon)
3. Pattern works for queue-based concurrent processing
4. Graceful fallback maintains system stability

**Once ArchiveWorker works**, the pattern is proven and can be applied to:
- Training (GPU-intensive)
- Inference (I/O-intensive, high-throughput)
- Embeddings (CPU-intensive, batch processing)
- Image/Audio/Video (specialized libraries)

## Success Criteria

**ArchiveWorker skeleton proves pattern when**:
1. ✅ Rust receives archive task from TypeScript
2. ✅ Rust calls Commands.execute('data/list') successfully
3. ✅ Rust receives entity data from DataDaemon
4. ✅ Rust archives 10 rows (copy-verify-delete)
5. ✅ TypeScript remains non-blocking during archive
6. ✅ Multiple collections can archive concurrently

**Then**: Pattern is universal, template can be extracted, other daemons can migrate.

## Vision

**Every performance-critical daemon** becomes a hybrid:
- TypeScript for orchestration (discovery, monitoring, health)
- Rust for execution (computation, I/O, concurrency)
- Commands.execute() as the bridge (bidirectional communication)

**Result**: System that's both elegant (TypeScript coordination) and blazing fast (Rust execution).

---

**Next Step**: Implement ArchiveWorker skeleton to prove this works.
