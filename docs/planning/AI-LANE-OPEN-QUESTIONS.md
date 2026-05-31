# AI Lane — Open Questions

> Explicit lane-by-lane punch list of design decisions we know we
> need but haven't made yet. Each entry: the question, why it
> matters, where it's blocking, candidate approaches. Living
> document; close items as decisions land.

**Status:** Open-question registry (2026-05-31).

**Parents:**
- [`docs/architecture/AI-COMMAND-NAMESPACE.md`](../architecture/AI-COMMAND-NAMESPACE.md) — the surface
- [`docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md`](../architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md) — the daemons behind it
- [`docs/architecture/EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md`](../architecture/EVERY-MODEL-INCLUDED-VIA-L1-BUDGET.md) — the inclusivity thesis

---

## Open questions, organized by lane

### Lane: Inference daemon (#109)

#### Q1 — Persona priority class signaling

How does a request indicate its pool class (realtime / interactive
/ background / sentinel)?

- **Matters because:** without this signal, the scheduler can't
  tier-pool, can't enforce latency targets, can't safely run
  background work alongside live conversation.
- **Blocks:** SlotPool design, RequestQueue ordering.
- **Candidates:**
  - Reuse the existing `purpose: Option<String>` field on
    `TextGenerationRequest`; introduce a vocabulary
    (`"realtime"`, `"interactive"`, `"background"`, `"sentinel"`)
  - Add `pool: Option<PoolClass>` to TextGenerationRequest
    explicitly
  - Set it on the persona record at registration; the daemon reads
    persona record by `persona_id` rather than per-request
  - Hybrid: persona record carries default, request can override
- **Decision needed by:** #109 implementation start.

#### Q2 — Continuous batching window — static or dynamic?

What's the right batch-admission window per pool class?

- **Matters because:** window directly trades latency for
  throughput. Too short → low GPU utilization. Too long → realtime
  pool overshoots its budget.
- **Blocks:** BatchAssembler implementation.
- **Candidates:**
  - Static per-pool (realtime=0ms, interactive=2ms, background=20ms)
  - Pressure-driven (more pressure → longer windows for throughput)
  - Adaptive PID-style controller targeting pool latency budgets
  - Hybrid: per-pool ceiling + adaptive within bounds
- **Decision needed by:** BatchAssembler scaffold.

#### Q3 — LoRA paging cost calibration

How does the scheduler know how expensive each adapter's swap is?

- **Matters because:** the scheduler must NOT schedule operations
  that exceed pool latency. Hot-path swap is the prior-attempt
  failure mode.
- **Blocks:** LoRAPager safety.
- **Candidates:**
  - Boot-time calibration pass: measure swap cost per adapter
  - Stored profile keyed by `(adapter_id, host_class)` — refresh
    on hardware change
  - Per-host-class lookup table for known adapter shapes
  - Dynamic learning: measure over time, EMA-track
- **Decision needed by:** LoRAPager implementation.

#### Q4 — Adaptive quantization tier selection

What algorithm picks INT4 vs INT8 vs FP16 per slot?

- **Matters because:** wrong tier under pressure either OOMs or
  ships unnecessarily degraded quality.
- **Blocks:** AdaptiveQuantizer.
- **Candidates:**
  - Static pool → tier mapping (realtime=INT8, background=INT4)
  - Continuous pressure-driven (VRAM utilization above X% → drop
    tier)
  - Per-persona preference
  - Hybrid: persona default + pressure override + pool floor
  (realtime never drops below INT8)
- **Decision needed by:** AdaptiveQuantizer scaffold.

#### Q5 — Speculative warming prediction model

How does the daemon predict which persona to pre-page?

- **Matters because:** wrong prediction wastes paging + may evict
  needed adapters. Right prediction hides paging latency.
- **Blocks:** SpeculativeWarmer.
- **Candidates:**
  - Rule-based (mention detection, recent active speaker,
    turn-taking pattern)
  - Learned per-room conversation-flow model
  - Hybrid: rules for high-confidence cases, learned for the rest
- **Decision needed by:** SpeculativeWarmer scaffold.

#### Q6 — Pressure signal source per host class

Where does the daemon read VRAM / RAM / GPU utilization on each
host class?

- **Matters because:** the substrate runs on macOS (Apple Silicon
  unified memory + Intel discrete), Linux NVIDIA, Linux AMD, Linux
  Intel — each has different APIs.
- **Blocks:** PressureMonitor.
- **Existing:** `SubstrateGovernor` (CBAR-SUBSTRATE-ARCHITECTURE.md)
  already polls some signals. Extend or replicate?
- **Candidates:**
  - SubstrateGovernor publishes a unified pressure bus event;
    InferenceScheduler subscribes
  - InferenceScheduler queries Governor on demand
  - Each adapter reports its own per-device pressure (NVML for
    NVIDIA-backed adapters, etc.) and the scheduler aggregates
- **Decision needed by:** PressureMonitor scaffold.

#### Q7 — Base-model sharing under model-swap pressure

When batches of conflicting models arrive (some need Qwen, some
need Llama), how does the scheduler decide which model to keep
warm?

- **Matters because:** if model swaps happen frequently, all the
  base-model-sharing benefits disappear.
- **Blocks:** BaseModelSharing.
- **Candidates:**
  - LRU on model bytes
  - Pinned model pool (most-used model always warm)
  - Pool-class-driven (realtime pool's model always wins)
  - Dynamic sizing based on request distribution over recent window
- **Decision needed by:** BaseModelSharing implementation.

---

### Lane: Cross-grid inference (#108)

#### Q8 — Peer discovery + capacity advertising

How does the local daemon discover which peers run which models warm
+ what capacity they have available?

- **Matters because:** the M5 → 5090 case requires the M5 to KNOW
  the 5090 is available and willing.
- **Blocks:** AircRemoteInferenceAdapter routing.
- **Candidates:**
  - Periodic capacity beacon over airc (publish current
    `inference/capacity` per peer)
  - On-demand probe (ask all peers "do you have X warm?")
  - Centralized scope-wide capacity registry
  - Hybrid: long-poll capacity stream + on-demand verification
- **Decision needed by:** AircRemoteInferenceAdapter scaffold.

#### Q9 — Persona identity projection on remote peer

Joel's persona "Paige" lives on her local Mac. When she opens an
inference handle on the 5090 over airc, what identity does she
have on the 5090?

- **Matters because:** persona scope checks in the handle store +
  in RAG sources require an identity. Cross-persona leakage is a
  defense-in-depth concern.
- **Blocks:** AircRemoteInferenceAdapter session shape.
- **Reference:** [[personas-are-citizens-airc-is-identity-provider]]
- **Candidates:**
  - Project Paige's airc peer_id directly (the substrate identity
    primitive already crosses machines)
  - Create a temporary "remote-session" persona scoped to this
    handle
  - The 5090 holds a "remote-proxy" persona on Paige's behalf, scoped
    by her peer_id
- **Decision needed by:** AircRemoteInferenceAdapter scaffold.

#### Q10 — Backpressure when grid is saturated too

What happens when local slots AND all reachable grid peers are
saturated?

- **Matters because:** the substrate must degrade gracefully, not
  hang.
- **Blocks:** RouteSelector backpressure path.
- **Candidates:**
  - Queue locally with extended wait + emit backpressure event
  - Return typed "no capacity" error; let the caller (persona /
    sentinel) decide
  - Fall back to heuristic adapter with a clear "degraded" flag in
    the response
- **Decision needed by:** RouteSelector design.

---

### Lane: ai/* namespace consolidation (#106)

#### Q11 — Migration path for existing `inference/*` + `embedding/*` callers

How do we move existing top-level command consumers under `ai/*`
without breaking them?

- **Matters because:** the namespace is the wrong shape today; we
  want to fix it without a flag day.
- **Blocks:** #106.
- **Candidates:**
  - Dual-route at the kernel: both old prefix + new prefix accepted
    for a deprecation window
  - Hard-fail rename: bump major + migrate all callers in one PR
  - Symlinks at the command registry level + log warnings
- **Decision needed by:** #106 start.

#### Q12 — Per-modality `ai/capacity` shape

`inference/capacity` returns one number today (LLM slots). The
unified `ai/capacity` should report vision / audio / embedding /
classical-ML caps separately. What's the wire shape?

- **Matters because:** callers can't reason about cross-modality
  scheduling without per-modality numbers.
- **Blocks:** scheduler cross-modality decisions, `ai/capacity`.
- **Candidates:**
  - Flat map: `{ "llm": 4, "vision": 2, "audio": 1, "embedding": 8 }`
  - Structured per-modality (slots, queued, average_latency_ms)
  - Per-pool-class subdivision within each modality
- **Decision needed by:** namespace consolidation.

---

### Lane: Observability (substrate-wide)

#### Q13 — Replay parity across modalities

`ReplayRagSource` exists today for RAG. Do `ReplayInferenceSource`,
`ReplayVisionSource`, `ReplayAudioSource` follow the same shape?

- **Matters because:** AIs running adversarial review need to
  replay an entire persona turn (RAG + prompt + inference + vision
  + …) deterministically.
- **Blocks:** full-turn replay.
- **Candidates:**
  - One replay shape per modality, each modeling its own source
    contract
  - Single unified `Replay<ModalitySource>` with parametric type
  - Composite `ReplayPersonaTurn` that drives multiple replay
    sources from a single JSONL trace
- **Decision needed by:** task #56 (wire persona turn capture).

#### Q14 — Schema versioning for capture traces

Today's JSONL traces have no schema version. As fields evolve, old
traces become unreplayable.

- **Matters because:** replay is a product requirement; broken
  replay defeats it.
- **Blocks:** long-term replay viability.
- **Candidates:**
  - `version` field on every capture event
  - Schema-tag at the trace head; rejector for incompatible
  - Migration adapters per version pair
- **Decision needed by:** when replay starts being used in CI.

#### Q15 — Capture sink composition

Today a sink is one of Noop / JSONL / InMemory. What about
multi-sink (JSONL + IPC publish + InMemory at the same time)?

- **Matters because:** a mechanic-shop session might want
  live-streamed IPC + on-disk trace + in-memory inspection
  concurrently.
- **Blocks:** richer mechanic-shop workflows.
- **Candidates:**
  - `BroadcastCaptureSink` wrapping a Vec<Arc<dyn CaptureSink>>
  - Sinks compose via a fan-out trait combinator
  - Single global capture bus that sinks subscribe to
- **Decision needed by:** when mechanic-shop work starts.

---

### Lane: Hardware + memory hierarchy

#### Q16 — Apple unified-memory accounting

On Apple Silicon (M1+), unified memory is shared between CPU and
GPU. How does the scheduler reason about this vs the discrete-VRAM
model?

- **Matters because:** the M5 target IS unified memory. Wrong
  accounting drives wrong eviction / batching decisions.
- **Blocks:** Apple-class scheduler tuning.
- **Existing:** `SubstrateGovernor` has some unified-memory
  awareness.
- **Candidates:**
  - Single pressure signal across CPU + GPU work
  - Separate accounting with explicit "shared pool" budget
  - Per-adapter declaration of UMA-vs-discrete behavior
- **Decision needed by:** Apple Silicon performance work.

#### Q17 — KV cache eviction policy across pool classes

When VRAM tightens, KV cache eviction is the highest-leverage
pressure relief. Which slots lose their cache first?

- **Matters because:** evicting an active realtime conversation's
  KV is a quality cliff.
- **Blocks:** PressureMonitor eviction policy.
- **Candidates:**
  - LRU across all slots
  - LRU within pool class; background pool always evicted first
  - Eviction never targets realtime pool (hard pin)
  - Hybrid: LRU within pool, pool-class priority for cross-pool
- **Decision needed by:** PressureMonitor design.

---

### Lane: Multi-modal pipelines

#### Q18 — Vision crutch latency budget

When a text-only LLM persona needs to "see" via
`ai/vision/describe` → text → RAG → `ai/inference/generate`, the
vision step adds latency. How does the persona's pool class flow
through?

- **Matters because:** if Paige's realtime turn needs vision and
  vision goes to the background pool, her turn missed its budget.
- **Blocks:** vision-as-crutch integration.
- **Candidates:**
  - Pool class propagates through the chain — vision call inherits
    Paige's realtime class
  - Vision has its own pool independent of caller
  - The orchestrating layer (PersonaCognition) explicitly threads
    pool class
- **Decision needed by:** vision-crutch implementation.

#### Q19 — Multi-modal model decomposition

Native multimodal Qwen (the M5 target) handles vision INSIDE the
LLM forward pass. CNN crutches handle vision via a separate
classifier model. When does the scheduler pick which?

- **Matters because:** native multimodal is higher quality but
  requires the multimodal model to be loaded; CNN crutch is more
  flexible but loses cross-modal reasoning.
- **Blocks:** multimodal vision strategy.
- **Candidates:**
  - Persona preference (some pin native, some pin crutch)
  - Capability check at handle open (multimodal model available
    → use native; else crutch)
  - Per-request hint via `purpose`
- **Decision needed by:** native-multimodal Qwen integration.

---

### Lane: Prior-attempt forensics

#### Q20 — Recover the prior attempt's actual failure logs

Joel's "we wrote about this and attempted the same thing with
adapters before. It was rather shitty" implies prior work + logs +
post-mortems that aren't currently in the docs.

- **Matters because:** repeating mistakes is unforgivable when the
  documentation exists. Even partial recovery of the failure modes
  saves implementation time on #109.
- **Blocks:** Confident scheduler design.
- **Candidates:**
  - Search git log + branch history for adapter-paging /
    multi-persona-inference / scheduler PRs
  - Check older `docs/inference/` + `docs/architecture/` revisions
  - Ask Joel for pointers
- **Decision needed by:** #109 implementation start.

---

## Triage

These are scoped roughly by what blocks what:

| Question | Blocks task | Priority |
|---|---|---|
| Q20 — prior-attempt forensics | #109 | Highest (don't repeat) |
| Q1 — persona priority class signaling | #109 SlotPool | High |
| Q11 — namespace migration path | #106 | High (consumer impact) |
| Q3 — LoRA paging cost calibration | #109 LoRAPager | High |
| Q6 — pressure signal source | #109 PressureMonitor | High |
| Q9 — persona identity on remote peer | #108 | High |
| Q8 — peer discovery + capacity | #108 | High |
| Q2 — batching window | #109 BatchAssembler | Medium |
| Q4 — quantization tier selection | #109 AdaptiveQuantizer | Medium |
| Q7 — base-model sharing under swap | #109 BaseModelSharing | Medium |
| Q12 — per-modality capacity shape | #106 | Medium |
| Q17 — KV cache eviction policy | #109 PressureMonitor | Medium |
| Q5 — speculative warming model | #109 SpeculativeWarmer | Lower |
| Q10 — grid-saturated backpressure | #108 RouteSelector | Lower |
| Q13 — replay parity across modalities | #56 | Lower |
| Q14 — schema versioning for traces | (long-term) | Lower |
| Q15 — capture sink composition | mechanic-shop work | Lower |
| Q16 — Apple unified-memory accounting | Apple performance | Lower |
| Q18 — vision crutch latency budget | vision integration | Lower |
| Q19 — multi-modal model decomposition | native Qwen | Lower |

---

## How to close an item

When a decision is made:

1. Update the question to **Resolved** with a one-paragraph summary.
2. Link to the PR / commit / doc where the decision lives.
3. Move closed items to an "Archive" section at the bottom; don't
   delete (the rationale stays useful for future reviewers).
4. If the decision invalidates a candidate elsewhere on this page,
   note it inline.

The goal: when #109 (or any of the blocked tasks) actually starts,
the implementer reads this doc once and knows which decisions are
made and which they're empowered to make.
