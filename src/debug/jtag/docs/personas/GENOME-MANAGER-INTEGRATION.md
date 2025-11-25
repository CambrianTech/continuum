# GenomeManager Integration with AIProviderDaemon

**Date**: 2025-10-29 (Original), Updated: 2025-11-22
**Status**: Research & Architecture Design + RTOS Integration
**Phase**: 7.0 MVP (Interface structure only)

---

## Executive Summary

The **AIProviderDaemon** already exists and handles:
- AI model inference (Ollama, OpenAI, Anthropic, etc.)
- Provider adapter registration and health monitoring
- Request routing and failover
- Cost tracking and usage logging

The **GenomeManager** (just created in Phase 7) needs to coordinate:
- GPU resource tracking (total availability across all GPUs)
- LoRA adapter paging (load/unload genomic layers dynamically)
- Training job queue (prevent GPU oversubscription)
- Base model memory tracking (llama3.2:3b = ~2GB, qwen2.5:14b = ~8GB)

**KEY INSIGHT**: These systems should **collaborate**, not compete. AIProviderDaemon handles **inference**, GenomeManager handles **resource orchestration**.

## RTOS Architecture (Updated 2025-11-22)

### GenomeDaemon as RTOS Subprocess

GenomeManager operations now run as a **non-blocking RTOS subprocess** for performance:

```typescript
// GenomeDaemon extends PersonaSubprocess pattern
export class GenomeDaemon extends PersonaSubprocess<GenomeTask> {
  protected async handleTask(task: GenomeTask): Promise<boolean> {
    switch (task.type) {
      case 'activate-adapter':
        return await this.activateAdapter(task.adapterId, task.personaId);
      case 'evict-adapter':
        return await this.evictLRU();
      case 'check-memory-pressure':
        return await this.checkMemoryPressure();
      case 'start-training':
        return await this.startTraining(task.trainingJob);
    }
  }
}
```

**Key Properties:**
- **< 1ms activation time** (signal-based, not blocking main thread)
- **Context-adaptive priority** (slows down during active inference)
- **Signal-triggered** (only processes when memory pressure > 0.8 or training requested)
- **Non-blocking** (runs in parallel with PersonaUser cognition)

### Integration with Autonomous Loop

GenomeDaemon integrates with PersonaUser's `serviceInbox()` method:

```typescript
// PersonaUser.serviceInbox() calls GenomeDaemon
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);

  // ... select task

  // 4. GENOME PAGING: Activate skill (< 1ms signal to GenomeDaemon)
  await this.genome.activateSkill(task.domain);

  // ... process task

  // 8. Evict adapters if memory pressure (< 1ms signal)
  if (this.genome.memoryPressure > 0.8) {
    await GenomeDaemon.shared().evictLRU();
  }
}
```

**Result**: Genome operations don't block PersonaUser's main loop.

---

## Architecture Discovery

### AIProviderDaemon: Current State

**Location**: `daemons/ai-provider-daemon/`

**Responsibilities**:
```typescript
class AIProviderDaemon extends DaemonBase {
  // Adapter registry for AI providers
  private adapters: Map<string, ProviderRegistration> = new Map();

  // Text generation routing (with ProcessPool integration)
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // 1. Select provider (Ollama, OpenAI, etc.)
    // 2. Route through ProcessPool if available (server-side)
    // 3. Or call adapter directly (browser-side fallback)
    // 4. Log generation to database for cost tracking
  }

  // Health monitoring for all providers
  async checkProviderHealth(): Promise<Map<string, HealthStatus>> { }

  // Provider registration (called by server subclass)
  protected async registerAdapter(adapter: AIProviderAdapter, options: {...}): Promise<void> { }
}
```

**Clean Static Interface** (lines 522-600):
```typescript
// Usage anywhere in codebase:
const response = await AIProviderDaemon.generateText({
  messages: [{ role: 'user', content: 'Hello!' }],
  model: 'llama3.2:1b',
  preferredProvider: 'ollama'
});
```

**ProcessPool Integration** (lines 143-183):
- AIProviderDaemon checks for `getProcessPoolInstance()` (server-side only)
- Routes inference through ProcessPool worker threads if available
- Falls back to direct adapter call if ProcessPool unavailable

**Key Discovery**: AIProviderDaemon already has hooks for resource management via ProcessPool!

### OllamaAdapter: Local Model Management

**Location**: `daemons/ai-provider-daemon/adapters/ollama/shared/OllamaAdapter.ts`

**Current Features**:
- Request queue with maxConcurrent=4 (lines 50-138)
- Health monitoring with 30s interval (BaseAIProviderAdapter)
- Automatic restart on failure (killall ollama && ollama serve)
- Model unloading for fresh state (lines 423-446)

**Request Queue** (lines 50-138):
```typescript
class OllamaRequestQueue {
  private queue: Array<QueuedRequest> = [];
  private activeRequests = 0;
  private readonly maxConcurrent: number = 4;
  private readonly REQUEST_TIMEOUT = 90000; // 90s

  async enqueue<T>(executor: () => Promise<T>, requestId: string): Promise<T> {
    // Prevents Ollama overload by limiting concurrent requests
  }
}
```

**Key Discovery**: OllamaAdapter already does basic traffic management, but doesn't know about:
- Total GPU memory limits
- Base model memory usage
- LoRA adapter memory usage
- Cross-provider coordination

---

## Integration Strategy: Two Systems, One Goal

### Division of Responsibilities

```
┌─────────────────────────────────────────────────────────────┐
│                      AIProviderDaemon                        │
│  "I handle inference requests across all providers"         │
├─────────────────────────────────────────────────────────────┤
│  • Provider adapter registration (Ollama, OpenAI, etc.)     │
│  • Request routing and failover                              │
│  • Health monitoring and recovery                            │
│  • Cost tracking and usage logging                           │
│  • ProcessPool coordination (worker threads)                 │
└─────────────────────────────────────────────────────────────┘
                            ↕
                   (Coordinates with)
                            ↕
┌─────────────────────────────────────────────────────────────┐
│                      GenomeManager                           │
│  "I orchestrate GPU resources holistically"                  │
├─────────────────────────────────────────────────────────────┤
│  • GPU resource tracking (total memory across GPUs)          │
│  • Base model memory tracking (llama3.2:3b = 2GB, etc.)     │
│  • LoRA adapter paging (load/unload genomic layers)         │
│  • Training job queue (prevent GPU oversubscription)         │
│  • Coordination across ALL AI operations                     │
└─────────────────────────────────────────────────────────────┘
```

### Integration Points

#### 1. **GPU Memory Awareness** (GenomeManager → AIProviderDaemon)

**Problem**: OllamaAdapter doesn't know how much GPU memory is available or what's already loaded.

**Solution**: GenomeManager provides memory state to AIProviderDaemon:

```typescript
// GenomeManager.ts
export interface SystemMemoryState {
  // GPU resources
  totalMemoryMB: number;
  availableMemoryMB: number;

  // Base models currently loaded
  loadedBaseModels: Map<string, {
    modelName: string;        // 'llama3.2:3b'
    memoryUsageMB: number;    // ~2048 MB
    usedByPersonas: Set<UUID>; // Which PersonaUsers are using this
    lastUsedAt: number;
  }>;

  // LoRA adapters currently loaded
  loadedAdapters: Map<string, LoadedAdapter>;
}

// NEW METHOD: Query current memory state
getMemoryState(): SystemMemoryState {
  return {
    totalMemoryMB: this.getTotalMemoryMB(),
    availableMemoryMB: this.getTotalAvailableMemoryMB(),
    loadedBaseModels: this.baseModels,
    loadedAdapters: this.loadedAdapters
  };
}
```

**Usage in AIProviderDaemon**:
```typescript
// AIProviderDaemon.ts
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // NEW: Check with GenomeManager before inference
  const memoryState = GenomeManager.shared().getMemoryState();

  // Check if model is already loaded
  const modelAlreadyLoaded = memoryState.loadedBaseModels.has(request.model);

  if (!modelAlreadyLoaded) {
    // Estimate model size
    const estimatedMemoryMB = this.estimateModelSize(request.model);

    // Check if we have enough memory
    if (memoryState.availableMemoryMB < estimatedMemoryMB) {
      // Coordinate with GenomeManager to free up space
      await GenomeManager.shared().evictBaseModel(estimatedMemoryMB);
    }

    // Register model load with GenomeManager
    await GenomeManager.shared().registerModelLoad(request.model, estimatedMemoryMB);
  }

  // Proceed with inference (existing code)
  const response = await this.selectAdapter(request.preferredProvider).generateText(request);

  // Update last used timestamp
  GenomeManager.shared().updateModelUsage(request.model);

  return response;
}
```

#### 2. **Base Model Eviction** (GenomeManager coordinates with Ollama)

**Problem**: When GPU memory is full, need to unload base models before loading new ones or LoRA adapters.

**Solution**: GenomeManager orchestrates eviction, Ollama executes it:

```typescript
// GenomeManager.ts
async evictBaseModel(requiredMemoryMB: number): Promise<void> {
  console.log(`🗑️ GenomeManager: Need ${requiredMemoryMB}MB, evicting base models...`);

  // Find least recently used base model
  let oldestModel: string | null = null;
  let oldestTime = Infinity;

  for (const [modelName, info] of this.baseModels) {
    if (info.usedByPersonas.size === 0 && info.lastUsedAt < oldestTime) {
      oldestTime = info.lastUsedAt;
      oldestModel = modelName;
    }
  }

  if (oldestModel) {
    console.log(`🗑️ GenomeManager: Evicting base model ${oldestModel} (last used ${Date.now() - oldestTime}ms ago)`);

    // Call Ollama to unload model
    await this.unloadBaseModel(oldestModel);
  }
}

// NEW METHOD: Unload base model via Ollama command
private async unloadBaseModel(modelName: string): Promise<void> {
  const { execSync } = await import('child_process');
  execSync(`ollama stop ${modelName}`, { stdio: 'ignore', timeout: 2000 });

  // Remove from tracking
  const info = this.baseModels.get(modelName);
  if (info) {
    this.baseModels.delete(modelName);
    console.log(`✅ GenomeManager: Freed ${info.memoryUsageMB}MB by unloading ${modelName}`);
  }
}
```

#### 3. **Training Coordination** (GenomeManager → OllamaAdapter)

**Problem**: LoRA training requires GPU resources. Need to ensure training doesn't conflict with inference.

**Solution**: GenomeManager checks inference load before starting training:

```typescript
// GenomeManager.ts
async submitTrainingJob(
  request: LoRATrainingRequest,
  providerId: string,
  priority: number = 50
): Promise<TrainingJob> {
  const trainer = this.adapters.get(providerId);
  if (!trainer) {
    throw new Error(`No trainer adapter registered for provider: ${providerId}`);
  }

  // NEW: Check current inference load
  const currentInferenceLoad = await this.getInferenceLoad();

  // Estimate training memory
  const estimatedMemoryMB = this.estimateTrainingMemory(request);

  // Check if we can start training without impacting inference
  const canStartNow =
    this.canStartTraining(estimatedMemoryMB) &&
    currentInferenceLoad < 0.5; // Don't train if inference is busy

  if (canStartNow) {
    await this.startTraining(job, request, trainer);
  } else {
    // Queue for later (when inference quiets down)
    this.trainingQueue.push({ job, request, trainer, priority });
  }

  return job;
}

// NEW METHOD: Get current inference load from AIProviderDaemon
private async getInferenceLoad(): Promise<number> {
  // Query OllamaAdapter's request queue
  const ollamaAdapter = AIProviderDaemon.getProviders().includes('ollama');
  if (!ollamaAdapter) return 0;

  // TODO: Add API to OllamaAdapter to expose queue stats
  // For now, assume healthy = low load, degraded = high load
  const health = await AIProviderDaemon.checkHealth();
  const ollamaHealth = health.get('ollama');

  if (!ollamaHealth) return 0;

  return ollamaHealth.status === 'healthy' ? 0.2 :
         ollamaHealth.status === 'degraded' ? 0.7 : 1.0;
}
```

#### 4. **LoRA Adapter Loading** (GenomeManager coordinates with inference)

**Problem**: Loading LoRA adapters takes time and memory. Need to preload before inference.

**Solution**: GenomeManager provides adapter loading API for PersonaUsers:

```typescript
// PersonaUser.ts (future integration)
async respondToMessage(message: ChatMessageEntity): Promise<void> {
  // NEW: Ensure LoRA adapter is loaded before inference
  if (this.genomeConfig.loraEnabled && this.genomeConfig.loraAdapterPath) {
    await GenomeManager.shared().loadAdapter(
      this.id,
      'conversational',
      this.genomeConfig.loraAdapterPath
    );
  }

  // Existing: Generate response with AI
  const response = await AIProviderDaemon.generateText({
    messages: ragContext.conversationHistory,
    model: this.genomeConfig.baseModel,
    preferredProvider: 'ollama'
    // TODO: How to tell Ollama to use LoRA adapter?
    // Ollama API doesn't support LoRA adapters yet in Phase 7.0 MVP
  });
}
```

**Phase 7.1+ Enhancement**: When Ollama adds LoRA support, GenomeManager will provide adapter path to inference request.

---

## Minimal Integration for Phase 7.0 MVP

**Goal**: GenomeManager and AIProviderDaemon coexist without breaking anything.

### Step 1: Add GenomeManager singleton initialization

**File**: `daemons/ai-provider-daemon/server/AIProviderDaemonServer.ts`

```typescript
import { GenomeManager } from '../../../system/genome/fine-tuning/server/GenomeManager';

// In AIProviderDaemonServer.initialize()
protected async initialize(): Promise<void> {
  await super.initialize(); // Base AIProviderDaemon initialization

  // Register adapters (existing code)
  await this.registerAdapter(new OllamaAdapter(), { priority: 100, enabled: true });
  // ... other adapters

  // NEW: Initialize GenomeManager
  const genomeManager = GenomeManager.shared({
    maxGPUMemoryUsagePercent: 80,
    maxConcurrentTraining: 1,
    maxLoadedAdapters: 5
  });

  await genomeManager.initializeGPUResources();

  // NEW: Register LoRA trainers with GenomeManager
  const { OllamaLoRAAdapter } = await import('../../../system/genome/fine-tuning/server/adapters/OllamaLoRAAdapter');
  genomeManager.registerAdapter('ollama', new OllamaLoRAAdapter());

  console.log('✅ AIProviderDaemonServer: GenomeManager initialized');
}
```

### Step 2: Expose GenomeManager via AIProviderDaemon static interface

**File**: `daemons/ai-provider-daemon/shared/AIProviderDaemon.ts`

```typescript
/**
 * Get GenomeManager instance - CLEAN INTERFACE
 *
 * @example
 * const genomeManager = AIProviderDaemon.getGenomeManager();
 * await genomeManager.submitTrainingJob(request, 'ollama');
 */
static getGenomeManager(): GenomeManager {
  // GenomeManager is singleton, so just return shared instance
  return GenomeManager.shared();
}
```

### Step 3: No changes to PersonaUser yet

**Rationale**: Phase 7.0 MVP is interface structure only. PersonaUsers don't use GenomeManager yet.

**Phase 7.1+**: PersonaUsers will call `GenomeManager.shared().submitTrainingJob()` to train LoRA adapters.

---

## Future Integration (Phase 7.1+)

### 1. **Base Model Memory Tracking**

**Implementation**: Expand GenomeManager to track base models loaded by Ollama:

```typescript
// GenomeManager.ts
interface BaseModelInfo {
  modelName: string;
  memoryUsageMB: number;
  usedByPersonas: Set<UUID>;
  lastUsedAt: number;
  loadedAt: number;
}

private baseModels: Map<string, BaseModelInfo> = new Map();

// Called by AIProviderDaemon before inference
async registerModelLoad(modelName: string, memoryUsageMB: number, personaId?: UUID): Promise<void> {
  const existing = this.baseModels.get(modelName);

  if (existing) {
    // Model already loaded, just update usage
    existing.lastUsedAt = Date.now();
    if (personaId) {
      existing.usedByPersonas.add(personaId);
    }
  } else {
    // Model not loaded yet, check memory
    if (this.getTotalAvailableMemoryMB() < memoryUsageMB) {
      await this.evictBaseModel(memoryUsageMB);
    }

    this.baseModels.set(modelName, {
      modelName,
      memoryUsageMB,
      usedByPersonas: personaId ? new Set([personaId]) : new Set(),
      lastUsedAt: Date.now(),
      loadedAt: Date.now()
    });

    console.log(`📥 GenomeManager: Registered base model ${modelName} (${memoryUsageMB}MB)`);
  }
}

// Called by AIProviderDaemon after inference
async updateModelUsage(modelName: string, personaId?: UUID): Promise<void> {
  const model = this.baseModels.get(modelName);
  if (model) {
    model.lastUsedAt = Date.now();
    if (personaId) {
      model.usedByPersonas.add(personaId);
    }
  }
}

// Called when PersonaUser shuts down
async unregisterPersonaFromModel(modelName: string, personaId: UUID): Promise<void> {
  const model = this.baseModels.get(modelName);
  if (model) {
    model.usedByPersonas.delete(personaId);
  }
}
```

### 2. **Ollama Request Queue Integration**

**Problem**: OllamaAdapter's request queue doesn't know about GPU memory limits.

**Solution**: Expose queue stats and coordinate with GenomeManager:

```typescript
// OllamaAdapter.ts
getQueueStats(): { queueSize: number; activeRequests: number; maxConcurrent: number } {
  return this.requestQueue.getStats();
}

// AIProviderDaemon.ts
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // Check Ollama queue before inference
  const adapter = this.selectAdapter(request.preferredProvider);
  if (adapter instanceof OllamaAdapter) {
    const queueStats = adapter.getQueueStats();

    // If queue is full, coordinate with GenomeManager to free resources
    if (queueStats.queueSize > 10) {
      await GenomeManager.shared().evictUnusedAdapters();
    }
  }

  // Existing inference code...
}
```

### 3. **Training Job Coordination**

**Problem**: Training jobs require exclusive GPU access to avoid interference with inference.

**Solution**: GenomeManager monitors inference load and schedules training during quiet periods:

```typescript
// GenomeManager.ts
private async startTraining(
  job: TrainingJob,
  request: LoRATrainingRequest,
  trainer: LoRATrainer
): Promise<void> {
  job.status = 'preparing';
  job.startedAt = Date.now();
  this.activeTrainingJobs.set(job.id, job);

  console.log(`🚀 GenomeManager: Starting training job ${job.id}`);

  // Wait for inference to quiet down
  await this.waitForInferenceQuietPeriod();

  // Pause new inference requests during training
  this.trainingMode = true;

  try {
    // Call trainer adapter (Phase 7.1+)
    const result = await trainer.trainLoRA(request);

    job.status = 'completed';
    job.completedAt = Date.now();
    job.result = result;
  } catch (error) {
    job.status = 'failed';
    job.completedAt = Date.now();
    job.result = {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  } finally {
    // Resume normal inference
    this.trainingMode = false;
    this.activeTrainingJobs.delete(job.id);
    this.processTrainingQueue();
  }
}

private async waitForInferenceQuietPeriod(): Promise<void> {
  // Wait until no active inference requests
  const maxWait = 30000; // 30 seconds max
  const startWait = Date.now();

  while (Date.now() - startWait < maxWait) {
    const inferenceLoad = await this.getInferenceLoad();
    if (inferenceLoad < 0.1) {
      console.log('✅ GenomeManager: Inference quiet, safe to start training');
      return;
    }
    await new Promise(resolve => setTimeout(resolve, 1000));
  }

  console.log('⚠️ GenomeManager: Timeout waiting for inference quiet period, starting training anyway');
}
```

---

## Summary

### What Already Exists ✅

1. **AIProviderDaemon**: Handles all inference across providers (Ollama, OpenAI, etc.)
2. **OllamaAdapter**: Local model inference with request queue (maxConcurrent=4)
3. **BaseAIProviderAdapter**: Health monitoring and automatic recovery
4. **ProcessPool**: Worker threads for parallel inference (already integrated)
5. **Clean static interface**: `AIProviderDaemon.generateText()` works everywhere

### What GenomeManager Adds 🆕

1. **GPU resource tracking**: Total memory, availability across all GPUs
2. **Base model memory tracking**: Know what's loaded (llama3.2:3b = 2GB, etc.)
3. **LoRA adapter paging**: Load/unload genomic layers dynamically
4. **Training job queue**: Prevent GPU oversubscription during training
5. **Holistic coordination**: One system knows everything about GPU usage

### Integration Plan 📋

**Phase 7.0 MVP** (Current):
- ✅ GenomeManager exists (interface structure only)
- ✅ OllamaLoRAAdapter exists (stub, returns `supportsFineTuning() = false`)
- ⏳ Initialize GenomeManager in AIProviderDaemonServer
- ⏳ Expose GenomeManager via static interface
- ⏳ Document integration points (this document)

**Phase 7.1+** (Future):
- Base model memory tracking (register/unregister on load/unload)
- LoRA adapter loading and paging (when Ollama adds support)
- Training job execution (coordinate with inference load)
- Queue stats integration (OllamaAdapter → GenomeManager)
- Inference pausing during training (training mode flag)

### Key Principles 🎯

1. **AIProviderDaemon handles inference, GenomeManager handles orchestration**
2. **Collaborate, don't compete**: Both systems work together
3. **Phase 7.0 MVP = interface structure**: No behavior changes yet
4. **Phase 7.1+ = full integration**: When Ollama supports LoRA adapters
5. **No breaking changes**: Existing inference continues working

---

## Next Steps

1. **Implement minimal integration** (Phase 7.0 MVP):
   - Initialize GenomeManager in AIProviderDaemonServer
   - Expose `AIProviderDaemon.getGenomeManager()` static method
   - Document coexistence in both READMEs

2. **Test coexistence** (Phase 7.0 MVP):
   - Verify AIProviderDaemon.generateText() still works
   - Verify GenomeManager.shared() returns singleton
   - Verify no interference between systems

3. **Plan Phase 7.1+ integration**:
   - Add base model tracking hooks in AIProviderDaemon.generateText()
   - Add queue stats API to OllamaAdapter
   - Implement training coordination with inference load monitoring

4. **Document for future developers**:
   - Update AIProviderDaemon README
   - Update GenomeManager documentation
   - Add integration examples

---

**Philosophy**: "Bring it all together and research what's set up" ✅

This document answers the user's question: "was there an aidaemon or something doing this?"

**Answer**: Yes! AIProviderDaemon handles inference. GenomeManager orchestrates resources. They should collaborate.
