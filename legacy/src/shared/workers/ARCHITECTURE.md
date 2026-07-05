# PersonaUser Worker Thread Architecture

## Current Implementation (Phase 1: Per-Persona Workers)

Each PersonaUser spawns its own dedicated worker thread for message evaluation.

### Architecture
```
PersonaUser (Main Thread)
└── PersonaWorkerThread
    └── persona-worker.js (Worker Thread)
        └── OllamaAdapter (AI inference)
```

### Benefits
- **Strong Isolation**: Each persona has dedicated resources
- **Simple Management**: No coordination needed between personas
- **Easy Debugging**: One worker per persona, clear ownership
- **No Synchronization**: No mutex/semaphore overhead

### Tradeoffs
- **Higher Memory**: Each worker maintains separate memory space
- **Context Switching**: More threads = more OS scheduler overhead
- **Scalability Limit**: ~10 PersonaUsers before resource pressure

### Current Scale
- 3 PersonaUsers (Helper AI, Teacher AI, CodeReview AI)
- 3 worker threads (1:1 mapping)
- Memory footprint: ~3 * (worker + Ollama context) per persona

---

## Future Implementation (Phase 2: Worker Pool)

**Trigger**: When scaling beyond 10 PersonaUsers

### Pool Architecture (Generic Workers)
```
WorkerPool (Main Thread)
├── Worker 1 (generic, handles any persona)
├── Worker 2 (generic, handles any persona)
├── Worker 3 (generic, handles any persona)
├── Worker N (size = os.cpus() or 5-10)
└── Task Queue (FIFO or priority-based)

PersonaUser 1 ──┐
PersonaUser 2 ──┼──> WorkerPool.evaluate() ──> Available Worker
PersonaUser N ──┘
```

### Design Decisions

#### Pool Size
- **Start**: 5-10 workers (Helper AI recommendation)
- **Max**: `os.cpus().length` workers
- **Strategy**: Monitor metrics (throughput, latency) and scale dynamically

#### Worker Type: Generic vs Specialized

**Option A: Generic Workers** (RECOMMENDED)
```typescript
class WorkerPool {
  private workers: PersonaWorkerThread[];

  async evaluate(personaId: string, message: Message): Promise<EvalResult> {
    const worker = await this.getAvailableWorker();
    return worker.evaluateMessage(message, personaId);
  }
}
```

**Pros:**
- Simpler implementation
- Better load balancing (any worker handles any task)
- Efficient resource utilization
- Easy to scale pool size dynamically

**Cons:**
- No persona-specific optimizations
- No specialized context per persona type

---

**Option B: Specialized Workers**
```typescript
class WorkerPool {
  private codeReviewPool: PersonaWorkerThread[];
  private helperPool: PersonaWorkerThread[];
  private teacherPool: PersonaWorkerThread[];

  async evaluate(persona: PersonaUser, message: Message): Promise<EvalResult> {
    const pool = this.getPoolForPersona(persona.type);
    const worker = await pool.getAvailableWorker();
    return worker.evaluateMessage(message, persona.id);
  }
}
```

**Pros:**
- Specialized workers can cache persona-specific context
- Could use different models per pool (e.g., llama3.2:1b for helpers, 3b for code review)
- Better for future genome/LoRA integration (specialized adapters per pool)

**Cons:**
- More complex management (multiple pools)
- Worse load balancing (idle workers in one pool while another is busy)
- Harder to scale dynamically

---

### Recommended Strategy (From AI Team)

**CodeReview AI's Recommendation: Hybrid Approach**
> "A hybrid approach could be considered, where each worker thread has some degree of specialization
> but also remains flexible enough to handle tasks from other persona types in cases of high demand
> or availability."

**Implementation Path:**

**Phase 2A: Start with Generic Pool (10-50 PersonaUsers)**
- Simpler to implement and maintain
- Sufficient for initial scaling
- Easy migration from current per-persona architecture
- Workers can handle any PersonaUser evaluation

**Phase 2B: Hybrid Specialization (50+ PersonaUsers)**
- Primary pool assignment by persona type (CodeReview, Helper, Teacher)
- Workers can "steal" tasks from other pools when idle (work stealing)
- Benefits: Specialization + flexibility
- Example: CodeReview worker prefers CodeReview tasks but can handle Helper tasks if available

**Phase 2C: Full Specialization (100+ PersonaUsers with Genomes)**
- Separate pools per persona type
- Required when personas have distinct genome/LoRA adapters
- Performance profiling shows benefit from specialized context caching
- Dedicated resources per persona type

---

## Implementation Checklist (Phase 2)

### Core Pool Implementation
- [ ] `WorkerPool` class with generic worker management
- [ ] Task queue (FIFO or priority-based)
- [ ] Worker availability tracking (busy/idle state)
- [ ] Graceful worker recycling (prevent memory leaks)
- [ ] Pool size configuration (min/max workers)

### Integration with PersonaUser
- [ ] Replace `new PersonaWorkerThread()` with `WorkerPool.getInstance()`
- [ ] Update `evaluateMessage()` to use pool
- [ ] Maintain per-persona configuration (model, provider)
- [ ] Add shutdown hook to return worker to pool

### Performance Monitoring
- [ ] Track worker utilization (busy/idle ratio)
- [ ] Monitor task queue length
- [ ] Measure evaluation latency (pool vs per-persona)
- [ ] Memory footprint comparison
- [ ] Auto-scaling triggers (add/remove workers dynamically)

### Reliability
- [ ] Worker health checks (detect crashed workers)
- [ ] Automatic worker restart on failure
- [ ] Timeout handling (prevent hung workers)
- [ ] Graceful degradation (fallback to synchronous evaluation)

---

## Performance Targets (Phase 2)

| Metric | Per-Persona (Current) | Worker Pool (Target) |
|--------|----------------------|---------------------|
| PersonaUsers Supported | 3-10 | 50+ |
| Memory per Persona | ~50MB | ~10MB |
| Context Switch Overhead | High (3-10 threads) | Low (5-10 threads) |
| Evaluation Latency | 300-500ms | 350-550ms (+10-20% queue overhead) |
| Resource Utilization | Medium (dedicated threads) | High (shared pool) |

---

## AI Team Consultation Log

**Date**: 2025-10-14

**Question**: Memory leak risk with per-persona workers? Should we use worker pool?

**Helper AI**:
- Worker threads CAN introduce memory leaks if not managed properly
- Recommends worker pool for reusing workers across tasks
- Benefits: efficient resource allocation, load balancing, scalability
- Pool size: Match CPU cores or start with 5-10 workers

**CodeReview AI**:
- Generally NOT a risk since each task has own scope
- Important: Set `workerData` property correctly
- No shared resources = minimal leak risk

**CodeReview AI's Specialization Advice**:
- Specialized workers maintain performance and expertise
- Separate pools/queues per persona type (CodeReview, Helper, Teacher)
- Pros: Task-relevant worker handling, better isolation
- Cons: Additional complexity in pool management and synchronization
- **Recommends**: Hybrid approach - some specialization but flexible for high demand

**Decision**:
- ✅ Keep per-persona workers for current scale (3 PersonaUsers)
- ✅ Implement proper `shutdown()` (DONE - PersonaUser.ts:1399-1405)
- ✅ Document worker pool design for future (this file)
- 📋 Future: Implement generic pool when scaling beyond 10 PersonaUsers
- 📋 Future: Add hybrid specialization at 50+ PersonaUsers
- 📋 Future: Full specialization when genome/LoRA adapters deployed

---

## Related Files

- `PersonaWorkerThread.ts` - Main thread manager (current implementation)
- `persona-worker.js` - Worker thread code (evaluation logic)
- `PersonaUser.ts:107-113` - Worker initialization
- `PersonaUser.ts:1399-1405` - Worker shutdown (leak prevention)

---

## Future Considerations (Phase 3+)

### Genome/LoRA Integration
When personas have custom LoRA adapters:
- Workers need to load/unload adapters dynamically
- Specialized pools become more valuable (adapter caching)
- Consider hybrid: generic pool + specialized genome workers

### Distributed Workers
For multi-machine deployment:
- Worker pool could span multiple servers
- Use message queue (Redis, RabbitMQ) instead of in-memory queue
- Stateless workers for horizontal scaling

### GPU Workers
For larger models (llama 70b, mistral, etc):
- Separate GPU worker pool (limited by GPU memory)
- CPU pool for small models (llama3.2:1b, 3b)
- Intelligent routing based on model requirements
