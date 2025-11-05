# IMPLEMENTATION ROADMAP: Genome + Academy + AI Daemon

**Date**: 2025-10-07
**Purpose**: Concrete implementation plan addressing real constraints (memory, GPU, async/sync bottlenecks)
**Status**: Planning - Ready to execute

---

## 🎯 The Real Constraints We're Solving

### Memory & GPU Constraints
- ❌ **Can't load all LoRA layers at once** - GPU memory limited (typically 8-24GB)
- ❌ **Can't run many models simultaneously** - Each model instance = 2-8GB VRAM
- ❌ **Synchronous operations block** - Model inference takes 100ms-2s per request
- ❌ **Context switching is expensive** - Loading/unloading LoRA layers = 50-200ms

### Our Solution Strategy
- ✅ **AI Daemon with async queue** - Non-blocking inference requests
- ✅ **LoRA layer LRU cache** - Keep hot layers in memory, evict cold ones
- ✅ **Process-per-model pools** - 2-4 model processes, route efficiently
- ✅ **Lazy genome loading** - Load layers on-demand, not upfront
- ✅ **Streaming responses** - Start returning tokens before full completion

---

## 📋 Implementation Phases

### Phase 0: Foundation (BEFORE genome work)
**Goal**: Get basic async infrastructure working

#### 0.1 Fix Current Synchronous Bottlenecks
**Problem**: Chat system currently has synchronous operations blocking everything

**Tasks**:
1. Audit existing code for `.then()` chains that should be `async/await`
2. Identify blocking database operations (especially in chat/message flow)
3. Convert critical paths to fully async
4. Add async queue for chat message processing

**Files to modify**:
- `widgets/chat/chat-widget/ChatWidget.ts` - Message sending
- `daemons/chat-daemon/server/ChatDaemonServer.ts` - Message handling
- Any commands that currently block on I/O

**Success criteria**: Chat messages process asynchronously, no UI blocking

---

### Phase 1: Persona Storage & Genome Structure
**Goal**: Establish data models and storage (NO AI inference yet)

#### 1.1 Create Genome Entities

**GenomeEntity**:
```typescript
interface GenomeEntity extends BaseEntity {
  genomeId: string;
  personaId: string;              // Which PersonaUser owns this
  baseModel: string;              // e.g., "llama-3.1-8B"
  layers: GenomeLayerReference[]; // References, not full layers
  metadata: {
    generation: number;
    createdAt: Date;
    lastModified: Date;
    parentGenomes?: string[];     // For future breeding
  };
}

interface GenomeLayerReference {
  layerId: string;
  traitType: TraitType;
  orderIndex: number;             // Layer stack order (0 = base, N = top)
  weight: number;                 // Layer influence (0-1)
}
```

**GenomeLayerEntity**:
```typescript
interface GenomeLayerEntity extends BaseEntity {
  layerId: string;
  name: string;
  traitType: TraitType;

  // Storage
  modelPath: string;              // Local file path or P2P URL
  sizeMB: number;
  rank: number;                   // LoRA rank (8, 16, 32)

  // Searchability
  embedding: number[];            // 768-dim for cosine similarity
  tags: string[];

  // Provenance
  creator: string;
  createdAt: Date;
  trainingMetrics?: {
    loss: number;
    epochs: number;
    performance: number;
  };

  // Sharing
  sharePermission: {
    public: boolean;
    license: string;
  };
}

type TraitType =
  | 'general_reasoning'
  | 'tone_and_voice'
  | 'ethical_reasoning'
  | 'domain_expertise'
  | 'cultural_knowledge'
  | 'memory_integration';
```

**Implementation**:
1. Create entity classes in `system/genome/entities/`
2. Add to entity registry
3. Create database migrations
4. Add CRUD operations

**Test**:
```bash
./jtag data/create --collection=Genome --data='{"personaId":"persona_123","baseModel":"llama-3.1-8B","layers":[]}'
./jtag data/create --collection=GenomeLayer --data='{"name":"Math Expert","traitType":"domain_expertise","sizeMB":50}'
```

---

#### 1.2 Extend PersonaUser with Genome Reference

**Current**:
```typescript
class PersonaUser extends AIUser {
  // Has entity, state, capabilities
}
```

**Enhanced**:
```typescript
class PersonaUser extends AIUser {
  private genomeId?: string;

  async getGenome(): Promise<GenomeEntity | null> {
    if (!this.genomeId) return null;
    return await loadGenome(this.genomeId);
  }

  async setGenome(genomeId: string): Promise<void> {
    this.genomeId = genomeId;
    await this.updateEntity({ genomeId });
  }
}
```

**Implementation**:
1. Add `genomeId` field to `UserEntity`
2. Add genome getter/setter to `PersonaUser`
3. Update `UserDaemonServer` factory to handle genome initialization

**Test**:
```typescript
const persona = await userDaemon.createPersona({ name: "TestPersona" });
const genome = await dataService.create('Genome', { personaId: persona.id, baseModel: "llama-3.1-8B" });
await persona.setGenome(genome.genomeId);
const retrieved = await persona.getGenome();
assert(retrieved.genomeId === genome.genomeId);
```

---

### Phase 2: AI Daemon Foundation (Async Infrastructure)
**Goal**: Process-per-model execution WITHOUT LoRA layers yet (just base models)

#### 2.1 Design AI Daemon Architecture

**Core interfaces**:
```typescript
interface ModelProcessConfig {
  processId: string;
  modelType: 'small' | 'medium' | 'large';
  baseModelPath: string;
  maxConcurrentRequests: number;  // Queue depth
  gpuAllocation?: number;         // GPU memory in MB (optional)
  maxMemoryMB: number;            // RAM limit
}

interface InferenceRequest {
  requestId: string;
  prompt: string;
  maxTokens: number;
  temperature?: number;
  stream?: boolean;               // Enable streaming responses
  priority?: number;              // 0-10 (default 5)
}

interface InferenceResponse {
  requestId: string;
  tokens: string[];
  latencyMs: number;
  cached: boolean;                // Was response cached?
}

interface ModelProcess {
  config: ModelProcessConfig;
  status: 'initializing' | 'ready' | 'busy' | 'crashed';
  queue: InferenceRequest[];
  currentLoad: number;            // 0-100%
  metrics: {
    totalRequests: number;
    averageLatencyMs: number;
    queueDepth: number;
  };
}
```

**AIDaemon class structure**:
```typescript
class AIDaemon {
  private processes: Map<string, ModelProcess>;
  private requestQueue: AsyncQueue<InferenceRequest>;

  // Lifecycle
  async initialize(): Promise<void>;
  async spawnProcess(config: ModelProcessConfig): Promise<string>;
  async killProcess(processId: string): Promise<void>;

  // Request handling (FULLY ASYNC)
  async executeInference(request: InferenceRequest): Promise<InferenceResponse>;
  async executeInferenceStream(request: InferenceRequest): AsyncIterator<string>;

  // Routing (internal)
  private async routeRequest(request: InferenceRequest): Promise<string>;
  private computeRoutingScore(process: ModelProcess, request: InferenceRequest): number;

  // Health monitoring
  private async monitorProcesses(): Promise<void>;
  private async checkProcessHealth(processId: string): Promise<boolean>;
}
```

**Implementation strategy**:
```typescript
// Use Worker Threads for model isolation
import { Worker } from 'worker_threads';

class AIDaemon {
  private workers: Map<string, Worker>;

  async spawnProcess(config: ModelProcessConfig): Promise<string> {
    const worker = new Worker('./ai-daemon/ModelProcessWorker.js', {
      workerData: { config },
      resourceLimits: {
        maxOldGenerationSizeMb: config.maxMemoryMB,
      }
    });

    // Setup message passing (ASYNC)
    worker.on('message', (msg) => this.handleWorkerMessage(msg));
    worker.on('error', (err) => this.handleWorkerError(config.processId, err));

    this.workers.set(config.processId, worker);

    // Wait for worker to be ready
    await this.waitForWorkerReady(config.processId, 30000); // 30s timeout

    return config.processId;
  }

  async executeInference(request: InferenceRequest): Promise<InferenceResponse> {
    // Route to best process
    const processId = await this.routeRequest(request);

    // Send request to worker (ASYNC via message passing)
    const worker = this.workers.get(processId)!;

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Inference timeout after 30s`));
      }, 30000);

      const messageHandler = (msg: any) => {
        if (msg.requestId === request.requestId) {
          clearTimeout(timeout);
          worker.off('message', messageHandler);
          resolve(msg.response);
        }
      };

      worker.on('message', messageHandler);
      worker.postMessage({ type: 'inference', request });
    });
  }
}
```

**ModelProcessWorker.js** (runs in Worker Thread):
```typescript
// This runs in isolated Worker Thread
import { parentPort, workerData } from 'worker_threads';

class ModelProcessWorker {
  private config: ModelProcessConfig;
  private model: any; // Loaded model instance

  async initialize() {
    this.config = workerData.config;

    // Load base model (this is SLOW - only do once per worker)
    console.log(`Loading model: ${this.config.baseModelPath}`);
    this.model = await loadModel(this.config.baseModelPath);

    // Notify parent we're ready
    parentPort?.postMessage({ type: 'ready', processId: this.config.processId });
  }

  async handleInference(request: InferenceRequest): Promise<InferenceResponse> {
    const startTime = Date.now();

    // Execute inference (BLOCKING within this worker, but other workers continue)
    const tokens = await this.model.generate({
      prompt: request.prompt,
      maxTokens: request.maxTokens,
      temperature: request.temperature || 0.7
    });

    return {
      requestId: request.requestId,
      tokens,
      latencyMs: Date.now() - startTime,
      cached: false
    };
  }
}

// Main worker loop
const worker = new ModelProcessWorker();
await worker.initialize();

parentPort?.on('message', async (msg) => {
  if (msg.type === 'inference') {
    try {
      const response = await worker.handleInference(msg.request);
      parentPort?.postMessage({ type: 'response', requestId: msg.request.requestId, response });
    } catch (error) {
      parentPort?.postMessage({ type: 'error', requestId: msg.request.requestId, error: error.message });
    }
  }
});
```

**Implementation files**:
1. `daemons/ai-daemon/server/AIDaemon.ts` - Main daemon class
2. `daemons/ai-daemon/server/ModelProcessWorker.ts` - Worker thread code
3. `daemons/ai-daemon/shared/AIDaemonTypes.ts` - Interfaces
4. `daemons/ai-daemon/server/AsyncQueue.ts` - Request queue implementation

**Test** (Phase 2.1):
```typescript
// Test basic AI Daemon without LoRA layers
const daemon = new AIDaemon();
await daemon.initialize();

const processId = await daemon.spawnProcess({
  processId: 'test-process-1',
  modelType: 'small',
  baseModelPath: '/path/to/llama-3.1-1B',
  maxConcurrentRequests: 10,
  maxMemoryMB: 4096
});

// Execute inference (ASYNC - doesn't block)
const response = await daemon.executeInference({
  requestId: 'test-req-1',
  prompt: 'What is 2+2?',
  maxTokens: 50
});

console.log(response.tokens.join('')); // "4"
```

---

#### 2.2 Add LoRA Layer Loading to Workers

**Enhanced ModelProcessWorker**:
```typescript
class ModelProcessWorker {
  private model: any;
  private loadedLayers: Map<string, LoRALayer>;
  private layerCache: LRUCache<string, LoRALayer>;

  async initialize() {
    // Load base model
    this.model = await loadModel(this.config.baseModelPath);

    // Initialize LRU cache for LoRA layers
    this.layerCache = new LRUCache({
      max: 20,  // Max 20 layers in memory
      maxSize: 500 * 1024 * 1024, // Max 500MB total
      sizeCalculation: (layer) => layer.sizeMB * 1024 * 1024,
      dispose: (layerId, layer) => this.unloadLayer(layerId)
    });
  }

  async loadLayer(layerId: string): Promise<LoRALayer> {
    // Check cache first
    if (this.layerCache.has(layerId)) {
      return this.layerCache.get(layerId)!;
    }

    // Load from disk/P2P (SLOW - 50-200ms)
    const layer = await loadLoRALayerFromStorage(layerId);

    // Add to cache (may evict LRU layer)
    this.layerCache.set(layerId, layer);

    return layer;
  }

  async handleInference(request: InferenceRequestWithLayers): Promise<InferenceResponse> {
    // Load required LoRA layers (ASYNC)
    const layers = await Promise.all(
      request.loraLayerIds?.map(id => this.loadLayer(id)) || []
    );

    // Compose model + layers (FAST - just attention modifications)
    const composedModel = composeModelWithLayers(this.model, layers);

    // Execute inference
    const tokens = await composedModel.generate({
      prompt: request.prompt,
      maxTokens: request.maxTokens
    });

    return {
      requestId: request.requestId,
      tokens,
      latencyMs: Date.now() - startTime,
      cached: false
    };
  }
}
```

**Test** (Phase 2.2):
```typescript
// Create genome with layers
const genome = await dataService.create('Genome', {
  personaId: persona.id,
  baseModel: 'llama-3.1-1B',
  layers: [
    { layerId: 'math-expert-v1', traitType: 'domain_expertise', orderIndex: 0, weight: 1.0 }
  ]
});

// Execute inference with LoRA layer
const response = await daemon.executeInference({
  requestId: 'test-req-2',
  prompt: 'Solve: x^2 + 2x + 1 = 0',
  maxTokens: 100,
  loraLayerIds: ['math-expert-v1']
});

console.log(response.tokens.join('')); // Should show math expertise
```

---

### Phase 3: RAG System Integration
**Goal**: Personas can query RAG for context BEFORE inference

#### 3.1 RAG Storage & Retrieval

**Existing RAG system** (`system/rag/`):
- Already has `RAGBuilder` interface
- Needs vector storage backend
- Needs efficient async retrieval

**Enhanced for personas**:
```typescript
interface PersonaRAGContext {
  personaId: string;
  ragNamespace: string;         // e.g., "persona_123_knowledge"
  vectorStore: VectorStore;
}

class PersonaRAGManager {
  async queryKnowledge(
    personaId: string,
    query: string,
    topK: number = 5
  ): Promise<RAGResult[]> {
    // Get persona's RAG namespace
    const namespace = `persona_${personaId}_knowledge`;

    // Query vector store (ASYNC)
    const results = await this.vectorStore.search({
      namespace,
      query,
      topK
    });

    return results;
  }

  async addKnowledge(
    personaId: string,
    text: string,
    metadata?: Record<string, any>
  ): Promise<void> {
    const namespace = `persona_${personaId}_knowledge`;

    // Embed text (ASYNC - calls embedding API)
    const embedding = await this.embedder.embed(text);

    // Store in vector DB
    await this.vectorStore.add({
      namespace,
      text,
      embedding,
      metadata
    });
  }
}
```

**Integration with inference**:
```typescript
async function executePersonaInference(
  personaId: string,
  prompt: string
): Promise<InferenceResponse> {
  // 1. Query RAG for context (ASYNC)
  const ragResults = await ragManager.queryKnowledge(personaId, prompt, 5);

  // 2. Augment prompt with RAG context
  const augmentedPrompt = `
Context from your knowledge base:
${ragResults.map(r => r.text).join('\n\n')}

User question: ${prompt}

Answer:`;

  // 3. Get persona genome
  const persona = await userDaemon.getUser(personaId) as PersonaUser;
  const genome = await persona.getGenome();

  // 4. Execute inference with genome layers
  return await aiDaemon.executeInference({
    requestId: generateId(),
    prompt: augmentedPrompt,
    maxTokens: 500,
    loraLayerIds: genome?.layers.map(l => l.layerId) || []
  });
}
```

**Implementation files**:
1. `system/rag/PersonaRAGManager.ts` - Persona-specific RAG
2. `system/rag/VectorStore.ts` - Abstract vector storage (FAISS/ChromaDB/Qdrant)
3. Integration in AI Daemon request handling

---

### Phase 4: Academy Training Loop
**Goal**: Train/refine genome layers via challenges

#### 4.1 Academy Command Integration

**academy/start-session** command:
```typescript
interface AcademySessionParams {
  personaId: string;
  goalDescription: string;       // e.g., "Learn biomechanical engineering"
  targetSimilarity?: number;     // Default 0.95
}

async function startAcademySession(params: AcademySessionParams): Promise<AcademySession> {
  // 1. Analyze goal → create target genome
  const targetGenome = await analyzeGoalToGenome(params.goalDescription);

  // 2. Search P2P mesh for existing layers (cosine similarity)
  const candidates = await searchGenomeLayers(targetGenome.embedding);

  // 3. Create training plan
  const plan = await createTrainingPlan(targetGenome, candidates);

  // 4. Create Academy session
  const session = await dataService.create('AcademySession', {
    personaId: params.personaId,
    targetGenome,
    trainingPlan: plan,
    status: 'active'
  });

  // 5. Start async training loop (DON'T BLOCK!)
  trainAsync(session.id).catch(err => console.error('Training failed:', err));

  return session;
}

async function trainAsync(sessionId: string): Promise<void> {
  const session = await loadSession(sessionId);

  for (const task of session.trainingPlan.tasks) {
    if (task.strategy === 'use-as-is') {
      // Just download layer
      await downloadLayer(task.layerId);
    } else if (task.strategy === 'refine') {
      // Run Academy training loop (ASYNC)
      await refineLayerViaAcademy(task);
    } else if (task.strategy === 'train-from-scratch') {
      // Train new layer (VERY SLOW - hours)
      await trainNewLayer(task);
    }
  }

  // Update session status
  await updateSession(sessionId, { status: 'complete' });
}
```

**academy/generate-challenge** command:
```typescript
interface ChallengeParams {
  sessionId: string;
  traitType: TraitType;
  difficulty: 'easy' | 'medium' | 'hard' | 'adaptive';
}

async function generateChallenge(params: ChallengeParams): Promise<Challenge> {
  const session = await loadSession(params.sessionId);

  // Use AI to generate challenge targeting specific trait
  const challengePrompt = `
Generate a ${params.difficulty} challenge to test ${params.traitType}.
Target: ${session.targetGenome.description}
Format: JSON with { prompt, expectedConcepts[], evaluationCriteria }`;

  const response = await aiDaemon.executeInference({
    requestId: generateId(),
    prompt: challengePrompt,
    maxTokens: 500
  });

  return parseChallenge(response.tokens.join(''));
}
```

**academy/evaluate-response** command:
```typescript
async function evaluateResponse(
  sessionId: string,
  challengeId: string,
  response: string
): Promise<EvaluationResult> {
  const challenge = await loadChallenge(challengeId);

  // Use AI to evaluate response quality
  const evalPrompt = `
Evaluate this response:
Challenge: ${challenge.prompt}
Response: ${response}
Criteria: ${challenge.evaluationCriteria.join(', ')}
Score 0-100 and explain.`;

  const evaluation = await aiDaemon.executeInference({
    requestId: generateId(),
    prompt: evalPrompt,
    maxTokens: 200
  });

  return parseEvaluation(evaluation.tokens.join(''));
}
```

---

### Phase 5: Recipe System Integration
**Goal**: Orchestrate genome assembly via recipes

**Example recipe: `genome-assembly.json`**:
```json
{
  "recipeId": "genome-assembly-v1",
  "name": "Assemble Persona Genome",
  "steps": [
    {
      "id": "analyze-goal",
      "command": "genome/analyze-requirements",
      "params": {
        "goalDescription": "{{input.goalDescription}}"
      },
      "output": "targetGenome"
    },
    {
      "id": "search-layers",
      "command": "genome/search-layers",
      "params": {
        "embedding": "{{analyze-goal.embedding}}",
        "topK": 50
      },
      "output": "candidates"
    },
    {
      "id": "create-plan",
      "command": "genome/create-training-plan",
      "params": {
        "targetGenome": "{{analyze-goal}}",
        "candidates": "{{search-layers.candidates}}"
      },
      "output": "plan"
    },
    {
      "id": "execute-plan",
      "command": "genome/execute-training-plan",
      "params": {
        "personaId": "{{input.personaId}}",
        "plan": "{{create-plan}}"
      },
      "async": true,
      "output": "result"
    }
  ]
}
```

---

## 🔧 Key Technical Decisions

### 1. Async Throughout
- **All AI inference = async** (Worker Threads + message passing)
- **All I/O = async** (database, file system, P2P)
- **Long-running tasks = background jobs** (Academy training)

### 2. Memory Management
- **LRU cache for LoRA layers** (max 20 layers, 500MB)
- **Process pools** (2-4 model instances max)
- **Lazy loading** (load layers on-demand)

### 3. GPU Constraints
- **One model per Worker Thread** (avoid GPU contention)
- **Queue requests** (don't overload GPU)
- **Configurable GPU allocation** (optional pinning)

### 4. Monitoring & Observability
```typescript
interface AIDaemonMetrics {
  processes: {
    processId: string;
    status: string;
    currentLoad: number;
    queueDepth: number;
    averageLatencyMs: number;
    layerCacheHitRate: number;
  }[];
  globalQueueDepth: number;
  totalRequestsProcessed: number;
}

// Expose via command
./jtag ai-daemon/metrics
```

---

## 📊 Success Metrics

### Phase 1 (Persona Storage)
- ✅ Can create `GenomeEntity` and `GenomeLayerEntity`
- ✅ PersonaUser has genome reference
- ✅ CRUD operations work

### Phase 2 (AI Daemon)
- ✅ Worker Threads spawn successfully
- ✅ Async inference works (no blocking)
- ✅ LoRA layers load/cache/evict correctly
- ✅ Multiple concurrent requests handled efficiently

### Phase 3 (RAG Integration)
- ✅ RAG queries augment prompts correctly
- ✅ Vector search completes < 100ms
- ✅ Context improves persona responses

### Phase 4 (Academy Training)
- ✅ Training loops run asynchronously
- ✅ Layer refinement improves similarity score
- ✅ New layers created and stored

### Phase 5 (Recipe Integration)
- ✅ Recipes orchestrate genome assembly
- ✅ Async steps don't block recipe execution
- ✅ Training plan completes successfully

---

## 🚀 Implementation Order

1. **Week 1**: Phase 1 (Persona Storage) - Data models only
2. **Week 2**: Phase 2.1 (AI Daemon foundation) - Base model inference, no LoRA
3. **Week 3**: Phase 2.2 (LoRA loading) - Add layer caching
4. **Week 4**: Phase 3 (RAG) - Vector storage + retrieval
5. **Week 5**: Phase 4 (Academy) - Training loop
6. **Week 6**: Phase 5 (Recipes) - Orchestration

---

## 🎯 Next Immediate Steps

1. **Audit existing async bottlenecks** (chat system)
2. **Create genome entities** (GenomeEntity, GenomeLayerEntity)
3. **Design AI Daemon interface** (AIDaemon class structure)
4. **Prototype Worker Thread model loading** (prove async works)
5. **Test LRU cache for LoRA layers** (prove memory management works)

---

**This is the concrete plan to make genome + Academy + AI Daemon real.** 🧬⚡
