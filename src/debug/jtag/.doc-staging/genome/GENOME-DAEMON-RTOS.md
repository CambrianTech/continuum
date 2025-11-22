# GenomeDaemon - RTOS Subprocess Architecture

**Date**: 2025-11-22
**Philosophy**: Non-blocking, signal-based, performance-first

---

## Critical Constraint: NO MAIN THREAD BLOCKING

**Like all RTOS subprocesses**: GenomeDaemon runs in SEPARATE THREAD, does NOT block PersonaUser.

```typescript
// ❌ WRONG: Blocking PersonaUser
await genome.activatePhenotype(layers);  // PersonaUser waits
const response = await genome.generate(prompt);  // Blocked

// ✅ RIGHT: Non-blocking command
genome.requestActivation(layers);  // Returns immediately
// ... PersonaUser continues processing
// ... GenomeDaemon activates in background
```

---

## Architecture: GenomeDaemon as Subprocess

### 1. GenomeDaemon Extends PersonaContinuousSubprocess

```typescript
/**
 * GenomeDaemon - Background genome management subprocess
 *
 * Like MemoryConsolidationSubprocess, but for LoRA adapters
 */
export class GenomeDaemon extends PersonaContinuousSubprocess {
  // Global state (shared across all personas via singleton pattern)
  private static instance: GenomeDaemon;

  private personaGenomes: Map<UUID, PersonaGenomeState> = new Map();
  private adapterRegistry: Map<string, AdapterMetadata> = new Map();

  // Pluggable adapters
  private backend: IAdapterBackend;
  private storage: IGenomeStorage;
  private compositor: ICompositor;
  private evictionPolicy: IEvictionPolicy;

  // Pending requests (lightweight queue)
  private pendingRequests: ActivationRequest[] = [];

  constructor() {
    super(null, {  // No persona - global daemon
      priority: 'low',  // Background work
      name: 'GenomeDaemon'
    });
  }

  static getInstance(): GenomeDaemon {
    if (!GenomeDaemon.instance) {
      GenomeDaemon.instance = new GenomeDaemon();
      GenomeDaemon.instance.start();  // Start immediately
    }
    return GenomeDaemon.instance;
  }

  /**
   * Lean tick() - Check signals, trigger heavy work only when needed
   *
   * Like cbar's motion detection → semantic segmentation
   */
  protected async tick(): Promise<void> {
    // 1. Check signals (FAST - just counters/flags)
    const signals = this.checkSignals();

    // 2. Process pending requests (if any)
    if (signals.hasPendingRequests) {
      await this.processPendingRequests();
    }

    // 3. Detect thrashing (lightweight check)
    if (signals.memoryPressure > 0.8 && signals.cacheHitRate < 0.3) {
      await this.mitigateThrashing();
    }

    // 4. Predictive loading (if idle)
    if (signals.isIdle && signals.cacheHitRate < 0.7) {
      await this.predictiveLoad();
    }

    // 5. Cleanup stale adapters (occasional)
    if (signals.shouldCleanup) {
      await this.cleanupStale();
    }
  }

  /**
   * Check lightweight signals (NO heavy operations)
   *
   * Like cbar checking motion detection flag
   */
  private checkSignals(): GenomeSignals {
    return {
      hasPendingRequests: this.pendingRequests.length > 0,
      memoryPressure: this.calculateMemoryPressure(),
      cacheHitRate: this.calculateCacheHitRate(),
      isIdle: this.pendingRequests.length === 0,
      shouldCleanup: Date.now() - this.lastCleanup > 60000 // Every minute
    };
  }

  /**
   * Request activation (NON-BLOCKING)
   *
   * PersonaUser calls this, continues immediately
   */
  requestActivation(
    personaId: UUID,
    layers: LayerActivation[],
    callback?: (result: ActivationResult) => void
  ): void {
    // Just enqueue, return immediately
    this.pendingRequests.push({
      personaId,
      layers,
      callback,
      timestamp: Date.now()
    });

    // Optionally wake up daemon for urgent requests
    if (layers.some(l => l.priority === 'urgent')) {
      this.wakeup();
    }
  }

  /**
   * Process pending requests (HEAVY - triggered by signal)
   */
  private async processPendingRequests(): Promise<void> {
    while (this.pendingRequests.length > 0) {
      const request = this.pendingRequests.shift()!;

      try {
        // Heavy operations here (triggered, not continuous)
        const result = await this.activateGenomeInternal(
          request.personaId,
          request.layers
        );

        // Callback to notify PersonaUser (if provided)
        if (request.callback) {
          request.callback(result);
        }

      } catch (error) {
        console.error(`GenomeDaemon: Activation failed for ${request.personaId}`, error);
      }
    }
  }

  /**
   * Internal activation (HEAVY - only called when triggered)
   */
  private async activateGenomeInternal(
    personaId: UUID,
    layers: LayerActivation[]
  ): Promise<ActivationResult> {
    // 1. Check cache (FAST)
    if (this.isCached(personaId, layers)) {
      return { cacheHit: true, latencyMs: 0 };
    }

    // 2. Check quota and evict if needed (HEAVY)
    await this.ensureQuotaAvailable(personaId, layers);

    // 3. Load adapters from storage (HEAVY)
    const adapters = await this.storage.loadAdapters(layers);

    // 4. Compose adapters (HEAVY)
    const composed = await this.compositor.compose(adapters, layers);

    // 5. Activate in backend (HEAVY)
    await this.backend.activateComposition(personaId, composed);

    // 6. Update cache and tracking (FAST)
    this.trackActivation(personaId, layers);

    return {
      cacheHit: false,
      latencyMs: composed.latencyMs,
      evicted: composed.evictedAdapters
    };
  }
}
```

---

## PersonaGenome - Thin Wrapper (Non-Blocking)

```typescript
/**
 * PersonaGenome - Lightweight interface to GenomeDaemon
 *
 * Does NOT block PersonaUser
 */
export class PersonaGenome {
  private personaId: UUID;
  private daemon: GenomeDaemon;
  private currentLayers: LayerActivation[] | null = null;

  constructor(personaId: UUID) {
    this.personaId = personaId;
    this.daemon = GenomeDaemon.getInstance();
  }

  /**
   * Activate phenotype (NON-BLOCKING)
   *
   * Returns immediately, activation happens in background
   */
  activatePhenotype(
    layers: LayerActivation[],
    callback?: (result: ActivationResult) => void
  ): void {
    // Update local tracking
    this.currentLayers = layers;

    // Send request to daemon, return immediately
    this.daemon.requestActivation(this.personaId, layers, callback);
  }

  /**
   * Activate and wait (BLOCKING - use sparingly!)
   *
   * For cases where PersonaUser MUST wait for activation
   */
  async activatePhenotypeSync(
    layers: LayerActivation[]
  ): Promise<ActivationResult> {
    return new Promise((resolve) => {
      this.activatePhenotype(layers, (result) => {
        resolve(result);
      });
    });
  }

  /**
   * Generate with active genome
   *
   * Delegates to daemon's backend (may block on first call if not activated)
   */
  async generate(prompt: string, options?: GenerationOptions): Promise<string> {
    // If not activated, activate synchronously
    if (!this.currentLayers) {
      throw new Error('No genome activated - call activatePhenotype() first');
    }

    return this.daemon.generate(this.personaId, prompt, options);
  }

  /**
   * Check if activation is complete (non-blocking check)
   */
  isActivated(): boolean {
    return this.daemon.isActivated(this.personaId, this.currentLayers);
  }

  /**
   * Wait for activation to complete
   */
  async waitForActivation(timeoutMs: number = 5000): Promise<boolean> {
    const start = Date.now();

    while (!this.isActivated()) {
      if (Date.now() - start > timeoutMs) {
        return false;  // Timeout
      }
      await new Promise(resolve => setTimeout(resolve, 50));
    }

    return true;
  }
}
```

---

## PersonaUser Integration (Non-Blocking)

```typescript
export class PersonaUser extends AIUser {
  public genome: PersonaGenome;

  constructor(...) {
    this.genome = new PersonaGenome(this.id);
  }

  /**
   * Process task (non-blocking genome activation)
   */
  async processTask(task: TaskEntity): Promise<void> {
    // 1. Request genome activation (NON-BLOCKING)
    const layers = this.selectLayersForTask(task);
    this.genome.activatePhenotype(layers);

    // 2. Continue processing immediately
    // ... prepare context, validate input, etc.

    // 3. Wait for activation before generation (if needed)
    const activated = await this.genome.waitForActivation(5000);

    if (!activated) {
      console.warn(`Genome activation timeout for ${this.displayName}`);
      // Fallback: use base model without adapters
    }

    // 4. Generate with active genome
    const response = await this.genome.generate(task.prompt);

    // Process response...
  }

  /**
   * Alternative: Fire-and-forget activation
   */
  async processTaskOptimistic(task: TaskEntity): Promise<void> {
    // 1. Request activation (returns immediately)
    const layers = this.selectLayersForTask(task);
    this.genome.activatePhenotype(layers);

    // 2. Generate immediately (may use cached genome from previous task)
    // If genome not ready, uses base model or waits internally
    const response = await this.genome.generate(task.prompt);

    // Process response...
  }
}
```

---

## Signal-Based Activation (Lean Core Loop)

### Signals (Lightweight Checks)

```typescript
interface GenomeSignals {
  hasPendingRequests: boolean;        // Any activation requests queued?
  memoryPressure: number;             // 0.0-1.0 (used / total)
  cacheHitRate: number;               // Last 100 requests
  isIdle: boolean;                    // No pending work?
  shouldCleanup: boolean;             // Time for maintenance?
  thrashingDetected: boolean;         // High evictions + low cache hits?
}

function checkSignals(): GenomeSignals {
  // FAST - just read counters/flags (no heavy operations)
  return {
    hasPendingRequests: this.pendingRequests.length > 0,
    memoryPressure: this.usedMemoryMB / this.totalMemoryMB,
    cacheHitRate: this.cacheHits / (this.cacheHits + this.cacheMisses),
    isIdle: this.pendingRequests.length === 0 && Date.now() - this.lastActivity > 1000,
    shouldCleanup: Date.now() - this.lastCleanup > 60000,
    thrashingDetected: this.evictionsLastMinute > 10 && this.cacheHitRate < 0.3
  };
}
```

### Triggered Actions (Heavy Work)

```typescript
protected async tick(): Promise<void> {
  const signals = this.checkSignals();  // FAST

  // Only do heavy work when triggered by signals

  if (signals.hasPendingRequests) {
    await this.processPendingRequests();  // HEAVY
  }

  if (signals.thrashingDetected) {
    await this.mitigateThrashing();  // HEAVY
  }

  if (signals.isIdle && signals.cacheHitRate < 0.7) {
    await this.predictiveLoad();  // HEAVY
  }

  if (signals.shouldCleanup) {
    await this.cleanupStale();  // HEAVY
  }
}
```

---

## Context-Adaptive Priority

Like MemoryConsolidationSubprocess, adjust based on system state:

```typescript
class GenomeDaemon extends PersonaContinuousSubprocess {
  /**
   * Adjust priority based on system load
   */
  private getEffectivePriority(): SubprocessPriority {
    const signals = this.checkSignals();

    // High load → slow down background work
    if (signals.memoryPressure > 0.9) {
      return 'lowest';  // Reduce frequency
    }

    // Thrashing → speed up (need to fix it)
    if (signals.thrashingDetected) {
      return 'high';  // Urgent mitigation
    }

    // Idle → normal background work
    return 'low';  // Default for GenomeDaemon
  }

  protected async tick(): Promise<void> {
    // Adjust sleep time based on context
    const priority = this.getEffectivePriority();
    this.setPriority(priority);

    // ... rest of tick logic
  }
}
```

---

## Performance Guarantees

### 1. Non-Blocking Activation

**Constraint**: `activatePhenotype()` returns in < 1ms

**How**: Just enqueue request, GenomeDaemon processes asynchronously

**Test**:
```typescript
const start = Date.now();
genome.activatePhenotype(layers);
const elapsed = Date.now() - start;
expect(elapsed).toBeLessThan(1);  // < 1ms
```

### 2. Lean Core Loop

**Constraint**: `tick()` completes in < 10ms when no work pending

**How**: Signal checks are fast (just counters), heavy work is triggered

**Test**:
```typescript
const start = Date.now();
await daemon.tick();  // No pending requests
const elapsed = Date.now() - start;
expect(elapsed).toBeLessThan(10);  // < 10ms
```

### 3. No Main Thread Blocking

**Constraint**: PersonaUser never blocks on genome operations

**How**: Fire-and-forget activation, optional wait with timeout

**Test**:
```typescript
// PersonaUser continues immediately
persona.genome.activatePhenotype(layers);
const canContinue = true;  // Not blocked
expect(canContinue).toBe(true);
```

---

## Comparison: Blocking vs Non-Blocking

### Blocking (WRONG - Main Thread Bottleneck)

```typescript
// ❌ PersonaUser BLOCKS while genome loads
async processTask(task: TaskEntity): Promise<void> {
  const layers = this.selectLayersForTask(task);

  // BLOCKING: PersonaUser waits for loading + eviction + composition
  await this.genome.activatePhenotype(layers);  // 500ms-2s!!!

  const response = await this.genome.generate(task.prompt);
}
```

**Problems**:
- PersonaUser blocked for 500ms-2s per activation
- Can't process other tasks while waiting
- No concurrent activations across personas
- Main thread bottlenecked

### Non-Blocking (RIGHT - Background Subprocess)

```typescript
// ✅ PersonaUser continues immediately
async processTask(task: TaskEntity): Promise<void> {
  const layers = this.selectLayersForTask(task);

  // NON-BLOCKING: Returns in <1ms
  this.genome.activatePhenotype(layers);

  // Continue processing immediately
  await this.prepareContext(task);
  await this.validateInput(task);

  // Wait only if needed (with timeout)
  await this.genome.waitForActivation(5000);

  const response = await this.genome.generate(task.prompt);
}
```

**Benefits**:
- PersonaUser never blocked
- GenomeDaemon handles activation in background
- Multiple activations can happen concurrently
- Main thread stays responsive

---

## Subprocess Communication

### 1. PersonaUser → GenomeDaemon (Request)

```typescript
// Non-blocking request
genome.requestActivation(personaId, layers, callback);
```

### 2. GenomeDaemon → PersonaUser (Callback)

```typescript
// Daemon calls callback when complete
callback({ cacheHit: false, latencyMs: 250 });
```

### 3. PersonaUser → GenomeDaemon (Query)

```typescript
// Check activation status (non-blocking)
const isReady = genome.isActivated();
```

---

## Integration with ResourceManager

```typescript
class GenomeDaemon extends PersonaContinuousSubprocess {
  private resourceManager: ResourceManager;

  async initialize(): Promise<void> {
    this.resourceManager = ResourceManager.getInstance();

    // Get GPU memory quota
    const totalGpuMemory = this.resourceManager.getSystemResources().totalGpuMemory;
    this.totalMemoryMB = totalGpuMemory * 0.5;  // Reserve 50% for adapters
  }

  private async ensureQuotaAvailable(
    personaId: UUID,
    layers: LayerActivation[]
  ): Promise<void> {
    const required = this.calculateMemoryRequired(layers);

    // Get persona quota from ResourceManager
    const quota = this.resourceManager.calculateGpuQuota(personaId, {
      requestType: 'genome-activation',
      priority: this.getPersonaPriority(personaId)
    });

    if (required > quota) {
      // Evict LRU adapters to make space
      await this.evictionPolicy.evictUntilAvailable(required);
    }
  }
}
```

---

## Testing Strategy

### Unit Tests (Subprocess Behavior)

```typescript
describe('GenomeDaemon Subprocess', () => {
  let daemon: GenomeDaemon;

  beforeEach(async () => {
    daemon = GenomeDaemon.getInstance();
    await daemon.start();
  });

  it('should return immediately on requestActivation', () => {
    const start = Date.now();

    daemon.requestActivation(personaId, layers);

    const elapsed = Date.now() - start;
    expect(elapsed).toBeLessThan(1);  // < 1ms
  });

  it('should process pending requests in background', async () => {
    daemon.requestActivation(personaId, layers);

    // Wait for background processing
    await new Promise(resolve => setTimeout(resolve, 500));

    const isActivated = daemon.isActivated(personaId, layers);
    expect(isActivated).toBe(true);
  });

  it('should have lean tick() when no work', async () => {
    const start = Date.now();

    await daemon.tick();  // No pending requests

    const elapsed = Date.now() - start;
    expect(elapsed).toBeLessThan(10);  // < 10ms
  });

  afterEach(async () => {
    await daemon.stop();
  });
});
```

### Integration Tests (PersonaUser + GenomeDaemon)

```typescript
describe('PersonaUser + GenomeDaemon Integration', () => {
  it('should not block PersonaUser during activation', async () => {
    const persona = new PersonaUser(...);
    const task = createTestTask();

    const start = Date.now();

    // Start task processing
    const taskPromise = persona.processTask(task);

    // PersonaUser should continue immediately (not blocked)
    const immediateElapsed = Date.now() - start;
    expect(immediateElapsed).toBeLessThan(10);  // Not blocked

    // Wait for task to complete
    await taskPromise;

    // Genome was activated in background
    expect(persona.genome.isActivated()).toBe(true);
  });
});
```

---

## Implementation Priority

### Phase 1: GenomeDaemon Subprocess Foundation

**Tasks**:
1. Create GenomeDaemon extending PersonaContinuousSubprocess
2. Implement signal-based tick()
3. Implement requestActivation() (non-blocking)
4. Implement processPendingRequests() (background)
5. Add PersonaGenome thin wrapper

**Deliverable**: Non-blocking genome activation

**Testing**:
```typescript
daemon.requestActivation(personaId, layers);  // <1ms
await sleep(500);  // Wait for background processing
expect(daemon.isActivated(personaId, layers)).toBe(true);
```

### Phase 2: Adapter Integration

**Tasks**:
1. Implement LocalGenomeStorage adapter
2. Implement SingleLayerCompositor adapter
3. Implement OllamaBackend adapter
4. Implement LRUPolicy adapter

**Deliverable**: Functional genome loading (single-layer)

### Phase 3: Performance Optimizations

**Tasks**:
1. Implement thrashing detection
2. Implement hysteresis
3. Implement cache hit tracking
4. Implement predictive loading
5. Add context-adaptive priority

**Deliverable**: Production-grade performance

---

## Key Design Decisions

### 1. Subprocess, Not Daemon Command

**Decision**: GenomeDaemon extends PersonaContinuousSubprocess

**Rationale**:
- Consistent with RTOS architecture (MemoryConsolidation, TaskGeneration)
- Non-blocking by design
- Priority-based timing
- Base class handles threading/queue/errors

### 2. Fire-and-Forget Activation

**Decision**: `activatePhenotype()` returns immediately

**Rationale**:
- PersonaUser never blocked
- Background processing
- Optional wait with timeout for cases that need it

### 3. Signal-Based, Not Continuous

**Decision**: tick() checks signals, triggers heavy work only when needed

**Rationale**:
- Lean core loop (< 10ms)
- Like cbar's motion detection → semantic segmentation
- No continuous polling/processing when idle

### 4. Callback Notification

**Decision**: Optional callback when activation complete

**Rationale**:
- PersonaUser can be notified asynchronously
- Fire-and-forget if notification not needed
- No polling required

---

## Success Criteria

**Performance**:
- ✅ `activatePhenotype()` returns in < 1ms
- ✅ `tick()` completes in < 10ms when no work
- ✅ PersonaUser never blocked on genome operations
- ✅ Multiple concurrent activations across personas

**Functionality**:
- ✅ Non-blocking activation
- ✅ Background processing
- ✅ Callback notification
- ✅ Optional synchronous wait

**Architecture**:
- ✅ Extends PersonaContinuousSubprocess
- ✅ Signal-based activation
- ✅ Context-adaptive priority
- ✅ Integration with ResourceManager

---

## Related Documents

**RTOS Foundation**:
- `.doc-staging/memory/rtos-final-architecture.md` - RTOS principles
- `.doc-staging/persona/subprocess-pattern.md` - PersonaSubprocess pattern
- `.doc-staging/memory/lean-core-loop-pattern.md` - Signal-based activation

**Genome Architecture**:
- `.doc-staging/genome/PERFORMANT-GENOME-ARCHITECTURE.md` - Adapter-driven design
- `docs/GENOME-DAEMON-ARCHITECTURE.md` - Original daemon design (update needed)

**Implementation**:
- `system/user/server/modules/PersonaSubprocess.ts` - Base class
- `system/user/server/modules/cognition/memory/MemoryConsolidationSubprocess.ts` - Example subprocess

---

## Summary

**GenomeDaemon = PersonaContinuousSubprocess + Adapter-Driven Design + Non-Blocking**

**Key Properties**:
1. Runs in separate thread (like MemoryConsolidation)
2. Non-blocking for PersonaUser (<1ms activation request)
3. Signal-based (lean tick(), heavy work triggered)
4. Context-adaptive (adjust priority based on load)
5. Adapter-driven (pluggable backends/storage/composition)

**Result**: Production-grade genome system that NEVER blocks the main thread

