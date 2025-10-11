# Genome Loader Architecture
**Phase 2: LoRA Model Loading with Worker Threads**

## Overview

The Genome Loader extends the existing AIProviderDaemon to support LoRA-adapted models (genomes). It uses Node.js worker threads to handle CPU-intensive model loading operations without blocking the main event loop.

## Architecture

### Components

```
AIProviderDaemon (existing)
├── OllamaAdapter (existing)
└── GenomeLoader (NEW)
    ├── Worker Thread Pool
    │   ├── ModelWorker (3-5 threads)
    │   └── Task Queue
    ├── Genome Cache
    │   ├── LRU eviction
    │   └── Memory monitoring
    └── Lifecycle Manager
        ├── Load genome
        ├── Unload genome
        └── Health check
```

### Key Design Decisions

**1. Worker Threads vs Child Processes**
- **Choice**: Worker threads
- **Why**:
  - Shared memory for efficient model data transfer
  - Lower overhead than child processes
  - Native TypeScript/Node.js integration
  - Simpler IPC (structured clone vs JSON serialization)

**2. Pool Size**
- **Choice**: 3 workers (configurable)
- **Why**:
  - Balance between parallelism and memory usage
  - Most machines have 4+ cores
  - Leaves resources for main thread and OS

**3. Communication Pattern**
- **Choice**: Message passing with Promise wrappers
- **Why**:
  - Type-safe with TypeScript
  - Async/await friendly
  - Clear ownership of tasks

## Test Strategy

### Critical Failure Modes (What we're testing for)

1. **Worker Crash Recovery**
   - Worker dies mid-load → Auto-restart + retry
   - Multiple workers crash → Circuit breaker

2. **Memory Leaks**
   - Models not unloaded → OOM crash
   - Task queue grows unbounded → Memory exhaustion
   - Circular references → Gradual memory growth

3. **Deadlocks**
   - Worker waits for main thread → Deadlock
   - Main thread waits for worker → Deadlock
   - Two workers wait for each other → Deadlock

4. **Race Conditions**
   - Load + Unload same genome → Undefined state
   - Multiple loads of same genome → Duplicate memory usage
   - Concurrent access to shared state → Data corruption

5. **Resource Exhaustion**
   - Too many concurrent loads → CPU thrashing
   - Models exceed memory limit → OOM
   - Disk I/O saturation → Timeouts

### Integration Test Plan

**Test 1: Basic Load/Unload**
```typescript
✅ Load a genome → Verify in memory
✅ Unload genome → Verify freed
✅ Load → Unload → Load again → Works
```

**Test 2: Concurrent Operations**
```typescript
✅ Load 3 genomes in parallel → All succeed
✅ Load 10 genomes (pool size 3) → Queuing works
✅ Load + Unload same genome → Cancellation works
```

**Test 3: Worker Crash Recovery**
```typescript
✅ Kill worker mid-load → Auto-restart + retry
✅ Kill all workers → Circuit breaker activates
✅ Workers recover → System resumes
```

**Test 4: Memory Management**
```typescript
✅ Load 100MB genome → Memory usage increases
✅ Unload genome → Memory usage decreases
✅ Load 10 genomes → LRU eviction works
✅ Memory limit reached → Reject new loads
```

**Test 5: Performance Under Load**
```typescript
✅ 100 sequential loads → Avg < 2s each
✅ 100 concurrent loads → No crashes
✅ Load while inferencing → No blocking
```

**Test 6: Error Handling**
```typescript
✅ Load non-existent genome → Graceful error
✅ Load corrupted genome → Detect + report
✅ Timeout on slow load → Cancel + retry
✅ Worker throws exception → Caught + logged
```

## Implementation Phases

### Phase 2.1: Worker Thread Infrastructure ✅ NEXT
- Create `ModelWorker.ts` (worker thread script)
- Create `WorkerPool.ts` (manages worker lifecycle)
- Integration test: Load/unload simple payload

### Phase 2.2: Genome Loading Logic
- Implement genome file loading in worker
- Add genome cache with LRU eviction
- Integration test: Load actual LoRA weights

### Phase 2.3: AIProviderDaemon Integration
- Extend OllamaAdapter for genome-adapted inference
- Add genome routing in AIProviderDaemon
- Integration test: PersonaUser generates with genome

### Phase 2.4: Production Hardening
- Add comprehensive error handling
- Implement circuit breaker pattern
- Add monitoring and metrics
- Integration test: All failure modes

## File Structure

```
daemons/ai-provider-daemon/
├── shared/
│   ├── AIProviderDaemon.ts       (existing)
│   ├── AIProviderTypes.ts        (existing)
│   ├── OllamaAdapter.ts          (existing)
│   └── genome/
│       ├── GenomeLoader.ts       (NEW - main API)
│       ├── GenomeCache.ts        (NEW - LRU cache)
│       └── GenomeTypes.ts        (NEW - types)
└── server/
    ├── AIProviderDaemonServer.ts (existing)
    └── workers/
        ├── ModelWorker.ts        (NEW - worker script)
        └── WorkerPool.ts         (NEW - pool manager)

tests/integration/
├── genome-loader.test.ts         (NEW - basic tests)
├── genome-worker-crash.test.ts   (NEW - recovery tests)
├── genome-memory.test.ts         (NEW - memory tests)
└── genome-concurrency.test.ts    (NEW - race condition tests)
```

## Success Criteria

- ✅ All integration tests pass
- ✅ No memory leaks (run 1000 load/unload cycles)
- ✅ Worker crash recovery works (kill workers randomly)
- ✅ Performance: Load genome in < 2s average
- ✅ Memory: Support 5+ genomes concurrently (3GB total)

## Non-Goals (Future Phases)

- ❌ Dynamic LoRA training (Phase 3)
- ❌ Multi-GPU support (Phase 4)
- ❌ Distributed genome loading (Phase 5)
- ❌ Genome versioning/checkpoints (Phase 3)

## References

- Node.js Worker Threads: https://nodejs.org/api/worker_threads.html
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
- LoRA paper: https://arxiv.org/abs/2106.09685
