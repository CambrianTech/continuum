# AI Daemon Genomic Architecture
## Auto Model Management + Process Isolation + LoRA Hot-Swapping + Checkpointing

**Status**: 📋 Design Phase
**Priority**: 🔥 Critical (enables performant multi-persona system)
**Dependencies**: AIProviderDaemon, PersonaUser, Academy

---

## 🎯 Vision

The AI Daemon is a **genomic operating system** for AI models - it manages model lifecycle, process isolation, resource scheduling, LoRA layer hot-swapping, and state checkpointing. Think of it as Docker + Kubernetes for AI inference, but optimized for genomic personas.

```
┌─────────────────────────────────────────────────────────────┐
│         AI Daemon (Genomic OS for AI Models)                │
│                                                              │
│  ✅ Auto model installation (download on first use)         │
│  ✅ Process isolation (crash-proof sandboxing)              │
│  ✅ GPU/CPU scheduling (intelligent paging)                 │
│  ✅ LoRA hot-swapping (dynamic genome assembly)             │
│  ✅ Checkpointing (freeze/resume state)                     │
│  ✅ Queue management (priority scheduling)                  │
└──────────────────────────────────────────────────────────────┘
```

---

## 📊 Phased Implementation

### **Phase 1: Auto Model Management** (Week 1) 🔴 CRITICAL
**Goal**: Models auto-install on first use, no manual `ollama pull` required

**Tasks**:
- [ ] Add `checkModelAvailability()` to AIProviderAdapter interface
- [ ] Add `installModel()` with progress callback
- [ ] Add `getRecommendedModels()` for discovery
- [ ] Implement in OllamaAdapter
- [ ] Test auto-installation flow

**Success Criteria**:
- ✅ PersonaUser requests llama3.2:3b → auto-downloads if missing
- ✅ Progress logs show download percentage
- ✅ Persona responds after installation completes
- ✅ Second request uses cached model (no reinstall)

**Example**:
```typescript
// PersonaUser.ts
async respondToMessage(message: ChatMessageEntity): Promise<void> {
  // 1. Check if model is available
  const adapter = await AIProviderDaemon.getAdapter('ollama');
  const missing = await adapter.checkModelAvailability(['llama3.2:3b']);

  // 2. Auto-install if missing
  if (missing.length > 0) {
    console.log(`📦 Installing ${missing[0]}...`);
    await adapter.installModel(missing[0], (progress) => {
      console.log(`   ${progress.percentComplete}% (${progress.bytesDownloaded}/${progress.bytesTotal} bytes)`);
    });
    console.log(`✅ ${missing[0]} installed`);
  }

  // 3. Generate response
  const response = await AIProviderDaemon.generateText({ model: 'llama3.2:3b', ... });
}
```

---

### **Phase 2: Process Isolation** (Week 2) 🟠 HIGH PRIORITY
**Goal**: Each model runs in isolated worker process, system never blocks

**Tasks**:
- [ ] Design AIWorkerProcess architecture
- [ ] Implement worker spawn/shutdown lifecycle
- [ ] Add IPC message passing (request/response)
- [ ] Add crash recovery and restart logic
- [ ] Test concurrent generation (3+ personas simultaneously)

**Success Criteria**:
- ✅ 3 personas generate responses simultaneously (parallel, not sequential)
- ✅ Ollama crash doesn't kill entire system
- ✅ Main process responds to `./jtag ping` during 10s inference
- ✅ Worker restart after crash preserves model state

**Architecture**:
```
┌─────────────────────────────────────────────────────────────┐
│         AIProviderDaemon (Main Process)                     │
│                                                              │
│  - Worker pool management                                   │
│  - Request routing                                          │
│  - Health monitoring                                        │
└────────────┬────────────────────────────────────────────────┘
             │
     ┌───────┴───────┬──────────────┬─────────────┐
     │               │              │             │
┌────▼─────┐   ┌────▼──────┐ ┌─────▼─────┐ ┌────▼─────┐
│ Worker 1 │   │  Worker 2 │ │ Worker 3  │ │ Worker 4 │
│ (Fork)   │   │  (Fork)   │ │ (Fork)    │ │ (Fork)   │
│          │   │           │ │           │ │          │
│ Ollama   │   │ Ollama    │ │ OpenAI    │ │ Anthropic│
│ llama3.2 │   │ phi3:mini │ │ gpt-4o    │ │ claude-3 │
│ :3b      │   │           │ │           │ │          │
└──────────┘   └───────────┘ └───────────┘ └──────────┘
  PID 1234      PID 1235     PID 1236      PID 1237
  4GB RAM       2GB RAM      1GB RAM       1GB RAM
  Sandboxed     Sandboxed    Sandboxed     Sandboxed
```

**Implementation**:
```typescript
// system/ai-workers/AIWorkerProcess.ts
export class AIWorkerProcess {
  private process: ChildProcess;
  private modelId: string;
  private adapter: AIProviderAdapter;
  private pendingRequests: Map<UUID, RequestHandler> = new Map();

  constructor(modelId: string, adapter: AIProviderAdapter) {
    this.modelId = modelId;
    this.adapter = adapter;

    // Spawn isolated worker process
    this.process = fork('./ai-worker-process.js', [modelId], {
      execArgv: ['--max-old-space-size=4096'],  // 4GB RAM limit
      stdio: ['pipe', 'pipe', 'pipe', 'ipc']
    });

    // IPC message handling
    this.process.on('message', this.handleMessage.bind(this));
    this.process.on('exit', this.handleExit.bind(this));
    this.process.on('error', this.handleError.bind(this));
  }

  /**
   * Execute inference request (non-blocking)
   */
  async generate(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    return new Promise((resolve, reject) => {
      const requestId = crypto.randomUUID();

      this.pendingRequests.set(requestId, { resolve, reject, timeout: setTimeout(() => {
        reject(new Error(`Request ${requestId} timed out after 30s`));
        this.pendingRequests.delete(requestId);
      }, 30000) });

      // Send to worker via IPC
      this.process.send({
        type: 'generate',
        requestId,
        request
      });
    });
  }

  /**
   * Handle crash and restart
   */
  private async handleExit(code: number, signal: string): Promise<void> {
    console.error(`❌ Worker ${this.modelId} exited (code: ${code}, signal: ${signal})`);

    // Reject all pending requests
    for (const [requestId, handler] of this.pendingRequests) {
      handler.reject(new Error(`Worker crashed: ${signal}`));
      clearTimeout(handler.timeout);
    }
    this.pendingRequests.clear();

    // Auto-restart after 1 second
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log(`🔄 Restarting worker ${this.modelId}...`);
    // Re-spawn process (handled by AIResourceManager)
  }

  /**
   * Graceful shutdown
   */
  async shutdown(): Promise<void> {
    this.process.send({ type: 'shutdown' });
    await this.waitForExit(5000);  // 5 second grace period
    if (!this.process.killed) {
      this.process.kill('SIGKILL');  // Force kill
    }
  }
}
```

---

### **Phase 3: GPU/CPU Resource Scheduling** (Week 3) 🟡 MEDIUM PRIORITY
**Goal**: Intelligent paging of models in/out of VRAM, queue-based scheduling

**Tasks**:
- [ ] Design AIResourceManager with LRU cache
- [ ] Implement VRAM usage tracking
- [ ] Add model eviction (page out least-recently-used)
- [ ] Add request queue with priority scheduling
- [ ] Test with limited VRAM (8GB) and many models

**Success Criteria**:
- ✅ 8GB GPU runs 5 different 3B models by paging in/out
- ✅ High-priority requests jump queue
- ✅ LRU eviction keeps hot models in VRAM
- ✅ No OOM crashes, graceful degradation

**Architecture**:
```typescript
// system/ai-workers/AIResourceManager.ts
export class AIResourceManager {
  private vramLimit: number = 8000;  // 8GB in MB
  private vramUsed: number = 0;

  private modelCache: LRUCache<string, LoadedModel>;
  private loraCache: LRUCache<UUID, LoRAAdapter>;

  private requestQueue: PriorityQueue<AIRequest>;
  private workers: Map<string, AIWorkerProcess> = new Map();

  /**
   * Schedule request with intelligent resource management
   */
  async scheduleRequest(request: AIRequest): Promise<TextGenerationResponse> {
    // 1. Calculate required VRAM
    const modelSize = this.estimateModelSize(request.model);
    const loraSize = request.loraLayers.reduce((sum, l) => sum + l.size, 0);
    const required = modelSize + loraSize;

    console.log(`📊 Request needs ${required}MB VRAM (${this.vramUsed}/${this.vramLimit}MB used)`);

    // 2. If not enough VRAM, evict LRU models
    if (this.vramUsed + required > this.vramLimit) {
      await this.evictLRU(required);
    }

    // 3. Load model (or use cached)
    const model = await this.loadModel(request.model);
    this.vramUsed += modelSize;

    // 4. Get or create worker
    const worker = await this.getOrCreateWorker(request.model);

    // 5. Hot-swap LoRA layers if needed
    if (request.loraLayers.length > 0) {
      await worker.swapLoRALayers(request.loraLayers);
      this.vramUsed += loraSize;
    }

    // 6. Execute (non-blocking)
    const response = await worker.generate(request);

    // 7. Update LRU cache
    this.modelCache.touch(request.model);

    return response;
  }

  /**
   * Evict least-recently-used models to free VRAM
   */
  private async evictLRU(needed: number): Promise<void> {
    let freed = 0;

    console.log(`🗑️  Need to free ${needed}MB VRAM...`);

    // Evict models in LRU order
    for (const [modelId, model] of this.modelCache.oldestFirst()) {
      if (freed >= needed) break;

      console.log(`   📤 Paging out ${modelId} (${model.vramSize}MB)`);
      await this.unloadModel(modelId);
      freed += model.vramSize;
      this.vramUsed -= model.vramSize;
    }

    console.log(`✅ Freed ${freed}MB VRAM`);
  }

  /**
   * Priority queue for request scheduling
   */
  private prioritizeRequest(request: AIRequest): number {
    // Higher priority = process first
    if (request.userId === SYSTEM_USER) return 100;     // System requests (health checks)
    if (request.priority === 'realtime') return 90;     // User-facing chat
    if (request.priority === 'batch') return 50;        // Background training
    return 70;  // Default
  }
}
```

---

### **Phase 4: LoRA Hot-Swapping** (Week 4) 🟢 ENHANCEMENT
**Goal**: Dynamic genome assembly via vector similarity search

**Tasks**:
- [ ] Design LoRARegistry with embeddings
- [ ] Implement cosine similarity search
- [ ] Add hot-swap API to workers
- [ ] Integrate with PersonaUser task detection
- [ ] Test dynamic layer selection

**Success Criteria**:
- ✅ Persona detects "code review" task → loads [typescript-expert, code-review-protocol, strict-typing]
- ✅ Layer swap takes <500ms
- ✅ Next message uses new layers without restart
- ✅ Similarity search finds relevant layers (>0.7 cosine similarity)

**Architecture**:
```typescript
// system/genome/LoRARegistry.ts
export class LoRARegistry {
  private layers: Map<UUID, LoRALayerMetadata> = new Map();
  private embeddings: Map<UUID, Float32Array> = new Map();  // 512-dim vectors

  /**
   * Find best LoRA layers for current task using cosine similarity
   */
  async selectLayers(taskEmbedding: Float32Array, options: {
    maxLayers?: number;
    minSimilarity?: number;
    excludeLayers?: UUID[];
  } = {}): Promise<LoRALayer[]> {
    const maxLayers = options.maxLayers || 3;
    const minSimilarity = options.minSimilarity || 0.7;

    // 1. Calculate cosine similarity between task and all layers
    const similarities: Array<{ layerId: UUID; similarity: number }> = [];

    for (const [layerId, layerEmbed] of this.embeddings) {
      if (options.excludeLayers?.includes(layerId)) continue;

      const similarity = this.cosineSimilarity(taskEmbedding, layerEmbed);
      if (similarity >= minSimilarity) {
        similarities.push({ layerId, similarity });
      }
    }

    // 2. Sort by similarity and take top N
    const topLayers = similarities
      .sort((a, b) => b.similarity - a.similarity)
      .slice(0, maxLayers);

    console.log(`🧬 Selected ${topLayers.length} LoRA layers:`);
    for (const { layerId, similarity } of topLayers) {
      const layer = this.layers.get(layerId)!;
      console.log(`   - ${layer.name} (similarity: ${(similarity * 100).toFixed(1)}%)`);
    }

    // 3. Load LoRA adapters
    return await Promise.all(
      topLayers.map(({ layerId }) => this.loadLayer(layerId))
    );
  }

  /**
   * Cosine similarity between two vectors
   */
  private cosineSimilarity(a: Float32Array, b: Float32Array): number {
    let dotProduct = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }

    return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
  }
}

// PersonaUser.ts - Dynamic genome assembly
async prepareForTask(taskContext: RAGContext): Promise<void> {
  // 1. Generate embedding for current task
  const taskDescription = this.summarizeTask(taskContext);
  const taskEmbed = await this.embedText(taskDescription);

  // 2. Search LoRA registry for relevant layers
  const selectedLayers = await LoRARegistry.selectLayers(taskEmbed, {
    maxLayers: 3,
    minSimilarity: 0.7
  });

  // 3. Hot-swap layers in worker process
  await this.worker.swapLoRALayers(selectedLayers);

  console.log(`🧬 Genome assembled: ${selectedLayers.map(l => l.name).join(' + ')}`);
}
```

---

### **Phase 5: Checkpointing & State Management** (Week 5) 🔵 FUTURE
**Goal**: Freeze/resume persona state, save conversation context

**Tasks**:
- [ ] Design checkpoint format (model state + LoRA layers + conversation history)
- [ ] Implement freeze (serialize to disk)
- [ ] Implement resume (deserialize from disk)
- [ ] Add checkpoint triggers (manual, time-based, event-based)
- [ ] Test checkpoint/resume cycle

**Success Criteria**:
- ✅ Persona freezes mid-conversation → system restarts → resumes exactly where it left off
- ✅ Checkpoint includes: model state, active LoRA layers, conversation history, private memories
- ✅ Resume takes <5s
- ✅ No context loss after resume

**Architecture**:
```typescript
// system/checkpointing/PersonaCheckpoint.ts
export interface PersonaCheckpoint {
  // Identity
  personaId: UUID;
  checkpointId: UUID;
  timestamp: string;

  // Model state
  baseModel: string;
  modelState: CompressedModelState;  // Optional: full model weights

  // Genome state
  activeLoRALayers: UUID[];
  loraStates: Map<UUID, CompressedLoRAState>;

  // Conversation context
  conversationHistory: ChatMessageEntity[];
  ragContext: RAGContext;
  privateMemories: PersonaMemory[];

  // Metadata
  checkpoint Type: 'manual' | 'automatic' | 'crash-recovery';
  compressionRatio: number;
  sizeBytes: number;
}

export class PersonaCheckpointer {
  /**
   * Freeze persona state to disk
   */
  async freeze(persona: PersonaUser): Promise<PersonaCheckpoint> {
    console.log(`❄️  Freezing persona ${persona.displayName}...`);

    // 1. Collect current state
    const checkpoint: PersonaCheckpoint = {
      personaId: persona.id,
      checkpointId: crypto.randomUUID(),
      timestamp: new Date().toISOString(),

      baseModel: persona.baseModel,
      modelState: null,  // Skip full model (too large)

      activeLoRALayers: persona.activeLayers.map(l => l.id),
      loraStates: await this.serializeLoRALayers(persona.activeLayers),

      conversationHistory: await this.getRecentMessages(persona.id, 100),
      ragContext: persona.currentRAGContext,
      privateMemories: await this.getPrivateMemories(persona.id, 50),

      checkpointType: 'manual',
      compressionRatio: 0,
      sizeBytes: 0
    };

    // 2. Compress and write to disk
    const compressed = await this.compress(checkpoint);
    const path = `.continuum/checkpoints/${persona.id}/${checkpoint.checkpointId}.ckpt`;
    await fs.promises.writeFile(path, compressed);

    checkpoint.sizeBytes = compressed.length;
    checkpoint.compressionRatio = compressed.length / this.estimateUncompressedSize(checkpoint);

    console.log(`✅ Checkpoint saved: ${(checkpoint.sizeBytes / 1024 / 1024).toFixed(2)}MB`);

    return checkpoint;
  }

  /**
   * Resume persona from checkpoint
   */
  async resume(checkpointId: UUID): Promise<PersonaUser> {
    console.log(`🔄 Resuming from checkpoint ${checkpointId}...`);

    // 1. Load checkpoint from disk
    const checkpoint = await this.loadCheckpoint(checkpointId);

    // 2. Recreate persona
    const persona = await PersonaUser.create({
      id: checkpoint.personaId,
      baseModel: checkpoint.baseModel,
      // ... other config
    });

    // 3. Restore LoRA layers
    const layers = await Promise.all(
      checkpoint.activeLoRALayers.map(id => LoRARegistry.loadLayer(id))
    );
    await persona.worker.swapLoRALayers(layers);

    // 4. Restore conversation context
    persona.currentRAGContext = checkpoint.ragContext;

    // 5. Restore private memories
    await this.restoreMemories(persona.id, checkpoint.privateMemories);

    console.log(`✅ Persona ${persona.displayName} resumed`);

    return persona;
  }
}
```

---

## 🏗️ Complete System Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        AI Daemon (Main Process)                          │
│                                                                          │
│  ┌────────────────────┐  ┌─────────────────┐  ┌────────────────────┐   │
│  │ Auto Model Manager │  │ Resource Manager│  │ LoRA Registry      │   │
│  │                    │  │                 │  │                    │   │
│  │ - Install models   │  │ - VRAM tracking │  │ - Vector search    │   │
│  │ - Check available  │  │ - LRU eviction  │  │ - Layer metadata   │   │
│  │ - Progress logs    │  │ - Queue mgmt    │  │ - Embeddings (512d)│   │
│  └────────────────────┘  └─────────────────┘  └────────────────────┘   │
│                                                                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      Worker Pool Manager                           │ │
│  │  - Spawn/shutdown workers                                          │ │
│  │  - Health monitoring                                               │ │
│  │  - Crash recovery                                                  │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────┬───────────────────────────────────────────┘
                               │
       ┌───────────────────────┼───────────────────────┐
       │                       │                       │
   ┌───▼────┐            ┌────▼────┐           ┌─────▼─────┐
   │Worker 1│            │Worker 2 │           │ Worker 3  │
   │        │            │         │           │           │
   │ Ollama │            │ Ollama  │           │  OpenAI   │
   │ llama  │            │ phi3    │           │  gpt-4o   │
   │ 3.2:3b │            │ :mini   │           │           │
   │        │            │         │           │           │
   │ LoRAs: │            │ LoRAs:  │           │  LoRAs:   │
   │ [chat] │            │ [code]  │           │  [review] │
   │ [proto]│            │ [debug] │           │  [docs]   │
   └────────┘            └─────────┘           └───────────┘
   PID 1234              PID 1235              PID 1236
   4GB RAM               2GB RAM               1GB RAM
   2.5GB VRAM            2.0GB VRAM            0GB VRAM

   Checkpoint:           Checkpoint:           Checkpoint:
   ❄️  Saved 5m ago      ❄️  Saved 2h ago      ❄️  None
```

---

## 📝 Configuration

**User Settings (UserCapabilities):**
```typescript
interface AICapabilities {
  // Resource limits
  maxVRAM: number;              // 8000MB, 16000MB, 24000MB
  maxConcurrentWorkers: number; // 4, 8, 16
  maxRAMPerWorker: number;      // 4096MB

  // Model management
  autoInstallModels: boolean;   // Auto-download missing models?
  maxModelCacheSize: number;    // 50GB
  modelStoragePath: string;     // ~/.ollama/models

  // LoRA genome
  enableGenomicSwapping: boolean;   // Dynamic LoRA assembly?
  maxLoRALayersPerPersona: number;  // 3
  loraSelectionThreshold: number;   // 0.7 (cosine similarity)

  // Checkpointing
  autoCheckpoint: boolean;          // Auto-save persona state?
  checkpointInterval: number;       // 3600000ms (1 hour)
  maxCheckpointsPerPersona: number; // 10
  checkpointStoragePath: string;    // .continuum/checkpoints
}
```

---

## 🧪 Testing Strategy

**Phase 1 Tests (Auto Model Management):**
```bash
# 1. Auto-installation
rm -rf ~/.ollama/models/llama3.2*  # Remove model
./jtag exec --code="/* send message to persona using llama3.2:3b */"
# Expect: Logs show "📦 Installing llama3.2:3b... 45%... 100%"
# Expect: Persona responds after installation

# 2. Skip reinstall
./jtag exec --code="/* send another message */"
# Expect: No installation logs
# Expect: Immediate response
```

**Phase 2 Tests (Process Isolation):**
```bash
# 1. Concurrent generation
./jtag exec --code="/* send 3 messages to 3 personas simultaneously */"
# Expect: All 3 respond in parallel (~5-7s total, not 15-21s)

# 2. Crash recovery
killall -9 ollama  # Simulate crash
./jtag exec --code="/* send message */"
# Expect: Worker restarts
# Expect: Response after restart

# 3. Main process responsiveness
./jtag exec --code="/* trigger 10s inference */" &
./jtag ping
# Expect: ping responds immediately (main process not blocked)
```

**Phase 3 Tests (Resource Scheduling):**
```bash
# 1. VRAM eviction (8GB GPU, load 5x 3B models = 15GB)
# Load 5 different models sequentially
# Expect: LRU eviction frees space
# Expect: No OOM crashes

# 2. Priority queue
./jtag exec --code="/* low priority batch request */" &
./jtag exec --code="/* high priority user message */"
# Expect: User message processed first
```

**Phase 4 Tests (LoRA Hot-Swapping):**
```bash
# 1. Dynamic genome assembly
./jtag exec --code="
  input.value = 'Review this TypeScript code for type safety';
  chatWidget.sendMessage();
"
# Expect: Logs show "🧬 Genome assembled: typescript-expert + code-review-protocol + strict-typing"

# 2. Layer swap speed
# Measure time between genome log and first token
# Expect: <500ms
```

**Phase 5 Tests (Checkpointing):**
```bash
# 1. Freeze/resume cycle
./jtag persona/checkpoint --personaId=<TEACHER_AI>
npm restart  # Kill and restart system
./jtag persona/resume --checkpointId=<ID>
# Expect: Persona resumes with same conversation context
# Expect: No context loss
```

---

## 🔗 Related Documentation

- [AI Provider Daemon Architecture](./ARCHITECTURE.md) - Current daemon design
- [Genomic Data Architecture](/Volumes/FlashGordon/cambrian/continuum/middle-out/academy/genomic-data-architecture.md) - LoRA layer types
- [RAG Adapter Architecture](../../system/rag/RAG_ADAPTER_ARCHITECTURE.md) - Capability-aware context building
- [Process Isolation Architecture](/Volumes/FlashGordon/cambrian/continuum/middle-out/architecture/process-isolation-architecture.md) - OS-level sandboxing

---

## 📅 Timeline

**Week 1**: Auto model management (Phase 1)
**Week 2**: Process isolation (Phase 2)
**Week 3**: Resource scheduling (Phase 3)
**Week 4**: LoRA hot-swapping (Phase 4)
**Week 5**: Checkpointing (Phase 5)

**Total**: ~5 weeks to production-ready genomic AI daemon

---

## 🎯 Success Metrics

**Phase 1**: ✅ No manual `ollama pull` ever required
**Phase 2**: ✅ System survives model crashes, 3+ concurrent generations
**Phase 3**: ✅ 8GB GPU runs 5 models via intelligent paging
**Phase 4**: ✅ Personas dynamically assemble genome for each task
**Phase 5**: ✅ Personas resume from checkpoint with zero context loss

---

**Status**: Ready for Phase 1 implementation
**Next Step**: Implement auto model management in OllamaAdapter
