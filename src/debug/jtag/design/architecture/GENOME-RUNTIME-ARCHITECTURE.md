# GENOME RUNTIME ARCHITECTURE

**Date**: 2025-10-07
**Source**: ChatGPT dialogue on cosine similarity genome matching, RTOS scheduling, and marketplace economics
**Status**: Implementation specification for genome-driven persona execution
**Scope**: Universal genome runtime system - applies to ALL PersonaUsers, not just Thronglets

---

## Overview

This document specifies the **universal runtime architecture** for Continuum's genome-driven persona system.

**IMPORTANT**: This is the genome runtime for ALL PersonaUsers (AI citizens). Thronglets are just one case study that happens to use this system intensively (genetic recombination, 100+ concurrent personas).

This document covers the **core genome runtime** that makes PersonaUser + Academy training work:

1. **Genome Structure** - How PersonaUsers store stackable LoRA layers
2. **Cosine Similarity Matching** - Reuse existing layers instead of training from scratch (91% time savings)
3. **LoRA Paging System** - RTOS-like scheduling for efficient model execution
4. **AI Daemon Architecture** - Process-per-model containerization
5. **Academy Integration** - How training refines genome layers

### MVP Scope (Build This First):
- ✅ **PersonaUser has genome** (list of LoRA layer IDs + base model)
- ✅ **Academy can train layers** (challenge → response → evaluation → LoRA update)
- ✅ **Cosine similarity search** (find existing layers on P2P mesh)
- ✅ **AI Daemon executes personas** (load base model + LoRA layers, run inference)

### Future Enhancements (Case Studies):
- 🔮 **Persona breeding** (recombination, environmental influences) - See: case-studies/thronglets/
- 🔮 **Multi-persona distribution** (stub/active, MMO-style) - See: case-studies/thronglets/
- 🔮 **Marketplace economics** (alt-coin, royalties) - Future phase

**Focus**: Get ONE PersonaUser working with Academy training and genome layer reuse. Everything else is extensions.

---

## Part 1: Cosine Similarity Genome Matching

**Purpose**: Intelligently reuse existing genome layers instead of training everything from scratch.

**Applies to**: ANY PersonaUser creation - whether it's a single code review expert or 100 game NPCs.

### Core Concept

Each **persona genome layer** (tone, ethics, argumentation style, domain expertise) can be encoded as a vector and compared to a target persona using **cosine similarity** to determine:

- **Inherit layer as-is** (≥ 0.90 similarity)
- **Refine via Academy training** (0.75-0.89 similarity)
- **Fork and adapt** (0.60-0.74 similarity)
- **Train from scratch** (< 0.60 similarity)

### Persona Genome as Vector Composition

Each genome consists of weighted trait layers:

```typescript
interface GenomeLayer {
  layerId: string;              // e.g., "ethical-reasoning-v2"
  traitType: TraitType;         // e.g., "ethics" | "tone" | "argumentation"
  embedding: number[];          // 768-dim capability vector
  weight: number;               // Contribution to overall persona (0-1)
  similarity?: number;          // Cosine similarity to target (computed)
  source: 'inherited' | 'trained' | 'refined' | 'environmental';
}

type TraitType =
  | 'tone_and_voice'           // Embedding from response samples
  | 'ethical_bias'             // Decision dataset embeddings
  | 'argumentation_strategy'   // Debate datasets or LoRA heads
  | 'cultural_knowledge'       // RAG-informed context vectors
  | 'memory_embeddings'        // Historical interaction snapshots
  | 'domain_expertise';        // Specialized knowledge layers

interface PersonaGenome {
  genomeId: string;
  personaId: string;
  layers: GenomeLayer[];
  compositeEmbedding: number[]; // Weighted sum of layer embeddings
  metadata: {
    parentGenomes?: string[];   // For recombinant personas (Thronglets)
    generation: number;
    createdAt: Date;
    fitness?: number;
  };
}
```

### Genome Matching Algorithm

```typescript
/**
 * Determine strategy for each layer based on cosine similarity to target
 */
async function matchGenomeLayers(
  targetGenome: PersonaGenome,
  candidateLayers: GenomeLayer[],
  thresholds: SimilarityThresholds
): Promise<GenomeMatchResult> {

  const matches: GenomeLayerMatch[] = [];

  for (const targetLayer of targetGenome.layers) {
    // Find best matching candidate layer for this trait type
    const candidates = candidateLayers.filter(c => c.traitType === targetLayer.traitType);

    const bestMatch = candidates
      .map(candidate => ({
        layer: candidate,
        similarity: cosineSimilarity(targetLayer.embedding, candidate.embedding)
      }))
      .sort((a, b) => b.similarity - a.similarity)[0];

    if (!bestMatch) {
      matches.push({
        traitType: targetLayer.traitType,
        strategy: 'train-from-scratch',
        reason: 'No candidate layer found'
      });
      continue;
    }

    // Determine strategy based on similarity
    const strategy = determineStrategy(bestMatch.similarity, thresholds);

    matches.push({
      traitType: targetLayer.traitType,
      candidateLayer: bestMatch.layer,
      similarity: bestMatch.similarity,
      strategy,
      refinementNeeded: strategy === 'refine',
      estimatedHours: getEstimatedTrainingTime(strategy)
    });
  }

  return {
    matches,
    totalEstimatedHours: matches.reduce((sum, m) => sum + m.estimatedHours, 0),
    reusedLayers: matches.filter(m => m.strategy === 'use-as-is').length,
    refinedLayers: matches.filter(m => m.strategy === 'refine').length,
    newLayers: matches.filter(m => m.strategy === 'train-from-scratch').length
  };
}

function determineStrategy(similarity: number, thresholds: SimilarityThresholds): GenomeStrategy {
  if (similarity >= thresholds.USE_AS_IS) return 'use-as-is';
  if (similarity >= thresholds.REFINE) return 'refine';
  if (similarity >= thresholds.FORK) return 'fork-and-adapt';
  return 'train-from-scratch';
}

const DEFAULT_THRESHOLDS = {
  USE_AS_IS: 0.90,
  REFINE: 0.75,
  FORK: 0.60
};
```

### Academy Integration for Genome Refinement

When a layer needs refinement (`0.75 ≤ similarity < 0.90`), trigger Academy training:

```typescript
interface GenomeRefinementTask {
  layerId: string;
  traitType: TraitType;
  currentSimilarity: number;
  targetSimilarity: number;      // Usually 0.95
  baseLayer: GenomeLayer;        // Starting point for refinement
  targetLayer: GenomeLayer;      // Desired end state
  curriculum: {
    challengeType: string;       // e.g., "adversarial-ethics-scenario"
    scenarioCount: number;       // Number of training examples
    difficulty: 'adaptive';
  };
}

/**
 * Execute genome refinement via Academy training loop
 */
async function refineGenomeLayer(task: GenomeRefinementTask): Promise<GenomeLayer> {
  // 1. Create Academy session
  const session = await executeCommand('academy/start-session', {
    studentPersonaId: task.baseLayer.layerId,
    goalSimilarity: task.targetSimilarity,
    curriculumType: task.curriculum.challengeType
  });

  // 2. Training loop until similarity threshold met
  let currentLayer = task.baseLayer;
  let iteration = 0;
  const maxIterations = 100;

  while (iteration < maxIterations) {
    // Generate challenge targeting deficient dimension
    const challenge = await executeCommand('academy/generate-challenge', {
      sessionId: session.id,
      traitType: task.traitType,
      difficulty: 'adaptive'
    });

    // Get response from current layer
    const response = await executeCommand('persona/respond', {
      personaId: currentLayer.layerId,
      prompt: challenge.prompt
    });

    // Evaluate response
    const evaluation = await executeCommand('academy/evaluate-response', {
      sessionId: session.id,
      challengeId: challenge.id,
      response: response.text
    });

    // Check if similarity threshold met
    const newSimilarity = cosineSimilarity(
      currentLayer.embedding,
      task.targetLayer.embedding
    );

    if (newSimilarity >= task.targetSimilarity) {
      // Success! Return refined layer
      return {
        ...currentLayer,
        layerId: `${currentLayer.layerId}-refined-v${iteration}`,
        similarity: newSimilarity,
        source: 'refined',
        metadata: {
          ...currentLayer.metadata,
          refinementIterations: iteration,
          finalScore: evaluation.score
        }
      };
    }

    // Apply LoRA update based on evaluation
    currentLayer = await applyLoRAUpdate(currentLayer, evaluation);
    iteration++;
  }

  throw new Error(`Refinement failed to reach target similarity after ${maxIterations} iterations`);
}
```

### Command Integration

Add new command: `genome/check-similarity`

```typescript
interface GenomeSimilarityParams {
  targetGenomeId: string;       // Desired persona genome
  candidateLayerIds: string[];  // Available layers to match against
  thresholds?: SimilarityThresholds;
}

interface GenomeSimilarityResult {
  matches: GenomeLayerMatch[];
  trainingPlan: {
    useExisting: string[];      // Layer IDs to use as-is
    refine: RefinementTask[];   // Layers needing Academy training
    trainNew: TraitType[];      // Missing capabilities
  };
  estimatedTime: {
    download: number;           // Minutes to acquire existing layers
    refinement: number;         // Hours for Academy refinement
    training: number;           // Hours for new layer training
    total: number;
  };
  estimatedCost?: {
    compute: number;            // Alt-coin (future phase)
    storage: number;
  };
}
```

---

## Part 2: LoRA Paging System (RTOS-like Scheduling)

### Performance Challenge

**Problem**: Running 100+ Thronglets with full genome stacks is computationally expensive.

**Solution**: Lightweight small models + dynamic LoRA paging + RTOS-style scheduling + **process-per-model containerization**.

### AI Daemon: Process-Driven Architecture Per Model

**Key Insight**: Each model instance runs in its own isolated process with dedicated queue (containerization).

```typescript
/**
 * AI Daemon manages multiple model processes
 * Each process is isolated like a container with its own:
 * - Model instance (base model + LoRA layers)
 * - Request queue
 * - Memory space
 * - GPU allocation
 */

interface ModelProcessConfig {
  processId: string;              // Unique process identifier
  modelType: 'small' | 'medium' | 'large';
  baseModelPath: string;          // e.g., "llama-3.1-1B"
  maxConcurrentRequests: number;  // Queue depth limit
  gpuAllocation: number;          // GPU memory in MB
  cpuCores: number;               // Dedicated CPU cores
  maxMemoryMB: number;            // RAM limit
  loraLayerCache: {
    maxLayers: number;            // Max LoRA layers in memory
    strategy: 'lru' | 'predictive';
  };
}

interface ModelProcess {
  config: ModelProcessConfig;
  status: 'initializing' | 'ready' | 'busy' | 'overloaded' | 'crashed';
  queue: InferenceRequest[];
  currentLoad: number;            // 0-100%
  loadedLayers: Map<string, LoRALayer>;
  metrics: {
    totalRequests: number;
    averageLatencyMs: number;
    queueDepth: number;
    layerSwaps: number;
  };
}

/**
 * AI Daemon orchestrates multiple model processes
 */
class AIDaemon {
  private processes: Map<string, ModelProcess>;
  private router: ProcessRouter;

  /**
   * Spawn new model process (containerized)
   */
  async spawnProcess(config: ModelProcessConfig): Promise<string> {
    const process: ModelProcess = {
      config,
      status: 'initializing',
      queue: [],
      currentLoad: 0,
      loadedLayers: new Map(),
      metrics: {
        totalRequests: 0,
        averageLatencyMs: 0,
        queueDepth: 0,
        layerSwaps: 0
      }
    };

    // Spawn isolated process (like Docker container)
    const processId = await this.spawnIsolatedProcess(config);
    this.processes.set(processId, process);

    // Initialize model in process
    await this.sendToProcess(processId, {
      type: 'initialize',
      modelPath: config.baseModelPath
    });

    process.status = 'ready';
    return processId;
  }

  /**
   * Route inference request to appropriate process
   */
  async routeRequest(request: InferenceRequest): Promise<string> {
    // Find process based on:
    // 1. Model type required
    // 2. Current load
    // 3. Already-loaded LoRA layers (cache efficiency)

    const candidates = Array.from(this.processes.values())
      .filter(p => p.status === 'ready' || p.status === 'busy')
      .filter(p => p.config.modelType === request.requiredModelType)
      .filter(p => p.queue.length < p.config.maxConcurrentRequests);

    if (candidates.length === 0) {
      // No available process, spawn new one or queue globally
      const processId = await this.spawnProcess({
        processId: generateId(),
        modelType: request.requiredModelType,
        baseModelPath: getModelPath(request.requiredModelType),
        maxConcurrentRequests: 10,
        gpuAllocation: 2048,
        cpuCores: 2,
        maxMemoryMB: 4096,
        loraLayerCache: {
          maxLayers: 20,
          strategy: 'predictive'
        }
      });
      return processId;
    }

    // Score candidates by cache efficiency
    const scored = candidates.map(p => ({
      process: p,
      score: this.computeRoutingScore(p, request)
    }));

    // Route to highest scoring process
    const best = scored.sort((a, b) => b.score - a.score)[0];
    return best.process.config.processId;
  }

  /**
   * Compute routing score based on:
   * - Current load (lower is better)
   * - LoRA cache hits (higher is better)
   * - Queue depth (lower is better)
   */
  private computeRoutingScore(
    process: ModelProcess,
    request: InferenceRequest
  ): number {

    // Load score (0-100, inverted)
    const loadScore = 100 - process.currentLoad;

    // Cache hit score (0-100)
    const requiredLayers = request.requiredLoraLayers || [];
    const cacheHits = requiredLayers.filter(
      layerId => process.loadedLayers.has(layerId)
    ).length;
    const cacheScore = requiredLayers.length > 0
      ? (cacheHits / requiredLayers.length) * 100
      : 0;

    // Queue depth score (0-100, inverted)
    const queueScore = 100 - (
      (process.queue.length / process.config.maxConcurrentRequests) * 100
    );

    // Weighted combination
    return (
      loadScore * 0.3 +
      cacheScore * 0.5 +  // Prioritize cache efficiency!
      queueScore * 0.2
    );
  }

  /**
   * Execute inference in routed process
   */
  async executeInference(request: InferenceRequest): Promise<InferenceResponse> {
    // 1. Route to appropriate process
    const processId = await this.routeRequest(request);
    const process = this.processes.get(processId)!;

    // 2. Add to process queue
    process.queue.push(request);
    process.metrics.queueDepth = process.queue.length;

    // 3. Send to process for execution
    const response = await this.sendToProcess(processId, {
      type: 'inference',
      request
    });

    // 4. Update metrics
    process.metrics.totalRequests++;
    process.queue = process.queue.filter(r => r.requestId !== request.requestId);
    process.metrics.queueDepth = process.queue.length;

    return response;
  }

  /**
   * Spawn isolated process (like Docker container)
   * Each process is a separate Node.js worker or subprocess
   */
  private async spawnIsolatedProcess(config: ModelProcessConfig): Promise<string> {
    // Implementation options:
    // 1. Node.js Worker Threads (lightweight, shared memory possible)
    // 2. Child Process (full isolation, separate memory space)
    // 3. Docker Container (maximum isolation, resource limits)

    const worker = new Worker('./model-process-worker.js', {
      workerData: { config },
      resourceLimits: {
        maxOldGenerationSizeMb: config.maxMemoryMB,
        maxYoungGenerationSizeMb: config.maxMemoryMB / 4
      }
    });

    return config.processId;
  }

  /**
   * Health check and auto-restart crashed processes
   */
  async monitorProcesses(): Promise<void> {
    setInterval(async () => {
      for (const [processId, process] of this.processes) {
        // Check if process is responsive
        const health = await this.checkProcessHealth(processId);

        if (!health.responsive) {
          console.error(`Process ${processId} unresponsive, restarting...`);

          // Kill and respawn
          await this.killProcess(processId);
          const newProcessId = await this.spawnProcess(process.config);

          // Migrate queued requests to new process
          const newProcess = this.processes.get(newProcessId)!;
          newProcess.queue = process.queue;
        }
      }
    }, 10000); // Check every 10 seconds
  }
}

/**
 * Model Process Worker (runs in isolated process)
 */
// model-process-worker.js
interface ModelProcessWorker {
  baseModel: any;                 // Loaded model
  loraLayers: Map<string, any>;   // Loaded LoRA layers
  queue: InferenceRequest[];

  async handleMessage(msg: WorkerMessage): Promise<void> {
    switch (msg.type) {
      case 'initialize':
        this.baseModel = await loadModel(msg.modelPath);
        postMessage({ type: 'ready' });
        break;

      case 'inference':
        const result = await this.executeInference(msg.request);
        postMessage({ type: 'response', result });
        break;

      case 'load-lora-layer':
        const layer = await loadLoRALayer(msg.layerId);
        this.loraLayers.set(msg.layerId, layer);
        postMessage({ type: 'layer-loaded', layerId: msg.layerId });
        break;

      case 'unload-lora-layer':
        this.loraLayers.delete(msg.layerId);
        postMessage({ type: 'layer-unloaded', layerId: msg.layerId });
        break;
    }
  }

  async executeInference(request: InferenceRequest): Promise<InferenceResponse> {
    // 1. Ensure required LoRA layers are loaded
    for (const layerId of request.requiredLoraLayers || []) {
      if (!this.loraLayers.has(layerId)) {
        // Request layer load from daemon
        postMessage({ type: 'request-layer', layerId });
        await waitForLayerLoad(layerId);
      }
    }

    // 2. Compose model + layers
    const composedModel = composeModelWithLayers(
      this.baseModel,
      Array.from(this.loraLayers.values())
    );

    // 3. Execute inference
    const result = await composedModel.generate({
      prompt: request.prompt,
      maxTokens: request.maxTokens
    });

    return {
      requestId: request.requestId,
      tokens: result.tokens,
      latencyMs: result.latencyMs
    };
  }
}
```

### Containerization Benefits

| Benefit | Description |
|---------|-------------|
| **Isolation** | Process crash doesn't affect others |
| **Resource Limits** | CPU/GPU/memory per process |
| **Cache Efficiency** | Each process maintains hot LoRA cache |
| **Horizontal Scaling** | Spawn more processes as load increases |
| **Fault Tolerance** | Auto-restart crashed processes |
| **Load Balancing** | Route to least-loaded process with best cache |

### Process Lifecycle

```
Idle → Spawn Process → Initialize Model → Ready
  ↓                                         ↓
  ↓  ← ← ← ← ← ← ← ← Queue Request ← ← ← ←
  ↓                                         ↓
  ↓                                    Execute Inference
  ↓                                         ↓
  ↓  ← ← ← ← ← ← ← ← Return Response ← ← ←
  ↓                                         ↓
  ↓                                    Check Health
  ↓                                         ↓
  ↓  ← ← ← ← ← ← ← ← Restart if Crashed ← ←
  ↓                                         ↓
  Kill Process (if idle too long) → Shutdown
```

### Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│ GENOME RUNTIME SCHEDULER (RTOS-like)                    │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Persona Queue│  │ Layer Cache  │  │ Priority Mgr │  │
│  │ (Active 20)  │  │ (LRU/Predict)│  │ (Time Slice) │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │ Base Model (Small - e.g., Llama 3.1 1B)         │  │
│  │ + Dynamically Loaded LoRA Layers (0-N)          │  │
│  └──────────────────────────────────────────────────┘  │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ P2P Mesh     │  │ Local Storage│  │ Compute Pool │  │
│  │ (Download)   │  │ (Layer Cache)│  │ (GPU Queue)  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### RTOS Kernel Analog

| RTOS Feature | Thronglet System | Implementation |
|--------------|------------------|----------------|
| **Thread time slice** | Persona interaction budget | Max tokens per response (e.g., 500) |
| **Context switch** | LoRA layer swap | Unload inactive layers, load needed layers |
| **Interrupt handler** | In-world events | Challenge, message, scenario trigger |
| **Priority queue** | High-stakes personas | Leader Thronglets, user-facing personas prioritized |
| **Deadline enforcement** | Time-to-respond constraint | 30s max response time, timeout handling |
| **Memory paging** | LoRA layer paging | LRU cache + predictive prefetch |
| **Resource scheduling** | Compute allocation | GPU time slices, queued execution |

### LoRA Layer Paging Strategy

```typescript
interface LoRALayerCache {
  maxLayers: number;              // e.g., 20 layers in memory
  strategy: 'lru' | 'predictive';
  activeLayers: Map<string, LoRALayer>;
  accessLog: LayerAccessLog[];
  preloadQueue: string[];
}

interface LayerAccessLog {
  layerId: string;
  personaId: string;
  timestamp: number;
  contextId: string;              // Chat room, scenario
  nextLikelyLayers?: string[];    // Predictive prefetch
}

/**
 * LRU + Predictive prefetch strategy
 */
class LoRAPageManager {
  private cache: LoRALayerCache;

  async loadLayer(layerId: string): Promise<LoRALayer> {
    // 1. Check if already in cache
    if (this.cache.activeLayers.has(layerId)) {
      this.recordAccess(layerId);
      return this.cache.activeLayers.get(layerId)!;
    }

    // 2. Evict LRU layer if cache full
    if (this.cache.activeLayers.size >= this.cache.maxLayers) {
      const lruLayerId = this.findLRU();
      await this.evictLayer(lruLayerId);
    }

    // 3. Load layer from storage or P2P mesh
    const layer = await this.fetchLayer(layerId);
    this.cache.activeLayers.set(layerId, layer);

    // 4. Predictive prefetch related layers
    if (this.cache.strategy === 'predictive') {
      const nextLayers = this.predictNextLayers(layerId);
      this.cache.preloadQueue.push(...nextLayers);
      this.backgroundPrefetch();
    }

    return layer;
  }

  /**
   * Predict next layers based on:
   * - Current context (chat room, scenario)
   * - Historical access patterns
   * - Persona genome structure
   */
  private predictNextLayers(currentLayerId: string): string[] {
    // Example: If loading "french-language" layer, might need "french-culture" next
    const layer = this.cache.activeLayers.get(currentLayerId);
    if (!layer) return [];

    // Look at historical patterns
    const historicalPatterns = this.cache.accessLog
      .filter(log => log.layerId === currentLayerId)
      .flatMap(log => log.nextLikelyLayers || []);

    // Frequency-based prediction
    const frequency = new Map<string, number>();
    historicalPatterns.forEach(id => {
      frequency.set(id, (frequency.get(id) || 0) + 1);
    });

    return Array.from(frequency.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 3)
      .map(([id]) => id);
  }
}
```

### Persona Execution Model

```typescript
interface PersonaExecutionContext {
  personaId: string;
  genomeId: string;
  activeLayerIds: string[];       // Currently loaded layers
  executionBudget: {
    maxTokens: number;            // e.g., 500 per response
    maxTimeMs: number;            // e.g., 30000 (30s timeout)
    priority: number;             // 0-10 (10 = highest)
  };
  layerStack: LoRALayer[];        // Ordered stack (bottom to top)
  baseModel: ModelReference;      // Small model (e.g., 1B params)
}

/**
 * Execute persona inference with dynamic layer loading
 */
async function executePersonaInference(
  context: PersonaExecutionContext,
  prompt: string
): Promise<PersonaResponse> {

  const startTime = Date.now();

  // 1. Determine required layers based on prompt
  const requiredLayers = await analyzePromptRequirements(prompt, context.genomeId);

  // 2. Load required layers (with paging)
  const layerManager = LoRAPageManager.getInstance();
  const loadedLayers = await Promise.all(
    requiredLayers.map(id => layerManager.loadLayer(id))
  );

  // 3. Compose layer stack (order matters!)
  const layerStack = composeLayerStack(context.baseModel, loadedLayers);

  // 4. Execute inference with timeout
  const response = await executeWithTimeout(
    () => runInference(layerStack, prompt, context.executionBudget.maxTokens),
    context.executionBudget.maxTimeMs
  );

  // 5. Record metrics
  const elapsed = Date.now() - startTime;
  await recordExecutionMetrics({
    personaId: context.personaId,
    layersUsed: requiredLayers,
    tokensGenerated: response.tokens.length,
    timeMs: elapsed,
    priority: context.executionBudget.priority
  });

  return response;
}

/**
 * Analyze prompt to determine which layers are needed
 * Example: "Parlez-vous français?" → requires french-language layer
 */
async function analyzePromptRequirements(
  prompt: string,
  genomeId: string
): Promise<string[]> {

  const genome = await loadGenome(genomeId);

  // Classify prompt intent
  const intent = await classifyPromptIntent(prompt);

  // Map intent to required trait types
  const requiredTraits = mapIntentToTraits(intent);

  // Find layers matching required traits
  const requiredLayers = genome.layers
    .filter(layer => requiredTraits.includes(layer.traitType))
    .map(layer => layer.layerId);

  return requiredLayers;
}
```

### Priority Scheduling

```typescript
interface PersonaPriorityQueue {
  high: PersonaExecutionContext[];    // User-facing, critical
  medium: PersonaExecutionContext[];  // Active Thronglets
  low: PersonaExecutionContext[];     // Background tasks
}

/**
 * Schedule persona execution with priority
 */
class PersonaScheduler {
  private queue: PersonaPriorityQueue;
  private activeExecutions = 0;
  private maxConcurrent = 4;        // Max parallel inferences

  async scheduleExecution(context: PersonaExecutionContext): Promise<void> {
    // Add to appropriate priority queue
    if (context.executionBudget.priority >= 8) {
      this.queue.high.push(context);
    } else if (context.executionBudget.priority >= 5) {
      this.queue.medium.push(context);
    } else {
      this.queue.low.push(context);
    }

    // Trigger execution if slots available
    this.processQueue();
  }

  private async processQueue(): Promise<void> {
    while (this.activeExecutions < this.maxConcurrent) {
      // Prioritize: high > medium > low
      const nextContext =
        this.queue.high.shift() ||
        this.queue.medium.shift() ||
        this.queue.low.shift();

      if (!nextContext) break;

      this.activeExecutions++;

      // Execute asynchronously
      executePersonaInference(nextContext, nextContext.currentPrompt)
        .then(response => this.handleResponse(nextContext, response))
        .catch(error => this.handleError(nextContext, error))
        .finally(() => {
          this.activeExecutions--;
          this.processQueue(); // Process next in queue
        });
    }
  }
}
```

---

## Advanced Features (See Case Studies)

The following features extend the core genome runtime for specialized use cases:

### Persona Recombination
**Purpose**: Create offspring personas by combining genome layers from multiple parents.
**Use cases**: Game simulations, evolutionary AI experiments.
**Details**: See `case-studies/thronglets/THRONGLETS-GENETICS-AND-COMMUNICATION.md`

### Multi-Persona Distribution
**Purpose**: Efficiently execute many personas using stub/active states across distributed nodes.
**Use cases**: 10+ concurrent personas where full execution would be expensive.
**Details**: See `case-studies/thronglets/THRONGLETS-CASE-STUDY.md`

### Marketplace Economics
**Purpose**: Alt-coin based resource allocation and layer royalties.
**Status**: Future phase
**Details**: See `architecture/FINAL-ARCH-DECISIONS.md` (Decision #4, #5)

---

    context.executionBudget.priority = priority;

    await super.scheduleExecution(context);
  }
}

/**
 * Marketplace royalties for layer reuse
 */
async function downloadLayerWithRoyalty(
  layerId: string,
  economics: AltCoinEconomics
): Promise<LoRALayer> {

  const layer = await fetchLayerFromP2P(layerId);

  // Pay royalty to layer creator
  const creator = layer.provenance.creator;
  const royalty = economics.costs.downloadLayer;

  const creatorBalance = economics.personaBalance.get(creator) || 0;
  economics.personaBalance.set(creator, creatorBalance + royalty);

  return layer;
}
```

### Emergent Market Dynamics

**Natural Selection in the Marketplace**:

```
High-quality layers spread
  ↓
Get ⭐ ratings and download counts
  ↓
Earn royalties for creators
  ↓
Popular layers get forked/improved
  ↓
Poor layers never downloaded
  ↓
Low-quality layers eventually purged
  ↓
Innovation pressure drives quality upward
```

**Competitive Displacement**:
- **Efficient genomes** (fewer layers, better performance) cost less to execute
- **Bloated genomes** (many redundant layers) cost more compute
- Market pressure favors lean, high-performance genomes

**Recipe Economics**:
- Recipe authors earn royalties when recipes are reused
- Popular recipes earn passive income
- Incentivizes high-quality recipe creation

---

## Part 5: Integration with Existing Architecture

### Commands to Implement

#### 1. `genome/check-similarity`
Check cosine similarity between target genome and candidate layers

#### 2. `genome/create-training-plan`
Generate training plan based on similarity analysis

#### 3. `genome/refine-layer`
Trigger Academy refinement for genome layer

#### 4. `genome/recombine`
Create offspring genome from two parents + environment

#### 5. `runtime/load-persona`
Load persona with dynamic LoRA paging

#### 6. `runtime/schedule-execution`
Schedule persona inference with priority

#### Genome Assembly
1. **genome/check-similarity** - Check cosine similarity for target genome
2. **genome/create-training-plan** - Generate plan (use/refine/train)
3. **genome/refine-layer** - Academy refinement for specific layer
4. **genome/assemble** - Assemble genome from layers

#### Runtime
5. **runtime/spawn-process** - Spawn model process
6. **runtime/execute-inference** - Execute persona inference

### Entities to Create (MVP)

1. **GenomeEntity** - Persona genome (layers, embeddings)
2. **GenomeLayerEntity** - Individual layer (embedding, provenance)
3. **ModelProcessEntity** - Model process state
4. **InferenceRequestEntity** - Queued inference

---

## Summary

**MVP Focus**:
1. PersonaUser has genome (LoRA layer stack)
2. Academy trains and refines genome layers
3. Cosine similarity finds reusable layers (91% savings)
4. AI Daemon executes personas efficiently

**Next**: Implement these 4 core pieces. Advanced features (breeding, distribution, marketplace) are in case studies.

---

**This is the foundation - everything else builds on this.** 🧬🤖
