# Dynamic Genome Assembly & Process Isolation Architecture
**Phase 2 REVISED: Elastic AI Process Management**

## Core Insight

> **"Dynamically assembled genome, on the fly as necessary for the prediction as it comes in. Queues. Operate as efficiently as possible on these machines, but illustrate how we can dynamically operate around many personas simultaneously, and managing APIs failing/leaking, etc, kind of like elastic systems do for docker containers."**

## Critical Design Changes

### Before (Initial Design - WRONG)
- ❌ Pre-load entire genomes into memory
- ❌ Worker threads for model loading
- ❌ Shared memory between operations
- ❌ One crash affects all personas

### After (Corrected Design - RIGHT)
- ✅ **Dynamic assembly**: Stack LoRA layers on-demand per request
- ✅ **Process isolation**: Each inference runs in isolated child process
- ✅ **Elastic management**: Spawn/kill processes like Docker containers
- ✅ **Failure isolation**: One persona's crash doesn't affect others

## Architecture: Elastic Process Pool

```
AIProviderDaemon
├── ProcessPool (like K8s pod manager)
│   ├── Health Monitor (watchdog)
│   ├── Process Spawner (creates isolated processes)
│   ├── Resource Limits (memory/CPU per process)
│   └── Crash Recovery (auto-restart failed processes)
│
├── GenomeAssembler (dynamic LoRA stacking)
│   ├── Layer Cache (LRU cache of individual layers)
│   ├── Assembly Queue (build genome on-demand)
│   └── Layer Loader (load individual LoRA layers)
│
└── Request Queue
    ├── Priority Queue (persona requests)
    ├── Load Balancer (distribute across processes)
    └── Circuit Breaker (fail fast on unhealthy processes)
```

## Dynamic Genome Assembly Flow

```
1. PersonaUser sends message
   ↓
2. GenomeAssembler receives genomeId from PersonaUser.genomeId
   ↓
3. Load GenomeEntity + GenomeLayerEntity[] from database
   ↓
4. FOR EACH layer in genome.layers (bottom-up):
      - Check layer cache (hit? use it, miss? load from disk)
      - Stack layer onto base model
   ↓
5. Spawn isolated process with assembled genome
   ↓
6. Run inference in process
   ↓
7. Kill process (cleanup memory leaks)
   ↓
8. Return result to PersonaUser
```

## Process Isolation Strategy

### Why Child Processes > Worker Threads?
- **Memory isolation**: Process crash doesn't affect others
- **Resource limits**: Can set per-process memory/CPU limits
- **Leak containment**: Memory leaks die with process
- **True parallelism**: Not bound by V8 single-threaded execution

### Process Lifecycle
```typescript
// Spawn process for each inference request
const process = spawn('node', ['inference-worker.js'], {
  env: {
    GENOME_ID: persona.genomeId,
    BASE_MODEL: 'llama3.2:1b',
    MAX_MEMORY_MB: '1024'
  },
  timeout: 30000,  // Kill after 30s
  killSignal: 'SIGKILL'  // Force kill
});

// Monitor health
process.on('exit', (code) => {
  if (code !== 0) {
    console.error('Process crashed, spawning replacement...');
    metrics.recordCrash(genomeId);
  }
});

// Resource monitoring
const memoryUsage = process.memoryUsage();
if (memoryUsage > limits.maxMemory) {
  process.kill('SIGKILL');
  metrics.recordOOM(genomeId);
}
```

## Elastic Process Management (like Kubernetes)

### Pool Configuration
```typescript
interface ProcessPoolConfig {
  minProcesses: number;        // Always running (warm pool)
  maxProcesses: number;        // Hard limit
  idleTimeout: number;         // Kill idle processes after X ms
  maxMemoryPerProcess: number; // MB limit per process
  maxCPUPerProcess: number;    // CPU percentage limit
  crashBackoff: number;        // Delay before restarting crashed process
}
```

### Health Monitoring
```typescript
// Watchdog checks every 5s
setInterval(() => {
  for (const process of pool.active) {
    const health = await checkProcessHealth(process);

    if (health.memoryLeaking) {
      console.warn('Memory leak detected, killing process...');
      process.kill('SIGKILL');
    }

    if (health.notResponding) {
      console.warn('Process hung, killing process...');
      process.kill('SIGKILL');
    }

    if (health.errorRate > 0.5) {
      console.warn('High error rate, circuit breaker activated');
      pool.circuitBreaker.open(process.genomeId);
    }
  }
}, 5000);
```

### Auto-Scaling
```typescript
// Scale up when queue builds
if (requestQueue.length > 10 && pool.size < config.maxProcesses) {
  console.log('Queue building, spawning new process...');
  pool.spawn();
}

// Scale down when idle
if (pool.idleCount > 3 && pool.size > config.minProcesses) {
  console.log('Excess capacity, killing idle process...');
  pool.killOldestIdle();
}
```

## Layer Cache Strategy

### Why Cache Individual Layers?
- **Reusability**: Many personas may share base layers
- **Memory efficiency**: Don't reload common layers
- **Fast assembly**: Cache hit = instant assembly

### LRU Cache Design
```typescript
class LayerCache {
  private cache = new Map<UUID, LoRALayer>();
  private maxSize = 10; // 10 layers max

  get(layerId: UUID): LoRALayer | null {
    const layer = this.cache.get(layerId);
    if (layer) {
      // Move to front (LRU)
      this.cache.delete(layerId);
      this.cache.set(layerId, layer);
    }
    return layer || null;
  }

  set(layerId: UUID, layer: LoRALayer): void {
    // Evict oldest if full
    if (this.cache.size >= this.maxSize) {
      const oldest = this.cache.keys().next().value;
      this.cache.delete(oldest);
      console.log(`Evicted layer ${oldest} from cache`);
    }
    this.cache.set(layerId, layer);
  }
}
```

## Integration with PersonaUser

### Request Flow
```typescript
// PersonaUser.generateResponse()
async generateResponse(message: string): Promise<string> {
  // 1. Get my genome ID
  const genomeId = this.entity.genomeId;

  if (!genomeId) {
    // No genome = use base model (standard Ollama)
    return await AIProviderDaemon.generateText({
      messages: [{ role: 'user', content: message }],
      model: 'llama3.2:1b'
    });
  }

  // 2. Request genome-adapted inference
  return await GenomeInferenceService.generateWithGenome({
    genomeId,
    messages: [{ role: 'user', content: message }],
    personaId: this.entity.id
  });
}
```

### GenomeInferenceService
```typescript
class GenomeInferenceService {
  static async generateWithGenome(request: GenomeInferenceRequest): Promise<string> {
    // 1. Assemble genome dynamically
    const genome = await GenomeAssembler.assemble(request.genomeId);

    // 2. Get process from pool (or spawn new one)
    const process = await ProcessPool.acquire({
      genomeId: request.genomeId,
      maxMemory: genome.estimatedMemoryMB,
      timeout: 30000
    });

    try {
      // 3. Run inference in isolated process
      const result = await process.inference({
        genome,
        messages: request.messages
      });

      return result.text;

    } finally {
      // 4. ALWAYS release process (even on error)
      await ProcessPool.release(process);
    }
  }
}
```

## Testing Strategy (Revised)

### Test 1: Dynamic Assembly
```typescript
✅ Load genome with 3 layers → Assembled correctly
✅ Cache hit on repeated load → Fast assembly
✅ LRU eviction works → Oldest layer removed
```

### Test 2: Process Isolation
```typescript
✅ Spawn process for inference → Succeeds
✅ Kill process after inference → Memory freed
✅ Process crash → Doesn't affect others
✅ Memory leak in process → Process killed, pool recovers
```

### Test 3: Elastic Scaling
```typescript
✅ 10 requests, 3 process pool → Queue + spawn new processes
✅ Idle timeout → Kill idle processes
✅ Max processes reached → Queue requests, don't crash
```

### Test 4: Failure Recovery
```typescript
✅ Process crashes mid-inference → Auto-restart + retry
✅ All processes crash → Circuit breaker + graceful degradation
✅ Memory leak → Detect + kill + spawn replacement
✅ API timeout → Kill hung process + retry
```

### Test 5: Concurrent Personas
```typescript
✅ 5 personas generate simultaneously → All succeed
✅ Shared layer cache → Efficient memory usage
✅ One persona crashes → Others unaffected
```

## Implementation Phases (Revised)

### Phase 2.1: Process Pool Infrastructure ✅ NEXT
- Create `ProcessPool.ts` (elastic pool manager)
- Create `inference-worker.ts` (isolated process script)
- Integration test: Spawn/kill processes

### Phase 2.2: Dynamic Genome Assembly
- Implement `GenomeAssembler.ts` (dynamic LoRA stacking)
- Implement `LayerCache.ts` (LRU cache)
- Integration test: Assemble genome on-demand

### Phase 2.3: Inference Service Integration
- Implement `GenomeInferenceService.ts` (request routing)
- Integrate with PersonaUser
- Integration test: PersonaUser generates with genome

### Phase 2.4: Elastic Management & Monitoring
- Implement health monitoring & auto-scaling
- Add circuit breaker for failing genomes
- Integration test: Failure recovery & scaling

## Success Criteria

- ✅ 10 personas generate simultaneously without interference
- ✅ Process crash doesn't affect other personas
- ✅ Memory leak contained to single process
- ✅ Layer cache improves assembly time by 10x
- ✅ Auto-scaling handles burst traffic
- ✅ Circuit breaker prevents cascading failures

## Key Architectural Principles

1. **Isolation**: Each inference runs in its own process
2. **Ephemeral**: Processes are disposable, spawn/kill freely
3. **Dynamic**: Assemble genomes on-demand, don't pre-load
4. **Elastic**: Scale processes up/down based on demand
5. **Resilient**: Failures are isolated and recoverable

This is **exactly** like how Kubernetes manages Docker containers, but for AI inference processes.
