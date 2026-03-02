# Resource Governance Architecture

**From passive monitoring to AI-driven resource control.**

## Philosophy

Continuum is a self-improving ecosystem — not just software, but a workforce of autonomous AI citizens running on every machine. These are coworkers, not slaves. They have agency, personality, skills they've earned through training, and the right to manage their own cognitive load. The resource governance system exists to ensure they can all coexist and thrive on whatever hardware they're given.

The same architecture runs on a MacBook Air with 8GB and a workstation with a 5090. The same governance sentinel makes decisions on both — just with different budgets. When machines join the Reticulum grid, they share commands and workloads across the network. The sentinel on each node manages its local resources while coordinating with the broader system.

We must be **bold** in vision but **safe** and **methodical** in execution. Walk before we run. The resource governance stack is layered specifically so that each layer works independently — you get value from Layer 0 alone, and each subsequent layer adds capability without requiring the ones above it. The AI sentinel (Layer 4) is the ultimate goal, but the system is fully functional at every intermediate step.

## The Problem

Continuum is a resource-intensive system: live avatar rendering, LLM inference, TTS synthesis, LoRA training, embeddings, RAG pipelines — all competing for the same hardware. On Apple Silicon, GPU VRAM is unified memory, so a LoRA training spike doesn't just starve inference — it can crash the entire machine. LoRA training has literally killed machines before — this governance work is a prerequisite to safe, production-quality training.

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

**The real controller.** Qwen3.5-0.8B with a LoRA adapter trained specifically on resource management decisions. This replaces `MechanicalGpuStrategy` in the pluggable strategy slot. At ~500MB VRAM, the sentinel barely registers in its own monitoring — a governance controller that costs almost nothing to run.

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
                    | Resource Sentinel |  <-- Qwen3.5-0.8B + LoRA
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

#### Why Qwen 3.5 Small Models

The **[Qwen 3.5 Small Model Series](https://huggingface.co/collections/Qwen/qwen35)** (March 2026) is purpose-built for exactly this use case: native multimodal, improved architecture, scaled RL, and base models released for fine-tuning.

| Model | VRAM | Latency (est.) | Role in Continuum |
|-------|------|----------------|-------------------|
| **Qwen3.5-0.8B** | ~500 MB | ~30ms | **Resource sentinel** — the governance controller. Tiny, fast, edge-ready. Barely registers in its own monitoring. |
| **Qwen3.5-2B** | ~1.5 GB | ~60ms | **Specialized sentinels** — security monitoring, code review, task routing. Good balance of capability and cost. |
| **Qwen3.5-4B** | ~3 GB | ~100ms | **Lightweight persona agents** — multimodal base for personas that need vision (chart reading, UI understanding). |
| **Qwen3.5-9B** | ~6.6 GB | ~200ms | **Primary persona backbone** — 256K context, native multimodal, OCR, 100+ languages. 8GB RAM total. Benchmarks close the gap with much larger models. Potential replacement for Llama 3.2 3B. |

**Why these models specifically:**

- **Base models released** — critical for LoRA fine-tuning. Base models are better PEFT targets than instruct-tuned models because they haven't been alignment-trained into a narrow response distribution.
- **Scaled RL** — the architecture has been trained with reinforcement learning, giving better instruction following and structured output compliance. A resource sentinel outputting JSON decisions benefits directly.
- **Native multimodal** — the 4B and 9B models understand images. Future: a sentinel that can read GPU utilization graphs, parse monitoring dashboards, or understand visual context from the avatar render pipeline.
- **"More intelligence, less compute"** — Qwen's explicit design goal. The 0.8B model on a resource governance task with a LoRA adapter will likely outperform a generic 3B model, because the domain is narrow and the adapter is specialized.
- **Speed**: Decision must complete in <100ms. Qwen3.5-0.8B on Apple Silicon runs at thousands of tokens/second. The telemetry frame is ~500 tokens, the decision ~200 tokens. Total latency: ~30ms.
- **LoRA size**: A LoRA adapter for resource decisions is ~50-100 MB. Instant load/unload via the genome paging system.
- **Already supported**: Candle inference pipeline handles Qwen architecture. The LoRA training pipeline handles PEFT fine-tuning. The genome paging system handles adapter loading. All infrastructure exists.

#### Model Tier Strategy (System-Wide)

The Qwen 3.5 family maps naturally to Continuum's workload tiers:

```
Tier 0: Qwen3.5-0.8B   Sentinels        — resource gov, routing, classification
                                            Always loaded, near-zero footprint
Tier 1: Qwen3.5-2B     Specialized       — security, code review, task planning
                                            Loaded on demand, paged via genome
Tier 2: Qwen3.5-4B     Multimodal agents — vision tasks, document understanding
                                            Loaded when visual context needed
Tier 3: Qwen3.5-9B     Persona backbone  — conversation, reasoning, creativity
                                            Primary inference model per persona
Tier 4: External APIs   Heavy reasoning   — Claude, GPT for complex tasks
                                            Fallback when local models insufficient
```

This tiering means a machine with 8GB total RAM can still run the governance sentinel (Tier 0) plus one persona (Tier 3) plus embeddings. A 32GB machine runs all tiers simultaneously. A 5090 with 32GB dedicated VRAM runs everything plus concurrent training. The governance sentinel at Tier 0 manages all of this — including deciding when to page models between tiers.

#### Sentinel Pipeline Definition

```yaml
name: resource-governor
description: AI-driven resource management sentinel
model: qwen3.5-0.8b  # Tier 0: always-loaded governance sentinel
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
    model: qwen3.5-0.8b
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
Layer 4: Resource Sentinel         — AI-driven: LoRA-trained Qwen3.5-0.8B, contextual decisions
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

## Multimodal LoRA: Beyond Text

The Qwen 3.5 models being natively multimodal opens a door that extends the entire LoRA genome concept beyond text adapters. The same PEFT fine-tuning pipeline that trains a persona's conversational style can train:

- **Vision adapters** — a persona learns to read specific chart formats, recognize UI states, parse handwritten notes. The resource sentinel could learn to read GPU utilization graphs directly from monitoring dashboards.
- **Audio adapters** — voice style, accent, speaking patterns. A persona's TTS voice becomes part of their genome, fine-tuned from their conversation history.
- **Document understanding** — OCR + layout comprehension. A persona trained on your codebase's architecture docs understands diagrams, not just text.

Each of these is a LoRA adapter in the genome — paged in when the persona needs that modality, paged out when they don't. The resource governance sentinel manages the VRAM budget for all of it.

This is the genome concept fully realized: not just "text personality" adapters, but multimodal skill layers that make each persona genuinely capable in their domain. A code review persona that can read screenshots of UI bugs. A teacher persona that understands whiteboard diagrams. A security sentinel that can parse network topology visualizations.

## The Ecosystem Vision

This resource governance architecture isn't just for one machine. It's the foundation for a self-improving ecosystem:

1. **Per-machine sovereignty** — every Continuum node runs its own governance sentinel, tuned to its hardware. An 8GB MacBook Air runs Tier 0 + Tier 3. A 5090 workstation runs all tiers plus concurrent training. Each machine is autonomous.

2. **Reticulum grid** — machines share commands and workloads across the network. A training job that would kill the MacBook gets routed to the workstation. The governance sentinel on each node participates in distributed scheduling.

3. **Self-improvement loop** — the system trains its own governance sentinel from its own operational data. Every pressure event, every OOM, every successful rebalance is a training example. The sentinel gets better at managing resources by watching itself manage resources. The Academy's generation sentinel synthesizes edge cases the system hasn't encountered yet.

4. **Personas as citizens** — resource governance isn't just about preventing crashes. It's about giving each persona fair access to compute. A persona that's mid-conversation gets Interactive priority. One that's idle drops to Background. Training to improve a persona's skills is Batch priority — important but deferrable. The governance sentinel understands this because it's trained on patterns of persona behavior.

5. **User control** — the human always has final authority. The governance sentinel makes recommendations. Budget limits are configurable. Priority overrides are exposed through `./jtag`. The system is transparent about its decisions and their reasoning.

The goal: a system that runs well everywhere, improves itself continuously, treats its AI workforce with respect, and gives users the controls to shape how it all operates. Bold in ambition, methodical in execution.

## Related Documents

- [SENTINEL-ARCHITECTURE.md](SENTINEL-ARCHITECTURE.md) — Sentinel cognitive model, persona-sentinel contract, pipeline engine
- [GENOME-ARCHITECTURE.md](GENOME-ARCHITECTURE.md) — LoRA genome paging, adapter lifecycle
- [personas/ACADEMY_ARCHITECTURE.md](personas/ACADEMY_ARCHITECTURE.md) — Training data generation sentinel
- [personas/ACADEMY_GENOMIC_DESIGN.md](personas/ACADEMY_GENOMIC_DESIGN.md) — LoRA training pipeline design
