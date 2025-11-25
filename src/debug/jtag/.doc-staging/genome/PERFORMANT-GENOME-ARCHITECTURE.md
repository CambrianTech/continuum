# Performant Genome Architecture - Adapter-Driven Design

**Date**: 2025-11-22
**Philosophy**: Everything is adapter-driven. Performance is critical. Sophisticated is required.

---

## Core Principle: Adapter-Driven Everything

**Not** "here's a Python script that does PEFT."

**Instead**: "Here's a pluggable architecture where EVERY component is an adapter."

### What's An Adapter?

An adapter is a pluggable implementation of an interface. You can swap adapters without changing the architecture.

**Examples**:
- Training providers (OpenAI, Fireworks, PEFT, MLX) → `ILoRATrainer` adapter
- Backend providers (Ollama, Fireworks API, OpenAI) → `IAdapterBackend` adapter
- Storage strategies (local disk, S3, hybrid) → `IGenomeStorage` adapter
- Composition methods (PEFT runtime, offline merge, API-only) → `ICompositor` adapter
- Eviction policies (LRU, priority-weighted, working-set) → `IEvictionPolicy` adapter

**The power**: Swap any component without touching core logic. Add new providers by dropping in a new adapter.

---

## The Three-Layer Architecture

```
┌────────────────────────────────────────────────────────────────┐
│ Layer 1: GenomeDaemon (Centralized Controller)                │
│ • Global coordination across ALL personas                      │
│ • LRU eviction, thrashing detection, hysteresis               │
│ • ResourceManager integration for quotas                       │
│ • Performance optimizations (cache hits, memory pressure)     │
└────────────────────────────────────────────────────────────────┘
                           ↓
┌────────────────────────────────────────────────────────────────┐
│ Layer 2: Adapter Interfaces (Pluggable Contracts)             │
│ • IAdapterBackend - How to load/inference with adapters       │
│ • IGenomeStorage - Where adapters are stored                  │
│ • ICompositor - How to compose multiple layers                │
│ • IEvictionPolicy - When/what to evict                        │
│ • ILoRATrainer - How to train new adapters                    │
└────────────────────────────────────────────────────────────────┘
                           ↓
┌────────────────────────────────────────────────────────────────┐
│ Layer 3: Concrete Adapters (Implementations)                  │
│ • OllamaBackend, FireworksBackend, OpenAIBackend             │
│ • LocalStorage, S3Storage, HybridStorage                      │
│ • PEFTCompositor, OfflineMergeCompositor, NoOpCompositor     │
│ • LRUPolicy, PriorityWeightedPolicy, WorkingSetPolicy        │
│ • PEFTTrainer, FireworksTrainer, OpenAITrainer               │
└────────────────────────────────────────────────────────────────┘
```

---

## Layer 1: GenomeDaemon (The Brain)

### Responsibilities

**NOT**: "Load this specific PEFT adapter using this Python script."

**YES**: "Coordinate adapter lifecycle across ALL personas with performance guarantees."

### Core Functions

```typescript
class GenomeDaemon {
  // Global state
  private personaGenomes: Map<UUID, PersonaGenomeState>;
  private adapterRegistry: Map<string, AdapterMetadata>;
  private resourceManager: ResourceManager;

  // Pluggable components (ADAPTERS!)
  private backend: IAdapterBackend;
  private storage: IGenomeStorage;
  private compositor: ICompositor;
  private evictionPolicy: IEvictionPolicy;

  /**
   * Activate genome for persona
   *
   * Orchestrates: storage lookup → quota check → eviction →
   * backend loading → composition → cache tracking
   */
  async activateGenome(
    personaId: UUID,
    layers: LayerActivation[]
  ): Promise<ActivationResult> {
    // 1. Check cache (performance optimization)
    if (this.isCached(personaId, layers)) {
      return { cacheHit: true, latencyMs: 0 };
    }

    // 2. Check memory quota
    const quota = this.resourceManager.getQuota(personaId);
    const required = this.calculateMemoryRequired(layers);

    if (required > quota.available) {
      // 3. Evict LRU adapters (using pluggable policy)
      await this.evictionPolicy.evictUntilAvailable(required);
    }

    // 4. Load adapters from storage (pluggable)
    const adapters = await this.storage.loadAdapters(layers);

    // 5. Compose adapters (pluggable - PEFT, offline merge, or no-op)
    const composed = await this.compositor.compose(adapters, layers);

    // 6. Activate in backend (pluggable - Ollama, Fireworks, etc.)
    await this.backend.activateComposition(personaId, composed);

    // 7. Update cache and tracking
    this.trackActivation(personaId, layers, required);

    return {
      cacheHit: false,
      latencyMs: composed.latencyMs,
      evicted: composed.evictedAdapters
    };
  }

  /**
   * Thrashing detection (sophisticated)
   *
   * Monitors: eviction rate, cache hit rate, working set size
   * Actions: hysteresis, throttling, emergency mode
   */
  private detectAndMitigateThrashing(): void {
    const metrics = this.calculateThrashingMetrics();

    if (metrics.isThrashing) {
      // Sophisticated mitigation strategies
      this.enableHysteresis();
      this.throttleLowPriorityPersonas();
      this.expandWorkingSet();
      this.alertSystem('Thrashing detected, mitigations active');
    }
  }
}
```

### Performance Guarantees

**Hysteresis**: Don't evict adapters loaded in last 30 seconds (prevents ping-pong)

**Cache Hits**: Track hit rate per persona, optimize working set

**Thrashing Detection**: Automatic detection + mitigation strategies

**Memory Pressure**: Graceful degradation under load

**Priority Weighting**: High-priority personas get preferential treatment

---

## Layer 2: Adapter Interfaces (The Contracts)

### IAdapterBackend - How To Interact With AI Backends

**Purpose**: Abstract "how do we actually run inference with adapters?"

```typescript
interface IAdapterBackend {
  readonly providerId: string;
  readonly supportsLoRA: boolean;
  readonly supportsRuntimeComposition: boolean;
  readonly maxActiveLayers: number;

  /**
   * Activate composed genome for persona
   *
   * For Ollama: Load .safetensors via API
   * For Fireworks: Specify adapter ID in request
   * For OpenAI: Silently ignore (use system prompt instead)
   */
  activateComposition(
    personaId: UUID,
    composition: ComposedGenome
  ): Promise<void>;

  /**
   * Generate inference with active genome
   */
  generate(
    personaId: UUID,
    prompt: string,
    options: GenerationOptions
  ): Promise<string>;

  /**
   * Deactivate genome (free resources)
   */
  deactivateComposition(personaId: UUID): Promise<void>;

  /**
   * Get backend-specific metadata
   */
  getCapabilities(): BackendCapabilities;
}
```

**Implementations**:
- `OllamaBackend` - Native LoRA, local inference
- `FireworksBackend` - Native LoRA, cloud inference
- `OpenAIBackend` - No LoRA, system prompt fallback
- `AnthropicBackend` - No LoRA, system prompt fallback

### IGenomeStorage - Where Adapters Live

**Purpose**: Abstract "where are adapter files stored?"

```typescript
interface IGenomeStorage {
  readonly storageType: 'local' | 's3' | 'hybrid';
  readonly supportsVersioning: boolean;
  readonly supportsLazyLoading: boolean;

  /**
   * List available adapters (with metadata)
   */
  listAdapters(): Promise<AdapterMetadata[]>;

  /**
   * Load adapter weights into memory
   *
   * For local: Read .safetensors from disk
   * For S3: Download + cache
   * For hybrid: Check local cache first, fallback to S3
   */
  loadAdapter(adapterId: string): Promise<AdapterWeights>;

  /**
   * Store newly trained adapter
   */
  storeAdapter(
    adapterId: string,
    weights: AdapterWeights,
    metadata: AdapterMetadata
  ): Promise<void>;

  /**
   * Delete adapter (from cache or permanently)
   */
  deleteAdapter(adapterId: string, permanent: boolean): Promise<void>;

  /**
   * Get storage metrics (used space, cache hit rate)
   */
  getMetrics(): StorageMetrics;
}
```

**Implementations**:
- `LocalGenomeStorage` - Disk-based (`.continuum/cache/layers/`)
- `S3GenomeStorage` - Cloud storage (S3/R2/etc.)
- `HybridGenomeStorage` - Local cache + cloud fallback

### ICompositor - How To Combine Multiple Layers

**Purpose**: Abstract "how do we compose N adapters into one phenotype?"

```typescript
interface ICompositor {
  readonly compositionMethod: 'peft' | 'offline-merge' | 'none';
  readonly supportsRuntimeWeighting: boolean;
  readonly maxLayers: number;

  /**
   * Compose multiple adapters with weights
   *
   * For PEFT: Runtime composition via set_adapters()
   * For offline merge: Pre-merged composite (TIES/DARE)
   * For none: Single adapter only
   */
  compose(
    adapters: AdapterWeights[],
    layers: LayerActivation[]
  ): Promise<ComposedGenome>;

  /**
   * Adjust weights dynamically (if supported)
   */
  adjustWeights(
    compositionId: UUID,
    weightMap: Record<string, number>
  ): Promise<void>;

  /**
   * Get composition metadata
   */
  getCompositionInfo(compositionId: UUID): CompositionMetadata;
}
```

**Implementations**:
- `PEFTCompositor` - Python PEFT integration, runtime composition
- `OfflineMergeCompositor` - Pre-merge adapters (TIES/DARE/linear)
- `SingleLayerCompositor` - One adapter at a time (simplest)
- `NoOpCompositor` - No composition (for non-LoRA backends)

### IEvictionPolicy - When/What To Evict

**Purpose**: Abstract "which adapter should we evict when memory is full?"

```typescript
interface IEvictionPolicy {
  readonly policyName: string;

  /**
   * Calculate eviction score for adapter
   * Higher score = more likely to evict
   */
  calculateEvictionScore(
    adapter: AdapterMetadata,
    persona: PersonaGenomeState,
    globalContext: GlobalGenomeContext
  ): number;

  /**
   * Select victim for eviction
   */
  selectVictim(
    candidates: Array<{adapter: AdapterMetadata; persona: PersonaGenomeState}>
  ): { personaId: UUID; adapterId: string };

  /**
   * Evict adapters until required memory available
   */
  evictUntilAvailable(requiredMB: number): Promise<EvictionResult>;
}
```

**Implementations**:
- `LRUPolicy` - Least recently used
- `PriorityWeightedLRUPolicy` - LRU with priority weighting
- `WorkingSetPolicy` - Keep frequently-used adapters (anti-thrashing)
- `HysteresisPolicy` - Never evict adapters loaded <30s ago

### ILoRATrainer - How To Train New Adapters

**Purpose**: Abstract "how do we create new LoRA adapters?"

```typescript
interface ILoRATrainer {
  readonly providerId: string;

  /**
   * Two primitives pattern (from adapter-architecture.md)
   */
  protected abstract _startTraining(
    request: LoRATrainingRequest
  ): Promise<TrainingHandle>;

  protected abstract _queryStatus(
    session: TrainingSessionEntity
  ): Promise<TrainingStatus>;

  /**
   * Public API (orchestration handled by base class)
   */
  trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult>;
  checkStatus(sessionId: UUID): Promise<TrainingStatus>;
}
```

**Implementations**:
- `PEFTTrainer` - Local PyTorch + PEFT training
- `MLXTrainer` - Apple Silicon MLX training
- `FireworksTrainer` - Fireworks AI API
- `OpenAITrainer` - OpenAI fine-tuning API
- `TogetherTrainer` - Together AI API

---

## Layer 3: Concrete Adapters (The Implementations)

### Example: OllamaBackend

```typescript
class OllamaBackend implements IAdapterBackend {
  readonly providerId = 'ollama';
  readonly supportsLoRA = true;
  readonly supportsRuntimeComposition = true; // If Ollama + PEFT
  readonly maxActiveLayers = 16; // PEFT limit

  async activateComposition(
    personaId: UUID,
    composition: ComposedGenome
  ): Promise<void> {
    // Ollama API: Specify adapter path
    await fetch('http://localhost:11434/api/chat', {
      method: 'POST',
      body: JSON.stringify({
        model: composition.baseModel,
        adapter: composition.adapterPath, // .safetensors file
        messages: [] // Warm up
      })
    });
  }

  async generate(
    personaId: UUID,
    prompt: string,
    options: GenerationOptions
  ): Promise<string> {
    const composition = this.getActiveComposition(personaId);

    const response = await fetch('http://localhost:11434/api/chat', {
      method: 'POST',
      body: JSON.stringify({
        model: composition.baseModel,
        adapter: composition.adapterPath,
        messages: [{ role: 'user', content: prompt }],
        stream: false
      })
    });

    const { message } = await response.json();
    return message.content;
  }

  async deactivateComposition(personaId: UUID): Promise<void> {
    // Ollama: Just stop referencing adapter (GC will clean up)
    this.activeCompositions.delete(personaId);
  }

  getCapabilities(): BackendCapabilities {
    return {
      supportsLoRA: true,
      supportsRuntimeComposition: true,
      maxActiveLayers: 16,
      memoryLimit: '8GB', // Depends on hardware
      costPerToken: 0, // Local, free
      latencyMs: 50 // Estimate
    };
  }
}
```

### Example: PEFTCompositor

```typescript
class PEFTCompositor implements ICompositor {
  readonly compositionMethod = 'peft';
  readonly supportsRuntimeWeighting = true;
  readonly maxLayers = 16;

  private pythonProcess: ChildProcess;
  private activeCompositions: Map<UUID, ComposedGenome>;

  async compose(
    adapters: AdapterWeights[],
    layers: LayerActivation[]
  ): Promise<ComposedGenome> {
    // Start Python subprocess if not running
    if (!this.pythonProcess) {
      this.pythonProcess = this.spawnPEFTServer();
    }

    // Send composition request to Python subprocess via JSON-RPC
    const request = {
      method: 'compose',
      params: {
        baseModel: layers[0].baseModel,
        adapters: layers.map(l => ({
          name: l.name,
          path: adapters.find(a => a.id === l.name)?.path,
          weight: l.weight
        }))
      }
    };

    const response = await this.sendToPython(request);

    return {
      id: uuidv4(),
      baseModel: request.params.baseModel,
      layers: layers,
      adapterPath: response.composedPath, // Temporary composed adapter
      latencyMs: response.latencyMs
    };
  }

  async adjustWeights(
    compositionId: UUID,
    weightMap: Record<string, number>
  ): Promise<void> {
    const composition = this.activeCompositions.get(compositionId);

    // Update weights in Python subprocess
    const request = {
      method: 'adjust_weights',
      params: {
        compositionId,
        weights: weightMap
      }
    };

    await this.sendToPython(request);

    // Update local tracking
    for (const layer of composition.layers) {
      if (weightMap[layer.name] !== undefined) {
        layer.weight = weightMap[layer.name];
      }
    }
  }

  private spawnPEFTServer(): ChildProcess {
    return spawn('python3', [
      'system/genome/python/peft_composition_server.py',
      '--port', '9999'
    ], {
      stdio: ['pipe', 'pipe', 'pipe']
    });
  }

  private async sendToPython(request: any): Promise<any> {
    // JSON-RPC over stdin/stdout
    this.pythonProcess.stdin.write(JSON.stringify(request) + '\n');

    return new Promise((resolve, reject) => {
      this.pythonProcess.stdout.once('data', (data) => {
        const response = JSON.parse(data.toString());
        if (response.error) {
          reject(new Error(response.error));
        } else {
          resolve(response.result);
        }
      });
    });
  }
}
```

### Example: PriorityWeightedLRUPolicy

```typescript
class PriorityWeightedLRUPolicy implements IEvictionPolicy {
  readonly policyName = 'priority-weighted-lru';

  calculateEvictionScore(
    adapter: AdapterMetadata,
    persona: PersonaGenomeState,
    globalContext: GlobalGenomeContext
  ): number {
    const ageSeconds = (Date.now() - adapter.lastUsedTime) / 1000;
    const priority = persona.priority ?? 0.5;

    // High priority = low score = less likely to evict
    // Old age = high score = more likely to evict

    // Never evict high-priority personas (>0.9)
    if (priority > 0.9) {
      return -Infinity;
    }

    // Never evict recently loaded (hysteresis)
    const timeSinceLoad = Date.now() - adapter.loadedAt;
    if (timeSinceLoad < 30000) { // 30 seconds
      return -Infinity;
    }

    // Score = age / priority weight
    return ageSeconds / (priority * 10);
  }

  selectVictim(
    candidates: Array<{adapter: AdapterMetadata; persona: PersonaGenomeState}>
  ): { personaId: UUID; adapterId: string } {
    let maxScore = -Infinity;
    let victim = null;

    for (const candidate of candidates) {
      const score = this.calculateEvictionScore(
        candidate.adapter,
        candidate.persona,
        this.globalContext
      );

      if (score > maxScore) {
        maxScore = score;
        victim = candidate;
      }
    }

    if (!victim) {
      throw new Error('No evictable adapters found');
    }

    return {
      personaId: victim.persona.personaId,
      adapterId: victim.adapter.id
    };
  }

  async evictUntilAvailable(requiredMB: number): Promise<EvictionResult> {
    const evicted: string[] = [];
    let freedMB = 0;

    while (freedMB < requiredMB) {
      const candidates = this.getCandidates();
      const victim = this.selectVictim(candidates);

      const adapter = this.adapterRegistry.get(victim.adapterId);
      await this.backend.unloadAdapter(victim.personaId, victim.adapterId);

      evicted.push(victim.adapterId);
      freedMB += adapter.sizeMB;
    }

    return { evicted, freedMB };
  }
}
```

---

## PersonaGenome Integration (Lightweight)

**Key insight**: PersonaGenome is a THIN WRAPPER around GenomeDaemon

```typescript
class PersonaGenome {
  private personaId: UUID;
  private daemon: GenomeDaemon;

  constructor(personaId: UUID) {
    this.personaId = personaId;
    this.daemon = GenomeDaemon.getInstance();
  }

  /**
   * Activate genome for current persona
   * Delegates to centralized daemon
   */
  async activatePhenotype(layers: LayerActivation[]): Promise<void> {
    await this.daemon.activateGenome(this.personaId, layers);
  }

  /**
   * Adjust layer weights dynamically
   * Delegates to daemon's compositor
   */
  async adjustWeights(weightMap: Record<string, number>): Promise<void> {
    await this.daemon.adjustWeights(this.personaId, weightMap);
  }

  /**
   * Get current genome state
   */
  getActivePhenotype(): PhenotypeProfile {
    return this.daemon.getPersonaGenome(this.personaId);
  }

  /**
   * Generate with active genome
   * Delegates to daemon's backend
   */
  async generate(prompt: string, options?: GenerationOptions): Promise<string> {
    return this.daemon.generate(this.personaId, prompt, options);
  }
}
```

**PersonaUser stays lightweight**:

```typescript
class PersonaUser extends AIUser {
  public genome: PersonaGenome;

  constructor(...) {
    this.genome = new PersonaGenome(this.id);
  }

  async processTask(task: TaskEntity): Promise<void> {
    // Activate appropriate genome layers
    const layers = this.selectLayersForTask(task);
    await this.genome.activatePhenotype(layers);

    // Generate response with active genome
    const response = await this.genome.generate(task.prompt);

    // Process response...
  }
}
```

---

## Performance Optimizations

### 1. Cache Hit Optimization

```typescript
class GenomeDaemon {
  private cacheHitRate: Map<UUID, number> = new Map();

  private trackCacheHit(personaId: UUID, hit: boolean): void {
    const current = this.cacheHitRate.get(personaId) ?? 0.5;

    // Exponential moving average
    const alpha = 0.1;
    const newRate = alpha * (hit ? 1 : 0) + (1 - alpha) * current;

    this.cacheHitRate.set(personaId, newRate);

    // If cache hit rate drops below threshold, adjust working set
    if (newRate < 0.3) {
      this.expandWorkingSet(personaId);
    }
  }
}
```

### 2. Thrashing Detection

```typescript
interface ThrashingMetrics {
  evictionsPerMinute: number;
  loadRequestsPerMinute: number;
  cacheHitRate: number;
  workingSetSize: number; // Unique adapters used
}

function detectThrashing(metrics: ThrashingMetrics): boolean {
  return (
    metrics.evictionsPerMinute > 10 &&    // High eviction rate
    metrics.cacheHitRate < 0.3 &&          // Low cache hit rate
    metrics.workingSetSize < 5             // Small working set (same adapters)
  );
}

function mitigateThrashing(): void {
  // 1. Enable hysteresis (longer protection window)
  HYSTERESIS_WINDOW_MS = 60000; // 30s → 60s

  // 2. Throttle low-priority personas
  MIN_PRIORITY_FOR_LOADING = 0.5; // Block priority < 0.5

  // 3. Expand working set (keep more adapters cached)
  WORKING_SET_SIZE_TARGET = 10; // 5 → 10

  // 4. Alert system
  Events.emit('genome:thrashing-detected', {
    severity: 'high',
    mitigations: ['hysteresis', 'throttling', 'expanded-working-set']
  });
}
```

### 3. Predictive Loading

```typescript
class GenomeDaemon {
  private predictionModel: Map<UUID, string[]> = new Map();

  /**
   * Predict next likely adapter based on recent patterns
   */
  private predictNextAdapter(personaId: UUID): string | null {
    const recentAdapters = this.getRecentAdapters(personaId, 10);

    // Simple pattern: if last 3 were [A, B, C], predict A next
    if (recentAdapters.length >= 3) {
      const pattern = recentAdapters.slice(-3);
      const candidate = pattern[0];

      // Preload if not cached
      if (!this.isCached(personaId, candidate)) {
        this.preloadAdapter(personaId, candidate);
      }

      return candidate;
    }

    return null;
  }

  private async preloadAdapter(personaId: UUID, adapterId: string): Promise<void> {
    // Load in background, don't block
    this.storage.loadAdapter(adapterId).then(weights => {
      this.cacheAdapter(personaId, adapterId, weights);
      console.log(`🔮 Predictive load: ${adapterId} for ${personaId}`);
    });
  }
}
```

---

## Adapter Registry & Marketplace

### Layer Registry (Like npm or Docker Hub)

```
registry.continuum.ai/
├── layers/
│   ├── wine-expertise-v1/
│   │   ├── adapter.safetensors (512MB)
│   │   ├── metadata.json
│   │   ├── checksum.sha256
│   │   └── README.md
│   ├── typescript-expert-v3/
│   └── drill-sergeant-v2/
└── personas/
    ├── vine-diesel/
    │   └── manifest.json (references layers)
    └── captain-calorie/
        └── manifest.json
```

### CLI Commands

```bash
# Pull layer from registry
./jtag genome/layer-pull wine-expertise-v1
# → Downloads to .continuum/cache/layers/wine-expertise-v1/

# List cached layers
./jtag genome/layer-list --cached
# → wine-expertise-v1 (512MB, v1.0.0)
# → typescript-expert-v3 (768MB, v3.2.1)

# Publish custom layer
./jtag genome/layer-publish my-custom-layer \
  --registry="registry.continuum.ai" \
  --visibility="public"

# Import persona (auto-pulls missing layers)
./jtag genome/persona-import vine-diesel.zip
# → Reads manifest.json
# → Auto-runs: genome/layer-pull wine-expertise-v1
# → Auto-runs: genome/layer-pull action-hero-style-v2
# → Verifies checksums
# → Ready to use
```

---

## Comparison: Naive vs Sophisticated

### Naive (Current peft_composition.py)

```python
# Standalone Python script
composer = PEFTComposer("llama3.1:8b")
composer.load_adapter("./wine", "wine")
composer.load_adapter("./personality", "personality")
composer.set_composition(["wine", "personality"], [0.7, 0.3])
response = composer.generate("What is wine?")
```

**Problems**:
- ❌ No integration with PersonaUser
- ❌ No global coordination (thrashing possible)
- ❌ No memory management across personas
- ❌ No caching or performance optimizations
- ❌ No adapter backend abstraction (PEFT only)
- ❌ No storage abstraction (local disk only)
- ❌ No eviction policy
- ❌ No metrics or observability

### Sophisticated (GenomeDaemon Architecture)

```typescript
// PersonaUser (lightweight)
await this.genome.activatePhenotype([
  { name: 'wine-expertise', weight: 0.7 },
  { name: 'vin-diesel-style', weight: 0.3 }
]);

const response = await this.genome.generate("What is wine?");

// Behind the scenes (GenomeDaemon orchestrates):
// 1. Check cache → MISS
// 2. Check quota → 512MB available
// 3. Storage adapter loads layers
// 4. Compositor composes with PEFT
// 5. Backend adapter activates in Ollama
// 6. Track activation for LRU
// 7. Return composed genome reference
```

**Benefits**:
- ✅ Centralized coordination (no thrashing)
- ✅ Global memory management with quotas
- ✅ Sophisticated eviction (priority-weighted LRU)
- ✅ Pluggable backends (Ollama, Fireworks, OpenAI)
- ✅ Pluggable storage (local, S3, hybrid)
- ✅ Performance optimizations (cache hits, hysteresis, predictive loading)
- ✅ Rich metrics and observability
- ✅ Adapter-driven (easy to extend)

---

## Implementation Priority

### Phase 1: GenomeDaemon Foundation

**Tasks**:
1. Implement GenomeDaemon singleton
2. Define all adapter interfaces
3. Implement LocalGenomeStorage adapter
4. Implement SingleLayerCompositor adapter (simplest)
5. Implement OllamaBackend adapter
6. Implement LRUPolicy adapter

**Deliverable**: GenomeDaemon manages single-layer genomes for Ollama

**Testing**:
```typescript
const daemon = GenomeDaemon.getInstance();

await daemon.activateGenome(persona1, [
  { name: 'wine-expertise', weight: 1.0 }
]);

const response = await daemon.generate(persona1, "What is wine?");
```

### Phase 2: Multi-Layer Composition

**Tasks**:
1. Implement PEFTCompositor adapter
2. Convert peft_composition.py to JSON-RPC server
3. TypeScript ↔ Python IPC bridge
4. Test multi-layer activation

**Deliverable**: PEFTCompositor enables N-layer phenotypes

**Testing**:
```typescript
await daemon.activateGenome(persona1, [
  { name: 'wine-expertise', weight: 0.7 },
  { name: 'vin-diesel-style', weight: 0.3 }
]);

const response = await daemon.generate(persona1, "Describe Cabernet");
// Response has BOTH wine knowledge AND Vin Diesel personality
```

### Phase 3: Performance Optimizations

**Tasks**:
1. Implement thrashing detection
2. Implement hysteresis
3. Implement cache hit tracking
4. Implement predictive loading
5. Add comprehensive metrics

**Deliverable**: Production-grade performance

### Phase 4: Multi-Backend Support

**Tasks**:
1. Implement FireworksBackend adapter
2. Implement OfflineMergeCompositor adapter
3. Implement HybridGenomeStorage adapter
4. Test cloud deployment

**Deliverable**: Works with both local (Ollama) and cloud (Fireworks)

### Phase 5: Layer Marketplace

**Tasks**:
1. Implement layer registry
2. Implement layer pull/push commands
3. Implement persona import/export
4. Version management

**Deliverable**: Shareable phenotypes like Docker images

---

## Key Design Decisions

### 1. Centralized vs Distributed

**Decision**: Centralized (GenomeDaemon)

**Rationale**: Global coordination prevents thrashing, enables sophisticated eviction policies, simplifies quota management

### 2. Adapter Pattern Everywhere

**Decision**: Every component is pluggable

**Rationale**: Easy to add new backends/storage/composition without touching core. Testable in isolation.

### 3. Performance First

**Decision**: Cache hits, hysteresis, thrashing detection are core features

**Rationale**: Genome paging must be fast or it's unusable. Sophisticated optimizations required.

### 4. Layer Marketplace

**Decision**: Layers are shareable, personas reference layers

**Rationale**: Modular training (N+M instead of N×M), community-driven evolution, efficient storage

### 5. Graceful Degradation

**Decision**: Non-LoRA backends supported via NoOpCompositor

**Rationale**: OpenAI/Claude don't support LoRA but can still work via system prompts

---

## Success Criteria

**Performance**:
- ✅ Cache hit rate > 70% under normal load
- ✅ Activation latency < 100ms for cache hits
- ✅ No thrashing under 10 concurrent personas
- ✅ Memory pressure handled gracefully

**Functionality**:
- ✅ Multi-layer composition works (N layers)
- ✅ Dynamic weight adjustment works
- ✅ Multiple backends supported (Ollama, Fireworks, OpenAI)
- ✅ Multiple storage backends (local, S3, hybrid)

**Developer Experience**:
- ✅ New backends added by implementing IAdapterBackend
- ✅ New storage added by implementing IGenomeStorage
- ✅ New eviction policies added by implementing IEvictionPolicy
- ✅ Rich metrics and observability

---

## Related Documents

**Architecture Foundation**:
- `docs/GENOME-DAEMON-ARCHITECTURE.md` - Original GenomeDaemon design
- `.doc-staging/genome/adapter-architecture.md` - Two primitives pattern
- `.doc-staging/genome/adapter-extensibility.md` - Adapter pattern examples

**Implementation Details**:
- `system/genome/python/peft_composition.py` - Python PEFT integration (to be converted to JSON-RPC server)
- `system/genome/fine-tuning/server/adapters/PEFTLoRAAdapter.ts` - Training adapter
- `system/user/server/modules/PersonaGenome.ts` - Current single-layer implementation (to be refactored)

**Existing Vision Documents** (pre-GenomeDaemon):
- `.doc-staging/genome/MULTI-LAYER-GENOME-ARCHITECTURE.md` - Multi-layer vision (superseded by this doc)
- `.doc-staging/genome/dynamic-composition-roadmap.md` - Composition strategy (superseded)
- `.doc-staging/genome/PEFT-IMPLEMENTATION-STATUS.md` - Status report (superseded)

---

## Summary

**The Architecture**: Three-layer adapter-driven design with GenomeDaemon as centralized controller

**The Philosophy**: Everything is pluggable. Performance is critical. Sophisticated is required.

**The Result**: Production-grade genome system that works with ANY backend, ANY storage, ANY composition method, with performance guarantees and rich observability.

**Next Action**: Implement GenomeDaemon foundation (Phase 1) - centralized coordinator with basic adapters

