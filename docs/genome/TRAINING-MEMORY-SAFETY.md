# Training Memory Safety Architecture

## Problem

Training processes (PEFT/QLoRA fine-tuning) are rogue memory consumers on Apple Silicon:
- VRAM IS system RAM on unified memory architecture
- A 3B QLoRA session consumes 4-8GB (model + optimizer states + gradients)
- Training subprocesses are invisible to Continuum's GPU/memory management
- No monitoring, no kill switch, no pressure response
- Result: machine freeze, OOM, forced reboot

## Solution: TrainingMemoryGuard

Every training process goes through a lifecycle that makes it a first-class citizen of the memory system.

### Lifecycle

```
1. PREFLIGHT  →  Fresh system/memory IPC check (not cached)
                 Estimate training footprint from model size
                 Refuse if available < estimated + 2GB safety margin

2. REGISTER   →  gpu/register-consumer IPC
                 Visible in eviction registry (./jtag gpu/eviction-registry)
                 Accounted in GPU pressure calculation
                 Priority: Batch (lowest — yields first)

3. MONITOR    →  Poll system/memory every 5s during training
                 <1GB available → SIGTERM training process → cleanup
                 <2GB available → warn via training:memory:warning event
                 Events: training:memory:critical, training:memory:warning

4. EXECUTE    →  Training runs with full visibility

5. CLEANUP    →  gpu/unregister-consumer IPC (guaranteed via try/finally)
                 Release accounted memory from pressure calculation
                 Stop monitoring timer
```

### Memory Estimates (QLoRA 4-bit)

| Model | Training Memory |
|-------|----------------|
| SmolLM2-135M | ~0.5 GB |
| SmolLM2-1.7B | ~3.0 GB |
| Llama-3.2-1B | ~2.5 GB |
| Llama-3.2-3B | ~6.0 GB |
| Qwen3-4B | ~8.0 GB |
| Phi-4-mini | ~7.0 GB |
| Unknown | ~6.0 GB (conservative) |

### IPC Commands (New)

```bash
# Register external consumer (training subprocess)
./jtag gpu/register-consumer --id="training:helper:coding" --label="Training: Helper / coding" --bytes=6442450944 --priority="batch"

# Unregister on completion
./jtag gpu/unregister-consumer --id="training:helper:coding" --bytes=6442450944

# View all registered consumers (training now visible here)
./jtag gpu/eviction-registry
```

### Key Files

| File | Purpose |
|------|---------|
| `system/genome/fine-tuning/server/TrainingMemoryGuard.ts` | Lifecycle manager |
| `commands/genome/train/server/GenomeTrainServerCommand.ts` | Uses guard for sync and async training |
| `workers/continuum-core/src/modules/gpu.rs` | gpu/register-consumer, gpu/unregister-consumer |
| `workers/continuum-core/src/gpu/memory_manager.rs` | account_external() method |
| `workers/continuum-core/bindings/modules/gpu.ts` | TS mixin methods |

### Training Timeout Scaling

Training timeout now scales with dataset size instead of fixed 600s:
```
timeout = max(600, 300 + examples * epochs * 15)
```

Pipeline timeout also accounts for post-benchmark training:
```
timeout = max(1800, 600 + challenges * 60 + challenges * epochs * 15)
```

### Python Unbuffered Output

`train-wrapper.sh` now runs Python with `-u` and `PYTHONUNBUFFERED=1` so sentinel captures output in real-time. Without this, stdout is buffered and lost when the process is killed by timeout.

### Future Work

- Actual PID tracking: sentinel should report child PID for targeted kill
- Memory profiling: measure actual vs estimated usage to refine estimates
- Adaptive training: reduce batch size or enable gradient checkpointing under memory pressure
- Multi-training coordination: only one training at a time, or share budget
