# PersonaUser Resource Leasing Models

## The Critical Question

**How should PersonaUsers interact with the global ResourceManager for GPU/LoRA resources?**

Two fundamentally different models:

### Model A: Lease-Based Materialization (Heavy Sessions)
```
PersonaUser spawns for a session
  ↓
Request GPU lease from ResourceManager (e.g., 2GB for 10 minutes)
  ↓
Load ALL required LoRA layers at once (full materialization)
  ↓
Operate with guaranteed resources for lease duration
  ↓
Lease expires OR session completes
  ↓
Release all resources
  ↓
Tear down PersonaUser (or enter dormant state)
```

### Model B: Incremental Layer Paging (Lightweight Operations)
```
PersonaUser always running (CNS service loop)
  ↓
Message arrives → Need "typescript-expertise" adapter
  ↓
Request layer from ResourceManager
  ↓
ResourceManager pages in layer (2-5s) OR denies (no capacity)
  ↓
Use adapter for this message
  ↓
ResourceManager may evict layer later if idle
  ↓
PersonaUser continues running, requests layers as needed
```

---

## Evidence from Existing ResourceManager

Looking at `system/resources/shared/ResourceManager.ts`:

### Supports BOTH Models

**For Model A (Lease-Based)**:
```typescript
interface ResourceRequest {
  requestType: 'evaluation' | 'model_load' | 'worker_spawn';
  gpuMemoryNeeded?: number;    // Request specific GPU allocation
  workerNeeded?: boolean;
  priority: 'low' | 'normal' | 'high' | 'critical';
  estimatedDuration?: number;  // <-- Lease duration!
}

interface ResourceDecision {
  granted: boolean;
  grantedGpuMemory?: number;   // May grant less than requested
  waitTimeMs?: number;         // Queue wait time
}
```

**For Model B (Incremental)**:
```typescript
interface AdapterResources {
  lastActivityTime: number;    // Track idle time
  gpuMemoryUsed: number;       // Current usage
  gpuMemoryQuota: number;      // Max allowed
}

// Resource reclamation for idle adapters
performCleanup(): void {
  const suggestions = this.moderator.suggestReclamation(context);
  // Evict idle adapters to free GPU memory
}
```

**Key Methods**:
- `registerAdapter()` - Register ONCE (supports Model B)
- `requestResources()` - Request per-operation (supports both)
- `releaseResources()` - Release after use (supports both)
- `performCleanup()` - Evict idle (supports Model B)

---

## Model A: Lease-Based Materialization

### Use Cases
- **Heavy training sessions**: Fine-tuning LoRA adapters (30-60 minutes)
- **Realtime games**: Guaranteed 16ms response time, no paging delays
- **Deep work sessions**: Code review on large PRs (15-30 minutes)
- **Batch processing**: Process 100 messages without interruption

### Lifecycle
```typescript
// PersonaUser spawns for training session
async materialize(session: TrainingSession): Promise<void> {
  // Request GPU lease
  const decision = await resourceManager.requestResources({
    adapterId: this.id,
    requestType: 'model_load',
    gpuMemoryNeeded: 2048,        // 2GB for full genome
    estimatedDuration: 1800000,   // 30 minutes
    priority: 'high'
  });

  if (!decision.granted) {
    console.log(`⏳ Queued: Wait ${decision.waitTimeMs}ms for GPU availability`);
    await sleep(decision.waitTimeMs);
    return this.materialize(session);  // Retry
  }

  // Load ALL LoRA layers at once
  console.log('🧬 Loading full genome (all adapters)...');
  await this.genome.loadAllAdapters();  // 2-10 seconds to load everything

  // Set lease expiration
  this.leaseExpiresAt = Date.now() + 1800000;  // 30 minutes from now

  console.log('✅ Materialized with guaranteed GPU lease');
}

// Work with guaranteed resources
async operateDuringLease(): Promise<void> {
  while (Date.now() < this.leaseExpiresAt) {
    // Process messages with zero paging delays (all adapters loaded)
    await this.processNextMessage();  // <1ms adapter switching
  }

  // Lease expired
  await this.dematerialize();
}

// Release resources
async dematerialize(): Promise<void> {
  console.log('🗑️  Lease expired, dematerializing...');

  // Unload ALL adapters
  await this.genome.unloadAllAdapters();

  // Release GPU memory
  await resourceManager.releaseResources(this.id, 'gpu_memory', 2048);

  // Enter dormant state (or tear down completely)
  this.state = 'dormant';
}
```

### Advantages
- ✅ **No paging delays** during session (all adapters pre-loaded)
- ✅ **Predictable performance** (guaranteed resources)
- ✅ **Good for intensive workloads** (training, games, batch processing)
- ✅ **Clear resource boundaries** (explicit lease start/end)

### Disadvantages
- ❌ **Heavy upfront cost** (2-10 seconds to load all adapters)
- ❌ **Resource hogging** (locks GPU even when idle during lease)
- ❌ **Inflexible** (can't easily switch to unexpected domains)
- ❌ **Wasted resources** if session ends early

---

## Model B: Incremental Layer Paging

### Use Cases
- **Casual chat**: Respond to occasional messages (low frequency)
- **Multi-domain assistant**: Switch between code/chat/vision frequently
- **Background agents**: Always-on personas with sporadic work
- **Resource-constrained**: Many personas sharing limited GPU

### Lifecycle
```typescript
// PersonaUser always running (CNS service loop)
async initialize(): Promise<void> {
  // Register with ResourceManager ONCE
  await resourceManager.registerAdapter(this.id, this.displayName);
  console.log('📋 Registered with ResourceManager');

  // Start autonomous loop (CNS)
  this.cns.start();
}

// Request adapter as needed
async handleMessage(message: ChatMessageEntity): Promise<void> {
  // Determine required domain
  const domain = this.classifyMessageDomain(message);  // 'typescript' | 'chat' | etc
  const adapterName = this.domainToAdapter[domain];

  // Check if already loaded
  if (this.genome.isAdapterLoaded(adapterName)) {
    console.log(`⚡ Adapter cached: ${adapterName} (0ms)`);
    await this.respondToMessage(message);
    return;
  }

  // Request from ResourceManager
  console.log(`📥 Requesting adapter: ${adapterName}`);
  const decision = await resourceManager.requestResources({
    adapterId: this.id,
    requestType: 'model_load',
    gpuMemoryNeeded: 512,     // 512MB for one adapter
    priority: 'normal'
  });

  if (!decision.granted) {
    console.log(`⏳ GPU unavailable: ${decision.reason}`);
    // Fallback: Use base model without LoRA, or queue for later
    await this.respondWithBaseModel(message);
    return;
  }

  // Page in adapter (2-5 seconds)
  console.log(`💾 Paging in: ${adapterName} (2-5s)`);
  await this.genome.loadAdapter(adapterName);

  // May need to evict LRU adapter if quota exceeded
  if (this.genome.memoryUsed > this.genome.memoryQuota) {
    const lruAdapter = this.genome.getLRUAdapter();
    console.log(`🗑️  Evicting LRU: ${lruAdapter}`);
    await this.genome.unloadAdapter(lruAdapter);
    await resourceManager.releaseResources(this.id, 'gpu_memory', 512);
  }

  // Now respond with adapter
  await this.respondToMessage(message);

  // ResourceManager may reclaim later during cleanup
}
```

### Advantages
- ✅ **Lightweight startup** (register only, don't load adapters)
- ✅ **Dynamic resource sharing** (GPU freed when idle)
- ✅ **Flexible domain switching** (load any adapter as needed)
- ✅ **Better for many personas** (resource pooling across ~10 personas)

### Disadvantages
- ❌ **Paging delays** (2-5s first use per adapter per session)
- ❌ **Unpredictable performance** (may get denied during high load)
- ❌ **Complexity** (LRU eviction, cache management, fallbacks)
- ❌ **Not suitable for realtime** (can't afford 5s paging delays in games)

---

## Hybrid Model: Best of Both Worlds

### Concept
PersonaUsers can **request different resource modes** based on the task:

```typescript
enum ResourceMode {
  DORMANT,       // Not using any GPU (database-backed state only)
  LIGHTWEIGHT,   // Incremental paging (Model B)
  SESSION,       // Lease-based full materialization (Model A)
  CRITICAL       // Guaranteed resources (games, demos)
}

class PersonaUser {
  private resourceMode: ResourceMode = ResourceMode.DORMANT;

  /**
   * Transition to different resource mode
   */
  async requestMode(mode: ResourceMode, duration?: number): Promise<boolean> {
    switch (mode) {
      case ResourceMode.LIGHTWEIGHT:
        // Register for incremental paging
        await resourceManager.registerAdapter(this.id, this.displayName);
        this.resourceMode = mode;
        return true;

      case ResourceMode.SESSION:
        // Request GPU lease for session
        const decision = await resourceManager.requestResources({
          adapterId: this.id,
          requestType: 'model_load',
          gpuMemoryNeeded: 2048,
          estimatedDuration: duration || 1800000,  // Default 30 min
          priority: 'high'
        });

        if (decision.granted) {
          await this.genome.loadAllAdapters();  // Full materialization
          this.resourceMode = mode;
          return true;
        }
        return false;  // Denied, stay in current mode

      case ResourceMode.CRITICAL:
        // Request guaranteed resources (highest priority)
        const critical = await resourceManager.requestResources({
          adapterId: this.id,
          requestType: 'model_load',
          gpuMemoryNeeded: 2048,
          priority: 'critical'  // Preempt other personas if needed
        });

        if (critical.granted) {
          await this.genome.loadAllAdapters();
          this.resourceMode = mode;
          return true;
        }
        return false;

      case ResourceMode.DORMANT:
        // Release all resources
        await this.genome.unloadAllAdapters();
        await resourceManager.releaseResources(this.id, 'gpu_memory', this.gpuMemoryUsed);
        this.resourceMode = mode;
        return true;
    }
  }
}
```

### Use Case Examples

**Casual Chat** (LIGHTWEIGHT):
```typescript
// PersonaUser starts in LIGHTWEIGHT mode
await personaUser.requestMode(ResourceMode.LIGHTWEIGHT);

// Messages arrive sporadically
// Adapters paged in/out as needed (2-5s delays acceptable)
```

**Training Session** (SESSION):
```typescript
// User starts training session
await personaUser.requestMode(ResourceMode.SESSION, 3600000);  // 1 hour lease

// All adapters pre-loaded, zero paging delays during session
// Lease expires after 1 hour OR session completes early
```

**Realtime Game** (CRITICAL):
```typescript
// User starts game
await personaUser.requestMode(ResourceMode.CRITICAL);

// Guaranteed 16ms response time (no paging, highest priority)
// May preempt other personas to free GPU
```

**Idle Overnight** (DORMANT):
```typescript
// System detects no activity for 30 minutes
await personaUser.requestMode(ResourceMode.DORMANT);

// All GPU resources released
// Persona state persisted to database
// Can reactivate quickly when needed
```

---

## Implications for PersonaMemory Refactoring

### Model A (Lease-Based)
```typescript
export class PersonaMemory {
  /**
   * Load full genome for lease period
   */
  async materializeGenome(): Promise<void> {
    console.log('🧬 Loading full genome...');
    const adapters = ['typescript-expertise', 'conversational', 'code-review', ...];

    for (const adapter of adapters) {
      await this.genome.loadAdapter(adapter);  // 2-5s each
    }

    console.log(`✅ Loaded ${adapters.length} adapters (${adapters.length * 3}s total)`);
  }

  /**
   * Adapter switching is instant (all cached)
   */
  async activateSkill(adapterName: string): Promise<void> {
    if (!this.loadedAdapters.has(adapterName)) {
      throw new Error(`Adapter ${adapterName} not materialized!`);
    }

    // Instant switching (<1ms)
    this.activeAdapter = adapterName;
  }
}
```

### Model B (Incremental Paging)
```typescript
export class PersonaMemory {
  private loraCache: Map<string, LoRAAdapter> = new Map();
  private maxCacheSize: number = 3;  // Max 3 adapters loaded simultaneously
  private lruOrder: string[] = [];

  /**
   * Load adapter on demand with LRU caching
   */
  async activateSkill(adapterName: string): Promise<void> {
    // FAST PATH: Already cached (0ms)
    if (this.loraCache.has(adapterName)) {
      console.log(`⚡ Cache hit: ${adapterName}`);
      this.updateLRU(adapterName);
      return;
    }

    // SLOW PATH: Need to page in (2-5s)
    console.log(`💾 Cache miss: ${adapterName} (paging...)`);

    // Request from ResourceManager
    const decision = await resourceManager.requestResources({
      adapterId: this.personaId,
      requestType: 'model_load',
      gpuMemoryNeeded: 512,
      priority: 'normal'
    });

    if (!decision.granted) {
      console.log(`⏳ GPU unavailable: ${decision.reason}`);
      throw new Error('GPU resources unavailable');
    }

    // Evict LRU if cache full
    if (this.loraCache.size >= this.maxCacheSize) {
      const lruAdapter = this.lruOrder[0];
      console.log(`🗑️  Evicting LRU: ${lruAdapter}`);
      await this.unloadAdapter(lruAdapter);
      await resourceManager.releaseResources(this.personaId, 'gpu_memory', 512);
    }

    // Page in adapter (2-5s)
    const adapter = await this.genome.loadAdapter(adapterName);
    this.loraCache.set(adapterName, adapter);
    this.lruOrder.push(adapterName);

    console.log(`✅ Paged in: ${adapterName}`);
  }
}
```

---

## Recommendation: Start with Hybrid Model

### Phase 1: Implement LIGHTWEIGHT Mode (Model B)
Most PersonaUsers will operate in LIGHTWEIGHT mode:
- Register with ResourceManager on initialization
- Page adapters incrementally (LRU caching)
- Graceful degradation when resources unavailable

**Why first**: Covers 80% of use cases (casual chat, background agents, multi-domain assistants)

### Phase 2: Add SESSION Mode (Model A) for Specific Use Cases
Heavy workloads can request SESSION mode:
- Training sessions (request 30-60 minute lease)
- Deep work sessions (code review, article writing)
- Demo/presentation mode (guaranteed performance)

**Why second**: Only needed for 20% of use cases, but critical for those

### Phase 3: Add CRITICAL Mode for Realtime Requirements
Realtime games, live demos:
- Highest priority (preempts other personas if needed)
- Guaranteed resources (no denials)
- Zero paging delays (all adapters pre-loaded)

**Why third**: Rare but essential for realtime contracts

---

## Questions to Resolve

1. **Default mode for new PersonaUsers?**
   - Proposed: LIGHTWEIGHT (most flexible, best resource sharing)

2. **Who decides mode transitions?**
   - User explicitly (via UI): "Start training session" → SESSION mode
   - PersonaUser autonomously (CNS): Detects intensive task → request SESSION
   - ResourceManager suggestion: High GPU pressure → force DORMANT for idle personas

3. **Lease duration limits?**
   - Proposed: SESSION mode max 2 hours, then auto-renew or dematerialize
   - CRITICAL mode max 1 hour (to prevent resource hogging)

4. **What happens when lease denied?**
   - Queue and wait (with estimated wait time)
   - Operate in LIGHTWEIGHT mode instead (with paging delays)
   - Notify user "GPU busy, estimated wait: 5 minutes"

5. **How to handle lease expiration during active work?**
   - Auto-renew if still active (with permission check)
   - Graceful degradation to LIGHTWEIGHT mode
   - Save state and prompt user "Extend session?"

---

## Integration with CNS and Tier 2 Scheduler

The HeuristicCognitiveScheduler (Tier 2) needs to know current resource mode:

```typescript
async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
  const adapter = this.domainToAdapter[domain];

  // Check resource mode
  switch (this.personaUser.resourceMode) {
    case ResourceMode.CRITICAL:
      // Always service (guaranteed resources)
      return true;

    case ResourceMode.SESSION:
      // Check if adapter loaded
      return this.personaUser.genome.isAdapterLoaded(adapter);

    case ResourceMode.LIGHTWEIGHT:
      // Check if can afford paging delay
      const adapterCached = this.personaUser.genome.isAdapterLoaded(adapter);
      if (!adapterCached && context.activeGames > 0) {
        console.log(`⚠️  Can't page adapter during game (would block game loop)`);
        return false;  // Don't page during realtime game
      }
      return true;  // Allow paging for non-realtime domains

    case ResourceMode.DORMANT:
      // No GPU access
      return false;
  }
}
```

This preserves the tiered architecture while adding resource-aware decision making.

---

## Next Steps

1. **Decide on default model**: LIGHTWEIGHT, SESSION, or HYBRID?
2. **Update PERSONA-PERFORMANCE-ARCHITECTURE.md** with chosen model
3. **Update PersonaMemory design** to implement chosen model
4. **Define ResourceRequest patterns** for PersonaUser lifecycle
5. **Test resource contention** with multiple personas

**My recommendation**: Start with LIGHTWEIGHT (Model B) for the refactoring, add SESSION mode later as needed. This keeps the refactoring focused while allowing future evolution to heavier resource modes.
