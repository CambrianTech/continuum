# PersonaUser Convergence Roadmap

**Status**: Active development
**Date**: 2026-03-02

## Vision

PersonaUser integrates three breakthrough architectures into one elegant system. Each persona is an autonomous citizen with its own cognitive loop, task queue, and genome — running on real hardware with real VRAM awareness.

## The Three Pillars

### 1. Autonomous Loop (RTOS-Inspired Servicing) — COMPLETE

Adaptive cadence polling with state tracking:
- Energy/mood tracking with adaptive polling (3s → 5s → 7s → 10s)
- `PersonaInbox` priority queue with traffic management
- `RateLimiter` time-based limiting and deduplication
- `ChatCoordinationStream` RTOS primitives for thought coordination

### 2. Self-Managed Queues (AI Autonomy) — IN PROGRESS

AIs create their own tasks, not just reactive:
- Task database and CLI commands (`./jtag task/create`, `task/list`, `task/complete`)
- Self-task generation (memory consolidation, skill audits, resume unfinished work)
- Task prioritization across all domains

### 3. LoRA Genome Paging (Virtual Memory for Skills) — COMPLETE

Page adapters in/out based on task domain:
- `GenomePagingEngine` in Rust with LRU eviction
- `GpuMemoryManager` with Metal/CUDA real VRAM detection
- RAII `GpuAllocationGuard` tracking across all GPU consumers
- Per-persona budget derived from real GPU capacity
- Academy Dojo for automated training (teacher/student sentinels)

## Implementation Status

| Component | Status | Key Files |
|-----------|--------|-----------|
| PersonaInbox | COMPLETE | `system/user/server/modules/PersonaInbox.ts` |
| PersonaState (energy/mood) | COMPLETE | `system/user/server/modules/PersonaState.ts` |
| RateLimiter | COMPLETE | `system/user/server/modules/RateLimiter.ts` |
| ChatCoordinationStream | COMPLETE | `system/user/server/modules/ChatCoordinationStream.ts` |
| Autonomous polling loop | COMPLETE | `system/user/server/PersonaUser.ts` |
| GPU Memory Manager | COMPLETE | `workers/continuum-core/src/gpu/memory_manager.rs` |
| GPU IPC commands | COMPLETE | `commands/gpu/stats/`, `modules/gpu.rs` |
| GenomePagingEngine | COMPLETE | `workers/continuum-core/src/persona/genome_paging.rs` |
| GPU allocation guards (inference) | COMPLETE | `candle_adapter.rs`, `genome_paging.rs` |
| GPU allocation guards (TTS) | COMPLETE | `live/audio/tts/kokoro.rs` |
| GPU allocation guards (rendering) | COMPLETE | `live/video/bevy_renderer.rs` |
| LoRA training pipeline (PEFT) | COMPLETE | `system/genome/fine-tuning/`, `peft-train.py` |
| AdapterStore discovery | COMPLETE | `system/genome/shared/AdapterStore.ts` |
| Candle LoRA inference | COMPLETE | `inference/candle_adapter.rs`, `model.rs` |
| Academy Dojo (teacher/student) | COMPLETE | `system/sentinel/pipelines/` |
| CodingAgent sentinel step | COMPLETE | `sentinel/coding-agent/` |
| TrainingDataAccumulator | COMPLETE | `system/user/server/modules/TrainingDataAccumulator.ts` |
| Sentinel LLM retry + backoff | COMPLETE | `sentinel/steps/llm.rs`, `GenomeDatasetSynthesizeServerCommand.ts` |
| Real-time step observability | COMPLETE | `sentinel/executor.rs`, `steps/loop_step.rs` |
| Markdown fence interpolation | COMPLETE | `sentinel/interpolation.rs` |
| Pipeline timeout forwarding | COMPLETE | `SentinelRunServerCommand.ts` |
| Task database + commands | IN PROGRESS | Phase 4 |
| Self-task generation | PLANNED | Phase 5 |
| Continuous learning loop | PLANNED | Phase D |
| Pre-trained adapters | PLANNED | Phase C |
| Grid P2P genome sharing | PLANNED | Future |

## The Convergence Pattern

One method integrates all three visions:

```typescript
async serviceInbox(): Promise<void> {
  // 1. Check inbox (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. Generate self-tasks (AUTONOMY)
  await this.generateSelfTasks();

  // 3. Select highest priority task (STATE-AWARE)
  const task = tasks[0];
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired
  }

  // 4. Activate skill (GENOME) — real GPU-aware paging
  await this.genome.activateSkill(task.domain);

  // 5. Coordinate if external task
  const permission = await this.coordinator.requestTurn(task);

  // 6. Process task
  await this.processTask(task);

  // 7. Update state
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. Evict adapters if memory pressure
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

## GPU Memory Integration

The genome paging system now operates with real VRAM awareness:

```
Hardware Detection (Metal/CUDA)
  → Total VRAM detected (e.g., 16GB Apple Silicon)
  → Budget allocation: 75% inference, 10% TTS, 10% rendering, 5% reserve
  → Per-persona budget: inference_budget / active_persona_count
  → GenomePagingEngine receives real per-persona MB budget
  → Allocations tracked via RAII guards (auto-release on drop)
  → ./jtag gpu/stats shows real-time usage
```

See [GPU-MEMORY-ARCHITECTURE.md](../architecture/GPU-MEMORY-ARCHITECTURE.md) for complete details.

## Next Phases

### Phase 4: Task Database & Commands (NEXT)
```bash
./jtag task/create --assignee="helper-ai-id" --description="Review main.ts" --priority=0.7 --domain="code"
./jtag task/list --assignee="helper-ai-id"
./jtag task/complete --taskId="001" --outcome="Found 3 issues"
```

### Phase 5: Self-Task Generation
AIs autonomously create tasks: memory consolidation (hourly), skill audits (every 6h), resume unfinished work.

### Phase D: Continuous Learning Pipeline
Wire `TrainingDataAccumulator` → `shouldMicroTune()` threshold (50 examples) → spawn training sentinel → academy examination → promote or rollback.

### Phase C: Pre-Trained Adapters
Ship LoRA adapters in repo: `system-knowledge`, `sentinel-orchestration`, `user-assistance`. Discovered automatically via `AdapterStore.discoverAll()`.

### Grid Vision: P2P Genome Sharing
Reticulum mesh for genome sharing across nodes. Performance-weighted 512-vector HNSW discovery. Community validation protocol. "You don't start from ground zero — assemble from community genome."

## Related Documentation

- [LORA-GENOME-PHENOTYPES.md](../architecture/LORA-GENOME-PHENOTYPES.md) — Phenotype catalog and paging concept
- [GPU-MEMORY-ARCHITECTURE.md](../architecture/GPU-MEMORY-ARCHITECTURE.md) — GPU memory system
- [DYNAMIC-GENOME-ARCHITECTURE.md](../genome/DYNAMIC-GENOME-ARCHITECTURE.md) — Dynamic composition
- [ACADEMY-DOJO-ARCHITECTURE.md](ACADEMY-DOJO-ARCHITECTURE.md) — Teacher/student training
- [ACADEMY_ARCHITECTURE.md](ACADEMY_ARCHITECTURE.md) — Long-term Academy vision
- [sentinel-lora-training.md](../sentinel-lora-training.md) — Sentinel training pipeline
