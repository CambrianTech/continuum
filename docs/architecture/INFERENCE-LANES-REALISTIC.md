# Inference Lanes — The Realistic Design

> One base model. N persona lanes. Each lane is a (TaskKind, persona,
> ThroughputLease) triple sized via the existing recipe budget table.
> Continuous batching multiplexes them through the same model. No
> new model loads per persona. No tier-down. Composes the prior art
> the substrate already shipped.

**Status:** Design (2026-05-31). Builds on prior art already in tree.
Concrete next move for task #109. Companion to
[`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
which has the broader architecture; THIS doc is the focused realistic
build plan.

**Parents:**
- [`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
- [`PERSONA-CONTEXT-PAGING.md`](PERSONA-CONTEXT-PAGING.md)
- [`AI-COMMAND-NAMESPACE.md`](AI-COMMAND-NAMESPACE.md)

---

## The thesis stated plainly

Joel, 2026-05-31:

> "I think we weren't clever enough with our lanes. The goal should be
> to ideally cover the needs of the persona, while being realistic.
> On this machine it might be 3 low end, maybe cpu, without a grid
> inference provider which we plan to use. But this isn't an option
> for some. We should at least attempt something reasonable, a model
> with creative capacity."

The prior-attempt failure mode wasn't just scheduling — it was the
**conception of a lane**. A lane is not "a separate model load." A
lane is "a budgeted KV cache slot serving one persona at one
TaskKind, multiplexed through one shared base model via continuous
batching." Once that's the unit, hosting 16 personas on a single
local machine becomes tractable.

Three premises:

1. **ONE base model on the host.** The substrate picks the best one
   that fits + has creative capacity (not stupid). Same weights serve
   every persona on this host.
2. **N lanes, each a (persona, TaskKind) pair.** Each carries its own
   KV cache budget from the existing recipe_budget table (Chat=8K,
   VoiceChat=8K, GameNpcIdle=4K, etc.).
3. **Existing primitives compose into the scheduler.** Don't reinvent
   slots, memory accounting, eviction, or pressure response — those
   are already shipped.

---

## Prior art inventory (what's already built)

The substrate is much further along than the broader scheduling doc
implied. Mapping component → existing implementation:

| Substrate role | File | Status |
|---|---|---|
| Slot/lease primitive | `cognition/throughput_lease.rs::ThroughputLease` | Built |
| Lease registry | `cognition/throughput_lease.rs::ThroughputLeaseRegistry` | Built |
| Admission planner | `cognition/adaptive_throughput.rs::AdaptiveThroughputRequest` + `AdaptiveThroughputPlan` | Built |
| Per-resource memory accounting | `inference/footprint_registry/mod.rs::FootprintRegistry` | Built |
| Cheapest-eviction search | `FootprintRegistry::cheapest_eviction_for` | Built |
| Lease + footprint composition | `FootprintRegistry::acquire_lease` / `release_lease` | Built |
| 5-tier memory hierarchy | `genome/working_set.rs::TierRole` + `WorkingSet` | Built |
| Per-tier eviction policies | `genome/tier.rs::EvictionPolicy` (LruWithinTurn / LruAcrossTurns / LfuPlusRecency / DemandAlignedWithRefinedPreference / AppendOnlyGcOnSleep) | Built |
| UMA-aware tier shape | `TierRole::is_present_on_uma` | Built |
| Page-fault telemetry | `WorkingSet::PageFault` | Built |
| Pressure broker | `paging/broker.rs::PressureBroker` | Built |
| Pressure tiers (Normal/Warning/High/Critical) | `paging/broker.rs::PressureTier` | Built |
| Paged resource pool primitive | `paging/pool.rs::PagedResourcePool` | Built |
| TaskKind seed-budget table | `inference/recipe_budget.rs::TaskKind::default_seed_tokens` | Built (10 TaskKinds) |
| Hardware probe + tier classification | `governor/` | Built (per existing #44-#52 work) |
| InferenceHandleStore + handle commands | `inference/handle_store.rs` + `handle_module.rs` | Built (#107) |
| Adapter trait + registry + concrete impls | `ai/adapter.rs` + adapter files | Built |
| Heuristic / fake adapter | `ai/heuristic_adapter.rs` | Built (#103) |
| LlamaCpp adapter | `inference/llamacpp_adapter.rs` | Built |

What's NOT built yet — the **coordinator** that composes these
into actual continuous-batching multi-persona serving.

---

## What "clever lanes" actually means

A **lane** is the unit of inference budget. It has three fields:

```rust
pub struct Lane {
    pub persona: PersonaId,
    pub task: TaskKind,                 // Chat / VoiceChat / GameNpcIdle / ...
    pub lease: ThroughputLease,         // acquired via FootprintRegistry
}
```

The lane's KV budget comes from `task.default_seed_tokens()`. The
lease's `cost_units` reflects that budget plus the model's
per-sequence overhead. The lease's `revocation_policy` follows the
lane's pool class:

- **Realtime conversation (chat, voice, video while engaged)** →
  `ThroughputLeaseRevocationPolicy::Pinned` — pressure broker MUST
  NOT evict mid-turn
- **Interactive (idle chat, idle voice)** →
  `ThroughputLeaseRevocationPolicy::Graceful` — notify-then-evict
  acceptable
- **Background (reflection, sentinel review, training)** →
  `ThroughputLeaseRevocationPolicy::Hard` — evict immediately under
  pressure

The lease lives in the existing `ThroughputLeaseRegistry`. The KV
bytes get accounted in the existing `FootprintRegistry` via
`acquire_lease()`. The 5-tier memory hierarchy decides where those
bytes physically live (Fast for active conversation, Warm / Bench
for idle, Cold for evicted-but-resumable). PressureBroker drives
eviction when total memory tightens.

**Crucial: lanes do NOT load separate model weights.** Every lane on
the host shares the same loaded base model bytes (the existing
`Arc<dyn AIProviderAdapter>` in the handle store). What differs per
lane is the per-sequence KV cache + per-persona LoRA adapter stack.

---

## How the coordinator composes prior art

```
ai/inference/open(persona, task, ...)
  │
  ├─→ AdaptiveThroughputRequest { target_silicon, cost_units = task.cost_for_silicon(...) }
  │
  ├─→ AdaptiveThroughputPlan.admit_or_queue()
  │     ├─→ if admit: ThroughputLease minted
  │     └─→ if queue: lane parked in admission queue
  │
  ├─→ FootprintRegistry::acquire_lease(lease, lane_kv_bytes)
  │     └─→ on memory pressure: cheapest_eviction_for() picks a
  │         non-pinned lane to evict
  │
  ├─→ WorkingSet (persona-scoped) gets KV cache pages keyed by lane
  │
  └─→ InferenceHandleStore.open(adapter, ...) returns HandleRef

ai/inference/generate(handle, request)
  │
  ├─→ store.generate(handle, request)
  │     └─→ adapter.generate_text(request)
  │           └─→ (NEW) BatchAdmission: this request joins the next
  │                continuous-batching iteration of the local model;
  │                the model runs one forward pass that produces the
  │                next token for every active lane concurrently
  │
  └─→ response routed back via the handle

ai/inference/close(handle)
  │
  ├─→ FootprintRegistry::release_lease(lease_id)
  ├─→ WorkingSet pages eligible for tier-demotion
  └─→ InferenceHandleStore.close(handle)
```

The only piece that requires NEW code is the **BatchAdmission +
continuous-batching path inside the LlamaCpp adapter**. Everything
else is wiring existing primitives.

---

## Realistic baseline target

Joel's framing: "On this machine it might be 3 low end, maybe cpu,
without a grid inference provider which we plan to use. But this
isn't an option for some. We should at least attempt something
reasonable, a model with creative capacity."

For a baseline low-end host (CPU-only, no grid, modest RAM):

| Parameter | Target |
|---|---|
| Base model | Qwen-2.5-3B-Instruct or Gemma-2-2B or Llama-3.2-3B at Q4_K_M (1.5–2.2 GB on disk + RAM) |
| Quantization tier | Q4_K_M (creative capacity preserved; not "stupid model") |
| Active lanes | 3 — typical case: 1× VoiceChat (8K), 1× Chat (8K), 1× GameNpcIdle (4K) |
| Total KV cache | ~20K tokens × ~64 KB/token (FP16) = ~1.3 GB; with INT8 KV ~ 650 MB |
| RAM footprint | base model (~2 GB) + KV (~1 GB) + working set + adapter scaffolding = ~4–5 GB |
| Concurrent inference | 1 model instance, 3 lanes in the continuous batch |
| Throughput per lane | ~3–6 tok/s on CPU per lane; aggregate ~10–18 tok/s through the batch |
| Latency target | <1s first token per lane for chat-class; voice/video may need a smaller model + warm KV (see degraded mode) |

**Higher-end** (M5 unified memory, no grid still): same architecture,
larger model (Qwen-2.5-7B Q4_K_M or even Qwen-2.5-14B Q4_K_M), more
lanes, native multimodal where the model supports it.

**With grid** (#108 AircRemoteInferenceAdapter): some lanes route to
peer machines running the same architectural shape. Discovery +
capacity broadcast over airc per [`airc-headers-are-the-routing-layer`].

---

## What we are NOT doing (clarifying the boundaries)

To avoid repeating prior-attempt mistakes, the realistic-lane design
explicitly does NOT do:

1. **No per-persona model load.** One base model per host. Different
   personas share weights.
2. **No quality-tiered model selection.** Background reflection uses
   the SAME model as live chat; throughput scales via lane budget +
   batching, not via "tier down to a 0.5B for the boring task."
3. **No hot-path LoRA swap.** Pinned realtime lanes' adapters stay
   resident. Adapter paging happens during idle windows / for
   inactive lanes only — exactly the prior-attempt failure mode the
   `Pinned` revocation policy already prevents.
4. **No global FIFO admission.** AdaptiveThroughputPlan already
   admits by `target_silicon` + `cost_units`, not strict arrival
   order. Pool class flows through via TaskKind, not via a separate
   priority field.
5. **No client-side awareness of any of this.** `ai/inference/{open,
   generate,close}` commands carry no scheduling negotiation —
   per [[inference-is-an-adapter-always-in-the-loop]] commands are
   dumb.

---

## What's NEW (the actual code to write for #109)

The substrate has the primitives. The coordinator that composes
them is the new code. The minimum viable cut:

### Step 1 — Lane type + handle binding

```rust
// core/continuum-core/src/inference/lane.rs (new)
pub struct Lane {
    persona: PersonaId,
    task: TaskKind,
    lease: ThroughputLease,
    handle_id: Uuid,  // ties to InferenceSession in handle_store
}
```

Extend `OpenSessionRequest` with `task: Option<TaskKind>` (default:
`Chat`). The handle module's `open` reaches into the coordinator
to mint a Lane before constructing the InferenceSession.

### Step 2 — Coordinator scaffold

```rust
// core/continuum-core/src/inference/coordinator.rs (new)
pub struct InferenceCoordinator {
    lease_registry: Arc<ThroughputLeaseRegistry>,
    footprint_registry: Arc<FootprintRegistry>,
    adaptive_planner: Arc<AdaptiveThroughputPlanner>, // wraps AdaptiveThroughputRequest path
    pressure_broker: Arc<PressureBroker>,
    handle_store: Arc<InferenceHandleStore>,
    lanes: DashMap<Uuid /* handle_id */, Lane>,
}

impl InferenceCoordinator {
    pub async fn open_lane(
        &self,
        persona: PersonaId,
        task: TaskKind,
        adapter: Arc<dyn AIProviderAdapter>,
        ...
    ) -> Result<HandleRef, CoordinatorError> { ... }

    pub async fn generate(&self, handle: &HandleRef, req: TextGenerationRequest)
        -> Result<TextGenerationResponse, CoordinatorError> { ... }

    pub fn close_lane(&self, handle: &HandleRef) -> Result<(), CoordinatorError> { ... }
}
```

### Step 3 — Wire the handle module through the coordinator

`InferenceHandleModule` becomes a thin facade. `open` / `generate` /
`close` delegate to the coordinator; the coordinator does the lease
+ footprint + lane work and ultimately calls into the existing
handle store for session management.

### Step 4 — Continuous batching in the LlamaCpp adapter

This is the **only genuinely new model-serving code**. The adapter
gets a `generate_batched(requests: Vec<TextGenerationRequest>)` path
that the coordinator calls instead of per-request `generate_text`.
On llama.cpp this is the existing batched-decode API. Pure adapters
(cloud, heuristic) can keep the per-request shape; their batching
is the cloud provider's problem.

Open question (Q21, new): does llama.cpp's batched decode hand back
per-sequence finish reasons cleanly? Need to verify against the
vendored llama.cpp before committing the design.

### Step 5 — Pressure-driven lane eviction

When `PressureBroker::evict_under_pressure` fires, the coordinator
walks lanes by lease revocation policy (Hard first, then Graceful,
never Pinned) and releases them. Released lanes' personas park their
KV cache to Bench tier; the lane's persona either retries with
backoff or accepts degraded service.

---

## Acceptance criteria for the realistic cut

The realistic-lane build is done when:

1. Three concurrent personas can hold open handles against a single
   base-model adapter, each at their own TaskKind, without quality
   degradation visible to any persona.
2. Tests stress 8 concurrent lanes (above the realistic 3 to prove
   headroom) without deadlock or KV cache fights.
3. PressureBroker firing evicts Hard / Graceful lanes in order
   without touching Pinned lanes.
4. The local heuristic adapter (`HeuristicInferenceAdapter`) works
   end-to-end through the coordinator, so headless CI can validate
   the multi-lane path without any model weights.
5. The trace events at every step (admission, lease acquire, batch
   admission, lane eviction, response delivery) flow through the
   capture sink pattern per [[observability-is-half-the-architecture]].

The grid case (#108) and the M5 multi-modal case (broader open
questions in
[`docs/planning/AI-LANE-OPEN-QUESTIONS.md`](../planning/AI-LANE-OPEN-QUESTIONS.md))
are extensions, not blockers. Get the realistic-lane local case
right first — the same shape extends to the higher-end targets.

---

## Open questions specific to lanes

These complement the broader open-questions doc; they're the
realistic-lane-specific decisions to make.

### Q21 — llama.cpp batched-decode finish-reason cleanliness

Does the vendored llama.cpp expose per-sequence finish reasons
(EOS / stop sequence / length) cleanly from batched decode? If not,
the coordinator has to track sequence-by-sequence state outside the
adapter.

### Q22 — model-pick policy for the realistic target

The substrate hardware probe (`governor/`) reports tier
classification. What's the canonical model-pick mapping?

- Apple Silicon UMA, ≥ 16 GB: Qwen-2.5-7B Q4_K_M
- Apple Silicon UMA, 8–16 GB: Qwen-2.5-3B Q4_K_M
- Mac Intel + Metal, ≥ 16 GB: Qwen-2.5-3B Q4_K_M (Intel Metal is slower)
- CPU-only, ≥ 8 GB: Gemma-2-2B Q4_K_M (best creative capacity at small size)
- CPU-only, < 8 GB: heuristic adapter (no model; substrate stays usable)

Joel decides this; it's policy, not architecture. But the policy
needs to live somewhere — probably as a `model_for_tier(tier)`
function in the governor module.

### Q23 — KV cache precision (FP16 vs INT8)

When KV cache tightens, does the coordinator silently switch to
INT8 KV via `inference/kv_quant.rs` (already in tree)? Or does that
require explicit policy permission? Per the adaptive-resolution
analogy this is exactly the dynamic dial we want; needs decision
gate.

### Q24 — TaskKind change mid-session

A persona starts a chat session (TaskKind::Chat, 8K budget) and the
conversation escalates to needing the bigger CodingLarge budget
(128K). Can the lane upgrade in place? Or does the persona close +
reopen the handle?

- Approach A: lane is immutable; persona reopens
- Approach B: `ai/inference/upgrade-lane { handle, new_task }`
  command that re-acquires the lease at the new budget
- Approach C: coordinator detects need from input length + auto-upgrades

Likely B for first cut; A is the bullet-proof MVP if B is too much.

### Q25 — Lane idle-state and warm vs cold KV

When a persona's lane goes idle (no requests for N seconds), does
the coordinator demote the KV cache to Warm (then Bench, then Cold)
preemptively, or only on pressure? The existing tier mechanism is
ready; the policy isn't decided.

---

## What this doc unblocks

- **Task #109** has a concrete starting plan: write the coordinator
  that composes existing primitives. No need to invent slots,
  eviction, lease, or memory accounting — those exist.
- **Task #103 hardening:** the heuristic adapter already round-trips
  through the handle commands; once the coordinator wraps the
  handle module, the heuristic adapter validates multi-lane
  serving end-to-end without GPUs.
- **Task #100 (rag-inspect ServiceModule)** can land independently;
  the realistic-lane work doesn't block it.
- **The M5 multi-modal lane target** stays in
  [`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
  as the higher-end goal; the realistic baseline doc here is the
  step that gets us functional on every host class first.

---

## Summary

The substrate has the primitives. We weren't clever enough with
LANE CONCEPTION, not with primitives. Once a lane is reframed as
"persona × TaskKind × lease over the shared base model," the
realistic-host case (3 CPU lanes, no grid, creative-capacity model)
is achievable through composition rather than invention.

The MVP cut (~1-2 weeks of focused work, given the primitives
exist) ships: Lane type, Coordinator, handle-module wire-through,
LlamaCpp batched decode, pressure-driven eviction, capture-sink
integration. Test stack uses heuristic adapter for full
multi-lane coverage without any GPU.

That's the realistic floor. Everything in
[`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
remains the aspirational ceiling — M5 multi-modal, cross-grid
offload, speculative warming, adaptive quantization at higher
sophistication. The same coordinator scaffold scales up to those
once the realistic floor lands.
