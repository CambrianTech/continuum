# Unified Modular Runtime — Migration & Performance Architecture

## Vision: CBAR-Style Low-Friction, High-Performance Modules

Consolidate 11 separate Rust worker processes into a single `continuum-core` process. Adding new functionality = implement ONE trait (`ServiceModule`) + one line (`runtime.register()`). Zero wiring. Like CBAR's `appendAnalyzer()`.

**Result: 20-line modules with automatic logging, metrics, priority scheduling, and zero IPC overhead.**

---

## Why This Makes Everything Fast

### Current Architecture (Slow)

```
┌─────────────┐     IPC      ┌─────────────┐     IPC      ┌─────────────┐
│  TypeScript │ ──────────►  │ continuum-  │ ──────────►  │  embedding  │
│   Server    │   ~50-400ms  │    core     │   ~5-50ms    │   worker    │
└─────────────┘              └─────────────┘              └─────────────┘
                                   │
                                   │ IPC ~50-400ms
                                   ▼
                             ┌─────────────┐
                             │   search    │
                             │   worker    │
                             └─────────────┘
```

**Problems:**
- 10 separate processes = 10 event loops polling
- IPC latency: 50-400ms per cross-process call (we measured this!)
- Memory duplication: each process loads own copies of models, runtimes
- Queueing contention: requests pile up at IPC boundaries

### Target Architecture (Fast)

```
┌─────────────┐     IPC      ┌──────────────────────────────────────────┐
│  TypeScript │ ──────────►  │              continuum-core              │
│   Server    │   ONE hop    │  ┌────────┐ ┌────────┐ ┌──────────────┐  │
└─────────────┘              │  │ Voice  │ │ Data   │ │  Embedding   │  │
                             │  │ Module │ │ Module │ │    Module    │  │
                             │  └────────┘ └────────┘ └──────────────┘  │
                             │  ┌────────┐ ┌────────┐ ┌──────────────┐  │
                             │  │ Search │ │Inference│ │   Logger    │  │
                             │  │ Module │ │ Module │ │    Module    │  │
                             │  └────────┘ └────────┘ └──────────────┘  │
                             │         SHARED MEMORY / ZERO-COPY        │
                             └──────────────────────────────────────────┘
```

**Benefits:**
- ONE process = ONE event loop
- Inter-module calls = function calls (~0.001ms vs ~50-400ms)
- Shared memory: embedding model loaded ONCE, used by all
- No queueing contention: work-stealing thread pool
- Unified metrics: see all module performance in one place

---

## Performance Gains (Measured & Expected)

| Metric | Before (10 processes) | After (1 process) | Improvement |
|--------|----------------------|-------------------|-------------|
| IPC latency | 50-400ms | ~0.001ms | **50,000-400,000x** |
| Memory usage | ~800MB (duplicates) | ~300MB (shared) | **2.5x less** |
| CPU idle | 10 event loops | 1 event loop | **10x less** |
| fastembed instances | 2 (memory + embedding) | 1 (shared) | **2x less** |
| Tokio runtimes | 10 | 1 | **10x less** |
| Context switches | High (IPC) | Low (threads) | **~10x less** |

### Real Measurements (IPC Bottleneck)

From our timing instrumentation:
```
[ORMRustClient] SLOW IPC: data/query total=426ms (stringify=0ms write=0ms network+rust=426ms parse=0ms)
[ORMRustClient] SLOW IPC: data/query total=92ms (stringify=0ms write=0ms network+rust=92ms parse=0ms)
```

**The 426ms is QUEUEING, not actual work.** Rust-side timing shows queries complete in <50ms. The rest is waiting in IPC queues.

---

## Current State

### Already ServiceModules (9 modules in continuum-core)

| Module | Commands | Priority | State Pattern |
|--------|----------|----------|---------------|
| **health** | `health-check`, `get-stats` | Normal | Stateless |
| **cognition** | `cognition/*`, `inbox/create` | High | Per-persona DashMap |
| **channel** | `channel/*` | High | Per-persona DashMap |
| **voice** | `voice/*` | Realtime | Shared services |
| **code** | `code/*` | Normal | Per-workspace DashMap |
| **memory** | `memory/*` | Normal | Per-persona manager |
| **rag** | `rag/compose` | Normal | Shared engine |
| **data** | `data/*`, `adapter/*` | Normal | Lazy adapter cache |
| **models** | `models/discover` | Background | Stateless |

### Legacy Workers (8 to migrate)

| Worker | Lines | Commands | Migration Complexity |
|--------|-------|----------|---------------------|
| **logger** | ~220 | `log/*` | Trivial |
| **search** | ~260 | `search`, `vector-search` | Trivial |
| **training** | ~125 | `training/*` | Trivial |
| **archive** | ~300 | `archive/*` | Easy |
| **chat-drain** | ~150 | `chat-drain/*` | Easy |
| **embedding** | ~550 | `embedding/*` | Medium |
| **data-daemon** | ~400 | WAL cleanup | Medium (may be redundant) |
| **inference-grpc** | ~2600 | `model/*`, `generate` | Complex |

---

## Migration Plan

### Phase 1: Trivial Migrations (Week 1)

**LoggerModule** — 220 lines, fire-and-forget logging
```rust
impl ServiceModule for LoggerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "logger",
            priority: ModulePriority::Background,
            command_prefixes: &["log/"],
            needs_dedicated_thread: false,
            ..
        }
    }
}
```

**SearchModule** — 260 lines, BoW/BM25 algorithms
```rust
impl ServiceModule for SearchModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "search",
            priority: ModulePriority::Normal,
            command_prefixes: &["search", "list-algorithms", "vector-search"],
            ..
        }
    }
}
```

**TrainingModule** — 125 lines, training job management
```rust
impl ServiceModule for TrainingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "training",
            priority: ModulePriority::Background,
            command_prefixes: &["training/"],
            ..
        }
    }
}
```

### Phase 2: Easy Migrations (Week 2)

**ArchiveModule** — 300 lines, cold storage management
- Moves old data to archive databases
- Uses Commands.execute() for data operations
- Can share DataModule's adapter cache

**ChatDrainModule** — 150 lines, chat message processing
- Drains chat queues
- Simple state machine

### Phase 3: Medium Migrations (Week 3)

**EmbeddingModule** — 550 lines, fastembed integration
- Currently loads its own fastembed model
- After migration: shares model via SharedCompute
- **Key optimization**: Model loaded ONCE, all modules use it

```rust
// Before: Each worker loads its own model
let model = TextEmbedding::try_new(InitOptions { .. })?;

// After: Shared via SharedCompute (lazy, load once)
let model = ctx.compute.get_or_compute("embedding_model", async {
    TextEmbedding::try_new(InitOptions { .. })
}).await;
```

**DataDaemonModule** — 400 lines, WAL cleanup
- May be redundant with existing DataModule
- Audit: what does it do that DataModule doesn't?
- If redundant: delete, don't migrate

### Phase 4: Complex Migration (Week 4)

**InferenceModule** — 2600 lines, Candle LLM inference
- Currently gRPC server (port 50051)
- Most complex: GPU memory management, model loading
- Key: LoRA adapter paging via SharedCompute

```rust
impl ServiceModule for InferenceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "inference",
            priority: ModulePriority::Background, // Long-running
            command_prefixes: &["model/", "generate", "gpu/"],
            needs_dedicated_thread: true, // GPU ops
            max_concurrency: 1, // One inference at a time
        }
    }
}
```

---

## Per-Module Automatic Features

When you implement ServiceModule and call `runtime.register()`, you automatically get:

### 1. Segregated Logging
```
.continuum/jtag/logs/system/modules/
├── voice.log      # VoiceModule logs only
├── data.log       # DataModule logs only
├── embedding.log  # EmbeddingModule logs only
└── ...
```

### 2. IPC Metrics (P50/P95/P99)
```bash
./jtag runtime/metrics/module --module=data
# → { avgTimeMs: 12, p50Ms: 8, p95Ms: 45, p99Ms: 120, slowCommandCount: 3 }

./jtag runtime/metrics/all
# → All modules with their stats
```

### 3. Priority Scheduling
```rust
ModulePriority::Realtime   // Voice, audio — <10ms budget
ModulePriority::High       // Cognition — <50ms target
ModulePriority::Normal     // Data, code — 10-100ms OK
ModulePriority::Background // Training, logging — seconds OK
```

### 4. Runtime Priority Control (for Ares RTOS controller)
```bash
./jtag runtime/control/priority/set --module=embedding --priority=realtime
./jtag runtime/control/list  # All modules with priorities
```

### 5. TypeScript Types (via ts-rs)
```typescript
import { ModulePriority, ModuleStats, ModuleInfo } from '@shared/generated/runtime';

// Ares can query and control the runtime
const modules = await Commands.execute('runtime/control/list');
```

---

## SharedCompute Pattern (Zero-Copy Sharing)

Like CBAR's `CBAR_VideoFrame::getRGBImage()` — compute once, share via Arc:

```rust
// First caller computes, all subsequent callers get cached Arc<T>
let embedding_model = ctx.compute.get_or_compute(
    "global", "embedding_model",
    async { TextEmbedding::try_new(opts).await }
).await;

// Zero-copy: all modules share the same Arc<TextEmbedding>
let embeddings = embedding_model.embed(texts, None)?;
```

**Use cases:**
- Embedding model (loaded once, used by memory, search, RAG)
- LLM model (loaded once, used by inference, cognition)
- Tokenizer (loaded once, used everywhere)

---

## Migration Checklist Per Worker

For each legacy worker → ServiceModule:

1. [ ] Create `modules/{name}.rs`
2. [ ] Implement `ServiceModule` trait
3. [ ] Move logic from `main.rs` → `handle_command()`
4. [ ] Convert state to appropriate pattern:
   - Stateless → just implement
   - Shared service → `Arc<Service>` field
   - Per-key state → `DashMap<Key, State>`
5. [ ] Add to `modules/mod.rs`
6. [ ] Register in `main.rs`: `runtime.register(Arc::new(Module::new()))`
7. [ ] Update TypeScript client socket path (if any)
8. [ ] Disable in `workers-config.json`
9. [ ] Verify: `./jtag {command}` still works
10. [ ] Delete old worker directory

---

## Final State

After all migrations:

```rust
// main.rs — The entire worker startup
#[tokio::main]
async fn main() -> Result<()> {
    let runtime = Runtime::new();

    // Internal modules
    runtime.register(Arc::new(HealthModule::new()));
    runtime.register(Arc::new(VoiceModule::new()));
    runtime.register(Arc::new(CognitionModule::new()));
    runtime.register(Arc::new(ChannelModule::new()));
    runtime.register(Arc::new(MemoryModule::new()));
    runtime.register(Arc::new(CodeModule::new()));
    runtime.register(Arc::new(RagModule::new()));
    runtime.register(Arc::new(DataModule::new()));
    runtime.register(Arc::new(ModelsModule::new()));

    // Absorbed from separate workers
    runtime.register(Arc::new(LoggerModule::new()));
    runtime.register(Arc::new(SearchModule::new()));
    runtime.register(Arc::new(TrainingModule::new()));
    runtime.register(Arc::new(ArchiveModule::new()));
    runtime.register(Arc::new(EmbeddingModule::new()));
    runtime.register(Arc::new(InferenceModule::new()));

    runtime.serve("/tmp/continuum-core.sock").await
}
```

**workers-config.json** becomes trivial:
```json
{
  "workers": [
    {
      "name": "continuum-core",
      "binary": "workers/target/release/continuum-core-server",
      "socket": "/tmp/continuum-core.sock"
    }
  ]
}
```

Or eliminated entirely — just start continuum-core directly.

---

## Adding New Functionality (Post-Migration)

```rust
// 1. Create modules/video.rs (~20 lines of actual logic)
pub struct VideoModule { /* state */ }

impl ServiceModule for VideoModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "video",
            priority: ModulePriority::Realtime,
            command_prefixes: &["video/"],
            needs_dedicated_thread: true,
            ..
        }
    }

    async fn handle_command(&self, cmd: &str, params: Value) -> Result<CommandResult, String> {
        // Your 20 lines of algorithm here
    }
}

// 2. Register (ONE line in main.rs)
runtime.register(Arc::new(VideoModule::new()));

// 3. Done. Automatic:
//    ✅ Logging to .continuum/jtag/logs/system/modules/video.log
//    ✅ Metrics with P50/P95/P99
//    ✅ Priority scheduling
//    ✅ Command routing for video/*
//    ✅ TypeScript types via ts-rs
```

**This is how CBAR worked. This is how we work now.**
