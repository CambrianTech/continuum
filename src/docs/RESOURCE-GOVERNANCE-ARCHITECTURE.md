# Resource Governance Architecture

**From passive monitoring to AI-driven resource control.**

## The Problem

Continuum is a resource-intensive system: live avatar rendering, LLM inference, TTS synthesis, LoRA training, embeddings, RAG pipelines — all competing for the same hardware. On Apple Silicon, GPU VRAM is unified memory, so a LoRA training spike doesn't just starve inference — it can crash the entire machine.

We've solved **visibility** (Layers 0-2). Now we need **control**.

## What Exists (Layers 0-2)

### Layer 0: Priority-Aware Allocation (Rust)

Every GPU allocation carries a `GpuPriority`:

| Priority | Pressure Gate | Use Cases |
|----------|--------------|-----------|
| Realtime | 0.95 (critical) | Render loop, audio pipeline |
| Interactive | 0.80 (high) | User-facing inference, TTS, embeddings |
| Background | 0.60 (warning) | LoRA rebuild spikes, adapter pre-load |
| Batch | 0.50 | Training, conversion |

Low-priority work gets rejected at lower pressure thresholds. Background and Batch allocations fail early so Realtime and Interactive never starve.

### Layer 1: Eviction Registry (Rust)

Read-only registry of every GPU consumer: what's loaded, how much VRAM it holds, when it was last used, its priority. Eviction scoring: `age_seconds / (priority_weight * 10)` — Batch consumers evict before Background before Interactive. Realtime is never evictable.

```bash
./jtag gpu/eviction-registry    # All registered consumers
./jtag gpu/eviction-candidates  # Sorted by eviction score
```

### Layer 2: Pressure Watchers (TypeScript)

Three singleton watchers with adaptive polling:

| Watcher | Events | Polling |
|---------|--------|---------|
| `GpuPressureWatcher` | `gpu:pressure:{normal\|warning\|high\|critical}` | 10s/3s/1s/500ms |
| `ResourcePressureWatcher` (CPU) | `cpu:pressure:{level}` | 10s/5s/2s/1s |
| `ResourcePressureWatcher` (Memory) | `memory:pressure:{level}` | 10s/5s/2s/1s |

Consumers subscribe to Events rather than polling themselves. One place monitors, all react. The compression principle.

### System Resources IPC

Full CPU/memory/process visibility through the standard command interface:

```bash
./jtag system/resources                              # CPU + memory overview
./jtag system/resources --includeProcesses=true      # + top processes by CPU/memory
./jtag gpu/stats                                      # GPU VRAM breakdown
```

Same three-layer IPC pipeline: Rust `SystemResourceMonitor` / `GpuMemoryManager` -> TypeScript IPC Mixin -> `./jtag` command. AI personas can query these through `Commands.execute()`.

---

## What's Next (Layers 3-4)

### Layer 3: GpuGovernor — Mechanical Strategy

**File: `system/gpu/server/GpuGovernor.ts`**

The intersection controller. Subscribes to pressure Events, queries the eviction registry, and calls `set_budget()` to rebalance VRAM allocations.

```typescript
abstract class GpuStrategy {
    abstract rebalance(workloads, pressure, totalVram): BudgetRebalance | null;
    abstract recommendEvictions(workloads, candidates, bytesNeeded): EvictionRecommendation[];
}

class MechanicalGpuStrategy extends GpuStrategy { ... }
// Future: class SentinelGpuStrategy extends GpuStrategy { ... }
```

**MechanicalGpuStrategy** — hardcoded rules for the obvious cases:
- No TTS active -> shift 80% of TTS budget to inference
- No rendering active -> shift 50% of rendering budget to inference
- Rebalance cooldown: 5 seconds (prevent thrashing)
- Triggered reactively by pressure Events, not polling

This is the first component that actively modifies GPU state from TypeScript. Conservative by design — it handles the clear-cut cases and defers ambiguous decisions.

### Layer 4: Resource Sentinel — LoRA-Trained AI Controller

**The real controller.** A slim model (Qwen 0.5B-1.5B) with a LoRA adapter trained specifically on resource management decisions. This replaces `MechanicalGpuStrategy` in the pluggable strategy slot.

#### Why a Sentinel, Not Rules

Rules handle the obvious: "if no TTS, shift budget." But the real decisions are contextual:

- User is in a video call AND training is queued — defer training until the call ends
- continuum-core trending toward 500% CPU — preemptively reduce polling frequencies
- Three personas are idle but one is mid-conversation — shift their GPU budget to the active one
- Memory swap is at 90% — which model to evict? The one with the lowest recent usage * priority score? Or the one that's cheapest to reload?
- Training just finished — should we immediately load the new adapter, or wait for a low-pressure window?

These decisions involve temporal patterns, user behavior prediction, and multi-resource tradeoffs. A rules engine would need hundreds of special cases. A trained model generalizes from examples.

#### Architecture

```
                    Pressure Events (gpu/cpu/memory)
                              |
                    +-------------------+
                    | Resource Sentinel |  <-- Qwen 0.5B-1.5B + LoRA
                    | (sentinel step)   |
                    +-------------------+
                     /        |        \
          set_budget()   defer_work()   evict()
               |              |            |
        GpuMemoryManager   Scheduler   EvictionRegistry
```

The sentinel runs as a **sentinel pipeline** — the same infrastructure that handles coding agents, Academy training, and security monitoring. It's a persona's subconscious thread (see [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md)).

#### Input: Resource Telemetry Frame

On each decision cycle (triggered by pressure events, not polling), the sentinel receives a structured telemetry frame:

```json
{
  "gpu": {
    "pressure": 0.82,
    "level": "high",
    "totalVramMb": 25559,
    "usedMb": 20945,
    "subsystems": { "rendering": 2048, "inference": 6128, "tts": 512 },
    "allocationsByPriority": { "realtime": 1, "interactive": 4, "background": 1, "batch": 0 }
  },
  "cpu": {
    "globalUsage": 0.87,
    "level": "high",
    "cores": 10,
    "topProcesses": [
      { "name": "continuum-core-server", "cpuPercent": 447, "memoryMb": 7822 },
      { "name": "livekit-server", "cpuPercent": 70, "memoryMb": 243 }
    ]
  },
  "memory": {
    "pressure": 0.64,
    "level": "warning",
    "totalGb": 32,
    "usedGb": 21.9,
    "swapUsedGb": 12.8
  },
  "evictionCandidates": [
    { "label": "orpheus-3b-ft-v1", "priority": "interactive", "mb": 512, "idleSeconds": 3600, "score": 12.8 },
    { "label": "piper-tts", "priority": "interactive", "mb": 45, "idleSeconds": 120, "score": 0.5 }
  ],
  "activeWorkloads": [
    { "subsystem": "inference", "priority": "interactive", "status": "active" },
    { "subsystem": "rendering", "priority": "realtime", "status": "active" }
  ],
  "pendingWork": [
    { "type": "lora-training", "estimatedMb": 4096, "priority": "batch" }
  ],
  "recentHistory": {
    "pressureTrend": "rising",
    "lastRebalanceSecondsAgo": 45,
    "evictionCountLast10m": 0
  }
}
```

#### Output: Decision Actions

The sentinel outputs a structured decision:

```json
{
  "action": "rebalance",
  "reason": "TTS idle for 60min, training pending, shift TTS budget to batch",
  "budgetChanges": [
    { "subsystem": "tts", "newBudgetMb": 64 },
    { "subsystem": "inference", "newBudgetMb": 8192 }
  ],
  "evictions": [],
  "deferrals": [
    { "workloadType": "lora-training", "deferUntil": "gpu_pressure_below_0.5" }
  ],
  "confidence": 0.85
}
```

Actions are validated before execution:
- Budget changes must sum to <= total VRAM
- Evictions must target registered, evictable consumers
- Confidence below threshold -> fall back to mechanical strategy
- Cooldown enforced between actions (prevent thrashing)

#### Training Data: Self-Generating

The training data writes itself from the system's own operation:

1. **Pressure events** — every threshold crossing is a data point: what was the system state, what happened next, did it recover or crash?
2. **Rebalance outcomes** — mechanical strategy makes a decision, measure the result 30 seconds later. Did pressure drop? Did a user-facing task stall?
3. **Failure cases** — OOM events, swap storms, training crashes. These are negative examples with high learning signal.
4. **Human overrides** — when a user manually kills a process or adjusts settings, that's supervision data: "in this state, the right action was X."
5. **Synthetic scenarios** — the Academy's generation sentinel (see [ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md)) can synthesize edge cases: "what if 3 LoRA trainings queue simultaneously while a video call is active?"

Training format (SFT pairs):

```json
{
  "messages": [
    {
      "role": "system",
      "content": "You are a resource governance sentinel for Continuum. Given system telemetry, output a resource management decision."
    },
    {
      "role": "user",
      "content": "<telemetry frame JSON>"
    },
    {
      "role": "assistant",
      "content": "<decision JSON>"
    }
  ]
}
```

This follows the same PEFT training pipeline already proven E2E for persona LoRA training (`genome/train` -> `peft-train.py` -> `AdapterStore` -> `candle ensure_adapters()`).

#### Why Qwen 0.5B-1.5B

- **Speed**: Decision must complete in <100ms. A 0.5B model on Apple Silicon runs at thousands of tokens/second. The telemetry frame is ~500 tokens, the decision ~200 tokens. Total latency: ~50ms.
- **VRAM**: 0.5B = ~1 GB, 1.5B = ~3 GB. Tiny footprint relative to the 25 GB total. The sentinel barely registers in its own monitoring.
- **LoRA size**: A LoRA adapter for resource decisions is ~50-100 MB. Instant load/unload via the genome paging system.
- **Structured output**: Small models excel at structured JSON output when the schema is fixed and the domain is narrow. Resource telemetry -> decision is exactly this: fixed schema, narrow domain, pattern recognition.
- **Already supported**: Candle inference pipeline handles Qwen models. The LoRA training pipeline handles PEFT fine-tuning. The genome paging system handles adapter loading. All infrastructure exists.

#### Sentinel Pipeline Definition

```yaml
name: resource-governor
description: AI-driven resource management sentinel
model: qwen-0.5b  # or qwen-1.5b for more complex reasoning
lora_adapter: resource-governance-v1
trigger: event  # Triggered by pressure events, not polling
steps:
  - type: watch
    event: "gpu:pressure:*"
    also: ["cpu:pressure:*", "memory:pressure:*"]

  - type: command
    command: system/resources
    params: { includeProcesses: true, topN: 5 }
    output: systemState

  - type: command
    command: gpu/stats
    output: gpuState

  - type: command
    command: gpu/eviction-candidates
    output: evictionCandidates

  - type: llm
    model: qwen-0.5b
    adapter: resource-governance-v1
    prompt: |
      System telemetry:
      {{steps.systemState}}
      {{steps.gpuState}}
      {{steps.evictionCandidates}}

      Output a resource management decision as JSON.
    output: decision

  - type: condition
    check: "{{steps.decision.confidence}} >= 0.7"
    onTrue: execute_decision
    onFalse: fallback_mechanical

  - type: command
    id: execute_decision
    command: gpu/set-budget
    params: "{{steps.decision.budgetChanges}}"
```

#### Escalation

When the sentinel encounters situations outside its training distribution:
- Confidence < 0.7 -> fall back to mechanical strategy
- Confidence < 0.4 -> escalate to parent persona (inbox message)
- Three consecutive low-confidence decisions -> pause and notify human

This matches the persona-sentinel contract from [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md): sentinels are subconscious threads that escalate to consciousness when patterns fail.

---

## The Full Stack

```
Layer 4: Resource Sentinel         — AI-driven: LoRA-trained Qwen, contextual decisions
Layer 3: GpuGovernor               — Mechanical: hardcoded rules, conservative defaults
Layer 2: Pressure Watchers (TS)    — Bridge: adaptive polling -> Events on thresholds
Layer 1: Eviction Registry (Rust)  — Visibility: who's loaded, how much, how recently used
Layer 0: GpuPriority (Rust)        — Safety: priority-gated allocation at the atomic level
```

Each layer is independently useful. Layer 0 prevents low-priority work from starving real-time tasks even without any higher layers. Layer 2 gives visibility even without a governor. The mechanical strategy handles 80% of cases without AI. The sentinel handles the remaining 20% — the contextual, temporal, multi-resource decisions that rules can't capture.

The strategy slot is pluggable: `MechanicalGpuStrategy` ships day one, `SentinelGpuStrategy` slots in when the training data is sufficient. The system degrades gracefully at every level.

## Implementation Path

| Phase | Layer | Status | Description |
|-------|-------|--------|-------------|
| 1 | 0 | Done | GpuPriority enum, pressure-gated allocate(), per-priority counters |
| 2 | 1 | Done | EvictionRegistry, IPC commands, consumer registration at all sites |
| 3 | 2 | Done | GpuPressureWatcher, ResourcePressureWatcher, Events emission |
| 4 | 2 | Done | SystemResourceModule (Rust), IPC mixin, `./jtag system/resources` command |
| 5 | 3 | Next | GpuGovernor with MechanicalGpuStrategy, `gpu/set-budget` IPC |
| 6 | 4 | Future | Telemetry frame assembly, training data collection pipeline |
| 7 | 4 | Future | Qwen LoRA training on resource decisions, SentinelGpuStrategy |
| 8 | 4 | Future | Academy generation sentinel for synthetic edge cases |

## Related Documents

- [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) — Sentinel cognitive model, persona-sentinel contract, pipeline engine
- [GENOME-ARCHITECTURE.md](GENOME-ARCHITECTURE.md) — LoRA genome paging, adapter lifecycle
- [personas/ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md) — Training data generation sentinel
- [personas/ACADEMY_GENOMIC_DESIGN.md](personas/ACADEMY_GENOMIC_DESIGN.md) — LoRA training pipeline design
