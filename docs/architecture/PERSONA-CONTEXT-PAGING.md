# Persona Context Paging — Design

**Status**: Design (2026-04-21)
**Author**: Claude + Joel, captured during the qwen3.5 scheduler debugging session
**Branch context**: written while iterating on `feature/qwen35-metal-acceleration`; supersedes the static `LlamaCppAdapter::with_context_length()` override pattern that was the immediate-term mitigation

## 0. Current State vs Target (Honest Migration Map)

This doc describes the architectural endpoint. The codebase is partway there. Knowing exactly where each piece is now is part of the design — it tells us what has to ship before paging is meaningful.

### 0.1 What's already in Rust

`continuum-core/src/`:
- `cognition/shared_analysis.rs` — analyze step (parse + JSON envelope handling)
- `cognition/response_orchestrator.rs` — score_persona / DEFAULT_RELEVANCE_THRESHOLD
- `cognition/types.rs` — shared types
- `persona/response.rs` — `respond()` entry point + `strip_thinks_emit_events`
- `persona/prompt_assembly.rs` — initial prompt build, multi_party_strategy enum, NamePrefixed/SingleUserTurn variants
- `persona/inbox.rs`, `persona/channel_*.rs` — message routing and prioritization
- `persona/genome_paging.rs` — LoRA adapter LRU + activation tracking (the §11 substrate already exists)
- `memory/cache.rs`, `memory/recall.rs`, `memory/embedding.rs`, `memory/timeline.rs`, etc. — substantial memory infra (~2800 lines)
- `inference/llamacpp_adapter.rs` + `inference/backends/llamacpp_scheduler.rs` — backend with `with_context_length` lever
- `model_registry/types.rs` — Model + Provider declarations including `multi_party_strategy`, `chat_template`, `stop_sequences`, `Capability` (now with AudioInput/Output/Vision)
- `gpu/memory_manager.rs` — accounting infrastructure (but using static `recommendedMaxWorkingSetSize` for Metal — wrong, see §12)

### 0.2 What's still in TS (and why it matters)

`system/user/server/modules/`:
- `PersonaAgentLoop.ts` (~309) — tool-call execution loop
- `PersonaResponseValidator.ts` (~110) — response shape validation
- `PersonaPromptAssembler.ts` (~343) — turn-N prompt construction (initial build duplicates Rust prompt_assembly; turn-N delta is TS-only)
- `PersonaToolExecutor.ts` (~636) — actual tool dispatch into the command system
- `Hippocampus.ts` (~693) — memory consolidation (Rust `memory/*` is the destination but consolidation passes still happen in TS)
- `PersonaResponseGenerator.ts` (~700) — orchestrator that calls Rust `personaRespond` then runs the TS agent loop

### 0.3 Live response path today

```
TS PersonaResponseGenerator
  ├─ TS RAG (ChatRAGBuilder — context assembly, source-by-source)
  ├─ Rust personaRespond (analyze + render + strip_thinks)  ← migrated
  ├─ TS runAgentLoop:
  │    ├─ TS validator
  │    ├─ TS prompt assembler turn-N
  │    └─ TS tool executor → command system
  └─ TS post to chat
```

The hot inference path (analyze + render) is Rust. The agent loop / validation / tool calling / memory consolidation is still TS.

### 0.4 Why this matters for the paging design

**The TS Node event loop is single-threaded.** With N personas in a recipe, Node services them strictly serially via its event loop; the Rust hot path runs concurrently underneath, but the moment control returns to TS, parallelism collapses.

Concrete impact: paging Phase 3.x (PageableBackend / PagingPolicy / spill+resume) is moot if the TS agent loop serializes everything anyway. We'd be paging KV slots that personas can't even reach because they're queued behind Node.

**Therefore: TS-to-Rust migration of the perf-critical persona modules is a prerequisite for paging being meaningful.** Reordered roadmap reflects this — Phase 0.5 (migration) sits BEFORE paging work in §19.

Modules that legitimately stay TS:
- Browser/widget code (`widgets/*`, lit / shadow DOM)
- Browser-only commands (`interface/screenshot`, etc.)
- WebSocket transport
- CLI scaffolding around `jtag`
- The web UI server itself

None of those are in the persona response hot path or affected by Node single-threading concerns.

## 1. Why Static Allocation Fails

The current architecture sizes per-persona KV-cache memory at backend load time as a fixed `n_ctx_seq × n_seq_max` slab. This breaks down across every realistic Continuum workload:

- **Chat** (10 personas in a room, 2 actively speaking, 8 idle): static allocation pays full KV for all 10. At qwen3.5-4b's declared 262K context, that's ~80 GB of KV. Hits the M5 Pro's 38 GB usable memory ceiling and crashes.
- **Coding** (1 persona working a 200K-token codebase): needs the full 256K window. A static "chat default" of 8K-32K **clips the model mid-task** — exactly the failure mode that haunted the qwen3.5 debugging weekend.
- **Video chat** (1 persona, image/audio frames streaming in): needs small text context but bursty multi-modal input. Static text-context sizing wastes RAM that the modality stream wants.
- **Video game** (potentially dozens of NPCs): static allocation forces an absolute cap on simultaneous personas.
- **Sentinels, Academy, learning tasks**: each has its own context profile; static defaults are wrong for at least one.

**The pattern**: limits crash, paging adapts. Same OS-level wisdom that drove virtual memory + swap.

The architectural answer is to treat per-persona context as a **runtime-adjustable resource** sized continuously from signals, with idle slots **paged to NVMe** instead of held in RAM.

## 2. Design Principles

1. **Signals, not constants.** No hardcoded "8K is enough for chat" or "256K is the default" anywhere in the adapter or scheduler. Every sizing decision derives from inputs the running system observes.

2. **Graceful degradation, never hard failure.** Memory pressure → spill more aggressively → cold-resume latency rises. User sees "AI took 1.5s to start" instead of "system crashed."

3. **Paging is the primitive, limits are emergent.** The system always *can* accommodate the next persona; what varies is *how much it costs* (latency, throughput, hot-set size). Limits show up as "above this point, cold-resume time exceeds the latency budget" — a soft economic decision, not an architectural ceiling.

4. **Single source of truth per signal.** Hardware tier is one place (`GpuMemoryManager`). Per-persona declared budget is one place (persona registry). Recipe membership is one place (recipe registry). Code reads from these, never duplicates them.

5. **Adapter pattern for the model layer.** Different model architectures (qwen, llama, mistral, gpt-oss, vision-capable, audio-native) have different KV characteristics. The paging layer talks to a `PageableBackend` trait; concrete backends (LlamaCpp, future Candle, future remote DMR-spill) implement the spill/resume primitives.

6. **No hidden defaults that bite at scale.** If a persona ends up with too little context to do its task, the fault is in the *signal* (its declared minimum was wrong, or pressure was too high), not in a constant buried in adapter code.

## 3. Core Abstractions

### 3.1 PersonaContextSlot

The unit the paging layer manages. One per persona × backend instance.

```rust
pub struct PersonaContextSlot {
    persona_id: Uuid,
    backend_id: BackendId,           // which model serves this persona
    /// Current allocation in tokens. Adjusted continuously by the
    /// PagingPolicy. Lives in `[base_budget, hard_max]` where
    /// hard_max = min(persona.declared_max, model.n_ctx_train).
    context_length: u32,
    /// Persona's declared minimum to do its job at all. Below this
    /// the slot is "unusable" — better to evict and cold-resume than
    /// to keep a starved hot slot.
    base_budget: u32,
    residency: Residency,
    /// 0.0..1.0. Driven by recipe (active speakers > silent), task
    /// (coding > chat > idle game NPC), proximity (in-game distance
    /// to player), recency (last_active). Used by the eviction
    /// policy: lowest importance evicts first.
    importance: f32,
    last_active_at: Instant,
    /// Hot KV bytes when Active; spill-file size when Idle.
    bytes_resident: u64,
}

pub enum Residency {
    /// KV pages live in GPU memory. Inference is immediate.
    Active,
    /// KV pages spilled to NVMe via `llama_state_seq_save_file`.
    /// Resume cost: ~bytes_resident / NVMe_bandwidth (M5 Pro: ~14 GB/s
    /// PCIe 5.0 ≈ 1.7s per 24 GB).
    Idle { spill_path: PathBuf },
    /// No KV state at all. Cold-resume requires re-tokenizing the
    /// prompt + prefilling. Cheapest in storage, slowest in latency.
    Cold,
}
```

### 3.2 PagingPolicy

The decision engine. Reads signals, writes slot mutations.

```rust
pub struct PagingPolicy {
    slots: Arc<RwLock<Vec<PersonaContextSlot>>>,
    /// Hardware ceiling: usable GPU/unified memory after model weights
    /// + Metal compute buffers + OS overhead. Sourced from
    /// GpuMemoryManager, not a constant.
    hardware_ceiling_bytes: u64,
    /// Live pressure signal. >=0.8 forces aggressive eviction.
    pressure_rx: watch::Receiver<f32>,
    /// Per-task-type latency budget. Chat = 200ms first-token,
    /// coding = 2s first-token (acceptable to spill-resume).
    latency_budget_by_task: HashMap<TaskKind, Duration>,
    /// Spill backend. NVMe path; could be tiered (NVMe → SATA → S3).
    spill_store: Arc<dyn SpillStore>,
}

impl PagingPolicy {
    /// Re-evaluate slot residency under current pressure. Called on:
    ///   - pressure_rx tick (every 1s)
    ///   - persona activity event (on_speak, on_idle, on_proximity_change)
    ///   - recipe change
    ///   - manual rebalance (debug / sentinel)
    pub fn rebalance(&self) -> RebalanceReport;

    /// Persona about to speak. Resume from spill if needed. Returns
    /// the latency we paid (cold ≫ idle ≫ active).
    pub async fn ensure_active(&self, persona_id: Uuid) -> Result<ResumeLatency, PagingError>;

    /// Persona finished its turn. Mark slot as recently-active;
    /// rebalance() may keep it hot or downgrade.
    pub fn on_persona_done(&self, persona_id: Uuid);

    /// Importance change — recipe, proximity, attention.
    pub fn set_importance(&self, persona_id: Uuid, new_importance: f32);
}
```

Critical property: **the policy is pure** — it reads signals and produces a desired slot state. The actual spill/resume work is delegated to the backend trait (separable, testable, swappable).

### 3.3 PageableBackend trait

What the model-layer adapters implement. Lives at the same architectural level as `AIProviderAdapter` but specifically for backends that hold KV state we can spill.

```rust
#[async_trait]
pub trait PageableBackend: Send + Sync {
    /// Allocate a sequence slot in the backend's pool. Backend may
    /// reject if hardware is exhausted; policy handles that by
    /// spilling another slot first.
    async fn alloc_seq(&self, seq_id: i32, context_length: u32) -> Result<(), BackendError>;

    /// Spill seq_id's KV state to the given path. After this returns,
    /// the backend has released the GPU pages. Resume requires
    /// `load_seq_state` then `prefill` of any new tokens.
    async fn save_seq_state(&self, seq_id: i32, path: &Path) -> Result<u64, BackendError>;

    /// Load seq_id's KV state from a previously-saved path. Returns
    /// the byte count restored (for accounting).
    async fn load_seq_state(&self, seq_id: i32, path: &Path) -> Result<u64, BackendError>;

    /// Free seq_id's slot entirely (no spill). For Cold transitions.
    async fn free_seq(&self, seq_id: i32) -> Result<(), BackendError>;

    /// Currently-allocated bytes for seq_id (Active) or 0 (Idle/Cold).
    fn seq_bytes(&self, seq_id: i32) -> u64;
}
```

`LlamaCppBackend` already has the upstream primitives (`llama_state_seq_save_file` / `llama_state_seq_load_file` exposed as raw FFI in the vendored llama.cpp). Wrapping them in this trait is the concrete first implementation.

Future backends:
- `CandleBackend` — implement spill via `safetensors` snapshot of KV tensors
- `DmrRemoteBackend` — DMR doesn't expose state save/load over HTTP (yet); spill = "evict the seq, full re-prefill on resume"
- `CloudBackend` (Anthropic, OpenAI) — no KV control; PagingPolicy treats these as `Residency::Cold` always (every turn is a fresh prefill on the cloud side anyway)

### 3.4 Signal sources

Every input the policy reads has exactly one canonical producer:

| Signal | Producer | Update cadence |
|---|---|---|
| Hardware ceiling bytes | `GpuMemoryManager::inference_budget_bytes()` | Once at boot + on hot-plug |
| Memory pressure (0.0..1.0) | `GpuMemoryManager::pressure_rx()` | 1s tick |
| Per-persona base/declared budgets | Persona entity registry | On persona create/update |
| Per-persona current importance | Recipe + activity + proximity hooks | Event-driven |
| Active recipe membership | Recipe registry | On recipe activation |
| Per-task latency budget | Task type → const map (the ONE legitimate constant in the system) | Static |
| Per-modality KV burst | Sensory bridge (vision/audio token cost) | Per-frame |

## 4. Lifecycle

State machine for a `PersonaContextSlot`:

```
                  ┌────────────────────────┐
   register ────► │   Cold (no state)      │
                  └─────────┬──────────────┘
                            │ persona invoked
                            │ alloc_seq + prefill
                            ▼
              ┌──────────► Active ◄──────────┐
              │             │                │
              │             │ idle for T_idle│
              │             │ OR pressure↑   │
              │             ▼                │
              │   spill (save_seq_state)     │
              │             │                │
              │             ▼                │
              └──── Idle (KV on NVMe) ◄──────┘
                            │
                            │ memory critical OR T_cold
                            │ free_seq + delete spill
                            ▼
                    ┌─────────────────┐
                    │  Cold (no state) │
                    └─────────────────┘
```

Transitions are driven by the `PagingPolicy::rebalance()` decisions, not by the persona itself. The persona just calls `ensure_active(persona_id)` and waits — the policy resumes whatever residency it was in.

## 5. Scenario Walkthroughs

### Chat (10 personas, 2 active speakers)

- All 10 slots `register`. 2 immediately go `Active` (the speakers). 8 stay `Cold` until called.
- A persona enters the conversation: `ensure_active` → Cold → Active. Cost: full prefill (~1-3s on M5 Pro for a 5K-token system prompt).
- A speaker finishes its turn: `on_persona_done`. Slot stays `Active` until 60s of silence, then policy spills to `Idle`.
- Same persona speaks again 30s later: `Active` already, immediate response (~50ms first-token).
- Same persona speaks again 5 minutes later: `Idle` → Active resume (~1.7s for 24GB spill restore on NVMe — but with prefix sharing, much less).

### Large coding task (1 persona, 200K context)

- Slot has `base_budget=200K`. PagingPolicy honors it; allocates 200K KV at start.
- All other persona slots downgrade — coding persona has high `importance=0.9`, others get evicted to make room.
- Hardware ceiling enforces: if 200K KV doesn't fit even with everyone else evicted, the policy refuses the allocation and surfaces a clear error: "this task needs $X bytes; available is $Y; reduce context, evict more, or upgrade hardware."

### Video game (NPC density)

- 50 NPC personas register. All start `Cold` (no KV state, but persona entity loaded).
- Player approaches NPC₁: proximity event → `set_importance(NPC₁, 0.6)` → policy promotes to `Idle` (preallocates spill space) or `Active` (if memory permits + latency budget says first-token < 200ms).
- Player walks within talking distance: `set_importance(NPC₁, 0.9)` → `Active`. First conversation pays cold-prefill cost.
- Player walks away: `set_importance(NPC₁, 0.2)` → spill to `Idle`.
- 50 NPC slots in steady state: maybe 3 `Active` (current convo + 2 nearby), 10 `Idle` (recently visited, fast-resume), 37 `Cold`. Total memory: ~hardware budget.

### Video chat (visual frame burst)

- Persona slot has `base_budget=8K` for normal chat conversation.
- A frame arrives requiring vision processing: persona declares `+8K transient` for the frame's image tokens. Policy temporarily allocates if budget allows; if not, defers the visual processing or spills another slot to make room.
- Frame consumed: transient released. Slot returns to `8K` baseline.

### Memory pressure spike (game running in background)

- `GpuMemoryManager::pressure_rx` jumps from 0.3 to 0.85 (game grabbed VRAM).
- `PagingPolicy::rebalance` fires.
- All `Active` slots reconsidered: lowest-importance ones spill to `Idle`. If pressure stays high, oldest `Idle` slots go `Cold`.
- User notices: maybe one persona that was instant-response now takes 1.5s to respond. **Acceptable degradation, no crash.**
- Pressure drops (game closed): eviction relaxes; recently-spilled slots get pulled back to `Active` opportunistically (or on-demand on next turn — TBD policy).

## 6. RAG Efficiency (Second Axis)

The current RAG dumps a ~30KB system prompt **per persona, per turn**, fully duplicated across all sequences. That's both a context-window problem (clips smaller models) and a memory problem (every seq's KV holds the same prefix).

Two complementary wins:

### 6.1 KV prefix sharing

llama.cpp's continuous-batching scheduler can be configured to recognize identical prompt prefixes across sequences and share the prefix's KV pages. We pay prefill ONCE for the shared system prompt; each sequence only pays for its delta.

For Continuum's typical chat (multiple personas in same room, identical room context):
- Old: N personas × 8K shared prefix = N × 8K KV
- New: 1 × 8K prefix (shared) + N × delta = 8K + N × small

Savings scale linearly with the number of personas in the same context.

### 6.2 Lazy RAG fetch

Currently RAG dumps everything the persona *might* need: tool defs, consolidated memories, room context, sentinel info, governance, capabilities. Most of it isn't relevant to any given turn.

Better: **RAG provides a minimal initial context + tool surface**. The model issues tool calls (`memory/query`, `room/context`, `tool/get`, `docs/search`) for the bits it actually needs. Initial context shrinks dramatically; total tokens-fetched stays small because most queries don't need deep context.

Tradeoff: latency. Lazy fetch = extra tool roundtrips before first useful response token. Acceptable for substantive turns, painful for "hi" replies. Policy decides per-task: chat = preload, code = lazy.

These are separable from the paging work but both reduce per-slot RAM, multiplying the paging headroom.

## 7. Implementation Phases

### Phase 0 (current, done)

- `LlamaCppAdapter::with_context_length(n)` exists for explicit caller override
- Per-model `multi_party_strategy` declared in registry
- AudioInput / AudioOutput / Vision capabilities declared per-model
- Test rig (`persona_respond_replay.rs`) reproduces prod-shape input

### Phase 1 — Persona-declared context budgets (this week)

- Add `context_budget_min` / `context_budget_max` to persona entity
- Recipe declares active personas
- At backend load time, sum active personas' `context_budget_min` → that's the floor
- Adapter sizes KV to `min(sum_of_maxes, hardware_ceiling)`
- No runtime adjustment yet; size set once at recipe activation

This is the smallest viable improvement over today's static allocation. **Crucially, NO hardcoded constants** — everything reads from persona/recipe/registry data.

### Phase 2 — `PageableBackend` trait + spill primitives (1-2 weeks)

- Define the trait; first impl is `LlamaCppBackend` wrapping `llama_state_seq_save_file` / `load_file`
- Spill store = NVMe directory (`~/.continuum/persona-state/<persona-id>/<seq>.kv`)
- Manual API only (`Backend::spill_seq(id) → Result`); no policy yet
- Tests: spill + resume produces identical KV (token-equivalence test)

### Phase 3 — `PagingPolicy` + signal wiring (1-2 weeks)

- The policy struct + state machine
- Signals wired: GpuMemoryManager pressure, recipe membership, persona importance, last_active
- `rebalance()` called on policy tick (1s) + activity events
- Eviction policy: lowest-importance + oldest-active spills first
- Cold-resume on `ensure_active`

### Phase 4 — KV prefix sharing (1 week)

- llama.cpp scheduler config for prefix-sharing across seqs
- Prompt assembler emits a stable "shared prefix" segment
- Per-seq deltas keyed off the prefix
- Verify KV memory drops with N seqs sharing the prefix

### Phase 5 — Lazy RAG fetch (2-3 weeks)

- RAG initial context shrinks to identity + tool surface
- Tool defs for `memory/query`, `room/context`, `docs/search`, etc.
- Per-task default: chat preloads more, code preloads less
- Latency telemetry to confirm net wins

### Phase 6 — Tiered spill (later)

- NVMe → cold storage (S3, network share) for very-long-idle personas
- Useful for "10000 NPC personas registered, 10 ever active in a session"

## 8. Open Questions / Risks

1. **Spill atomicity under inflight requests.** If persona A is mid-generation and the policy decides to spill it for persona B's resume, what happens to A's stream? Likely: defer eviction until A's current turn completes. Need a "pinned active" flag during inflight.

2. **NVMe wear from frequent spill cycles.** Heavy chat (turns every few seconds) could thrash. Mitigation: don't spill until idle for `T_idle ≥ 30s`; eviction policy prefers truly-idle slots.

3. **Cold-resume with KV-prefix-sharing.** If the shared prefix's KV is in another seq's slot that ALSO got spilled, resume needs to rebuild the prefix first. Detail: the shared prefix lives in a "phantom" seq_id whose lifecycle is tied to the recipe, not to any one persona.

4. **Cloud-adapter handling.** Cloud models (Claude, GPT) have no KV control from our side — every turn is a fresh prefill on their side. PagingPolicy treats these as always-`Cold` from a memory-accounting standpoint (we hold no KV state for them); the spill/resume primitives are no-ops.

5. **Vision/audio modality bursts** add tokens transiently. Need a separate "transient KV" channel that doesn't count against the persona's steady-state budget but does count against the hardware ceiling.

6. **What if `n_ctx_train` itself isn't honored by llama.cpp?** Some models clip silently when n_ctx exceeds what their GGUF metadata declares accurate. Need verification per model — the registry's declared `context_window` should be the tested ceiling, not just the metadata read.

7. **Recipe transitions.** Switching recipes (chat room → coding session) means re-evaluating ALL slots. Hot personas in the old recipe might be irrelevant in the new one (evict). New personas in the new recipe weren't allocated yet (cold-load). Transition cost is bounded by `count(new ∪ old) × per-persona-load-cost`.

8. **Is there a backend that benefits from KEEPING idle KV warm in CPU RAM** (vs always going to NVMe)? Possibly — Apple unified memory makes "GPU → CPU spill" much cheaper than "GPU → NVMe spill." Could add a `Residency::CpuResident` tier between Active and Idle.

## 9. Learned Policy — The Right Long-Term Implementation

The signals enumerated in §3.4 — pressure, latency budget, importance, recency, modality, recipe, hardware tier — are too many, too entangled, and too situation-dependent for hand-coded rules to balance well. The list is also incomplete: real workloads will surface signals we haven't named yet (time of day, user typing rhythm, network conditions if cloud adapters are mixed in, sentinel job priorities, learning-task progress).

The right long-term shape of `PagingPolicy::rebalance()` is **a learned policy, not a rule set**. Same architectural pattern that beats hand-coded heuristics in:

- macOS / iOS power management (CPU frequency, wake-up scheduling — learned from per-user activity)
- RTOS task schedulers with adaptive priorities
- vLLM's dynamic batching (learned scheduling from observed throughput)
- OS page-replacement (LRU is the textbook answer; ML-augmented replacement consistently outperforms it on real traces)

### Pre-learning phase (rules)

The hand-coded `PagingPolicy::rebalance()` from §3.2 is the **initial training scaffold**. It's deliberately conservative: simple eviction-by-importance × recency rules, easy to reason about, easy to debug. Its purpose isn't to be the final answer; its purpose is:

1. To run the system at all (Phases 1-3 ship without ML)
2. To **emit telemetry** that becomes the training signal (which decisions caused user-visible latency; which spills were "wasted" because the slot was needed back within seconds; which slots stayed hot for nothing)

### Telemetry → training corpus

Every rebalance decision records:

- The **state vector**: pressure, per-slot residency + importance + last_active + base_budget, hardware ceiling, modality flags, recipe membership
- The **action**: which slots changed residency, allocation deltas
- The **outcome** (observed over the next N seconds):
    - Was a spilled slot needed back within `T_recall`? (cost: cold-resume latency the user felt)
    - Did the kept-hot slot stay idle? (cost: RAM that could have been freed)
    - Was an evicted slot's persona requested for a fresh turn that took longer than the latency budget? (cost: SLA miss)

This is exactly the shape the existing fixture-capture pattern (`~/.continuum/fixtures/persona-respond/`) already uses for persona-render training data: state + action + outcome. The same FIFO-pruning + content-addressing architecture applies.

### Learned policy

A small model (don't need 4B for this — a few-MB MLP or even a decision tree forest is plenty) trained on the corpus to produce, given the state vector, the action that minimizes the cost function:

```
cost = α × cold_resume_latency_misses
     + β × wasted_hot_RAM_seconds
     + γ × SLA_miss_count
     + δ × NVMe_write_thrash
```

The α/β/γ/δ weights themselves are tunable per-hardware-tier and per-user-preference (a power user might weight latency lower than RAM headroom for their other work). Eventually those weights are also learned from user feedback ("system felt sluggish" / "ran out of RAM" / "felt great").

### Continuous improvement loop

The same machinery Continuum already uses for persona learning (Forge, Academy, Sentinel-AI) trains the paging policy:

- Collect telemetry from real sessions (sharded JSONL, FIFO-pruned, content-addressed — same pattern as the persona fixtures)
- Periodic retraining job (daily / weekly batch on a sentinel)
- A/B test new policy vs current on a fraction of decisions; promote when it dominates on the cost function
- Roll back trivially (the policy is a tiny artifact; swap it like a model)

### Why not just hand-tune the rules?

Because the **right balance changes per machine, per user, per workload, per time-of-day**, and hand-tuning on one engineer's laptop produces rules that fail on someone else's. A learned policy adapts to the actual deployment without anyone editing constants.

This is the same lesson that made macOS's power management win against the older "static governor" approach — too many signals, too much variance, judgment beats rules at scale.

### Phase 7 (post-paging-shipping)

- Define the cost function (start with simple weighted sum, refine from user feedback)
- Wire telemetry capture inside `rebalance()` 
- After ~1 month of real usage, train the first learned policy
- A/B against the rule-based policy; ship if it wins
- Continuous retraining as part of the normal Forge/Academy cadence

The rule-based policy never goes away — it's the **safe-mode fallback** when the learned policy hasn't been trained yet (new install, new hardware tier) or when its decisions look out-of-distribution (sanity-check guardrails). Same pattern as macOS's "performance" preset acting as the rule-based safety net under the learned governor.

## 10. The Rust Layer Is Bidirectional — Levers AND Telemetry

The policy (rule-based today, learned tomorrow) doesn't itself touch GPU memory or NVMe. The Rust layer is what makes the policy's decisions real, and what gives the policy the visibility to decide intelligently. The contract is **bidirectional**:

### 10.1 Levers — what the Rust layer exposes downward

The mechanisms the policy invokes to change reality:

```
PageableBackend trait (model layer):
  alloc_seq(seq_id, context_length)
  save_seq_state(seq_id, path)         // spill KV to NVMe
  load_seq_state(seq_id, path)         // resume KV from NVMe
  free_seq(seq_id)                     // discard KV entirely
  resize_seq(seq_id, new_context_length)  // adjust budget without spill

GenomeBackend trait (adapter layer):
  load_adapter(adapter_id) → ActivateSkillResult   // already in genome_paging.rs
  evict_adapter(adapter_id)                        // already in genome_paging.rs
  spill_adapter(adapter_id, path)                  // future: spill to NVMe vs full evict
  bind_adapter_to_seq(seq_id, adapter_id)          // per-seq LoRA composition

SpillStore trait (storage layer):
  write(key, bytes) -> latency observed
  read(key) -> bytes + latency observed
  delete(key)
  available_bytes()
```

The traits are the architecture's contract. New backends (Candle, Mistral.rs, future cloud adapters with state APIs) implement them; the policy doesn't change.

### 10.2 Telemetry — what the Rust layer reports upward

What the policy reads to make its next decision:

```
Memory observability (continuous):
  GpuMemoryManager::pressure() -> 0.0..1.0
  GpuMemoryManager::inference_budget_bytes() -> u64
  GpuMemoryManager::total_vram_bytes() -> u64
  per-backend resident_bytes() per seq_id
  per-adapter resident_bytes() per adapter_id

Latency observability (per operation):
  prefill_ms, decode_ms_per_token (already in llamacpp_scheduler perf log)
  spill_ms, resume_ms (the cost the policy paid for paging decisions)
  cold_load_ms (worst-case persona resume)
  adapter_swap_ms (already tracked in genome_paging)

Behavioral observability (post-hoc, for the learned policy's training):
  was_spilled_seq_resumed_within(threshold) -> bool   // "wasted spill" signal
  was_kept_hot_seq_idle_for(threshold) -> bool        // "wasted RAM" signal
  did_first_token_meet_latency_budget -> bool         // SLA signal
  attention_distribution_over_context -> Vec<f32>     // RAG efficiency signal
```

Both directions are first-class Rust types. The policy is just the consumer of telemetry + producer of lever invocations. The Rust layer is what makes the policy *possible* — without the levers it has no way to act, without the telemetry it has no way to learn.

This is also the reason the policy can be progressively replaced (rule → ML → anything else) without changing the substrate. The Rust contract stays stable; the policy implementation evolves underneath the same trait surface.

## 11. LoRA / Genome Adapters Are the Same Paging Problem

`persona/genome_paging.rs` already tracks per-adapter state — `GenomeAdapterInfo` with priority, loaded-flag, last-activated, trained-model name. This was scoped as "page LoRA adapters in/out based on task domain" in the Persona Convergence Roadmap, which is conceptually identical to KV-state paging — the only difference is what's being paged.

**The right architecture: one PagingPolicy, two resource types** (KV state + LoRA adapters), each with a `PageableResource` trait variant. Same lifecycle states, same signal-driven decisions, same eviction logic.

### 11.1 LoRA-specific dimensions

Adapter paging adds nuances KV doesn't have:

- **Compositional**: a single inference can apply N LoRA adapters simultaneously (per-layer scaling). The paging policy needs to track which COMBINATION is active per seq, not just which individual adapters.
- **Compacted base model**: per `genome_paging.rs::CompactionMetadata`, some adapters target a compacted base (fewer attention heads). Loading such an adapter implies switching the base — much heavier than just adding LoRA weights to the standard model. The policy's cost model has to account for this.
- **Bigger spill cost relative to size**: LoRA adapter weights are tens of MB each; the resume cost per byte is dominated by the disk seek, not the bandwidth. Spilling a small adapter is rarely worth it; evicting (full discard, re-download from storage on resume) is often the right move.
- **Hot-swap mid-conversation**: a persona shifts from chat to coding mid-turn. The right LoRA shifts. Paging policy needs to allow per-turn adapter set changes without invalidating the persona's KV state (since LoRA changes the model's output distribution but not the KV layout — the existing KV remains valid).

### 11.2 Combined budget

Total persona memory cost = `KV_bytes + active_adapter_bytes + base_model_share`. The policy budgets across all of it:

```
hardware_ceiling
  = base_model_load (Q4 4B = ~2.5GB for qwen3.5)
  + sum(active KV slots × per-slot context_length × per-token-cost)
  + sum(active LoRA adapters × adapter_size)
  + sum(active compacted_base_models × base_size)
  + Metal compute buffers (~1GB)
  + OS overhead
```

When pressure rises, the policy chooses which to spill: KV first if cheaply re-prefillable, LoRA adapters if recently-unused, compacted-base last (most expensive to reload). Cost-driven, not type-prioritized.

### 11.3 LoRA + KV interaction in lifecycle

When a persona spills its KV but keeps its LoRA loaded (cheaper memory + per-byte spill cost), the LoRA stays "warm" — next persona resume is fast because only KV needs to come back from NVMe. When BOTH are spilled, full cold-resume.

State combinations:
- KV=Active, LoRA=Active: persona ready to speak immediately
- KV=Idle, LoRA=Active: persona waking up (~1.7s for KV resume, LoRA already there)
- KV=Idle, LoRA=Cold: persona waking up + adapter reload (~few hundred ms extra)
- KV=Cold, LoRA=Cold: full cold-start (worst case, multi-second)
- KV=Active, LoRA=Cold: rare — usually paired

### 11.4 Existing infrastructure to integrate

Per `persona/genome_paging.rs`:
- `GenomePagingState` is already the right shape for the LoRA half
- `ActivateSkillResult` already returns `evicted` adapters — the eviction primitive exists
- Plasticity compaction is already accounted for

The integration work is:
1. Extract a `PageableResource` trait that both `GenomePagingState` and the new `PersonaContextSlot` implement
2. Move the eviction-decision logic OUT of `GenomePagingState` (currently inline) and into the unified `PagingPolicy`
3. Have the policy compose: "to make room for X bytes, evict the lowest-cost combination of KV slots + adapters that frees X bytes"

This is also where the Academy / Forge / Sentinel-AI hooks plug in — fine-tuning produces new adapter artifacts, and the paging system has to know about them at registration time so the policy can budget them.

## 12. GPU/Memory Monitoring Is the Same Adapter Pattern

The current `GpuMemoryManager` (`continuum-core/src/gpu/memory_manager.rs`) is the symptom of the broader anti-pattern: one struct with `#[cfg(target_os = "macos")]` / `#[cfg(feature = "cuda")]` branches, each platform doing different (and uneven) things:

- **Metal path (macOS)**: `MTLDevice.recommendedMaxWorkingSetSize()` — a STATIC lifetime hint, not live free memory. Pressure tracking is internal accounting only; the system never asks Metal "how full are you actually right now?"
- **CUDA path**: shells out to `nvidia-smi` for total VRAM at startup. No live observation. No per-process attribution.
- **CPU fallback**: a percentage of system RAM. No notion of pressure at all.
- **Vulkan / AMD / Intel**: not handled.
- **Pressure** is computed from our own bookkeeping of what we allocated, not from the OS. If a video game grabs 8GB outside our process, our pressure stays at 0.0 — we have no idea.

This is why "the macbook one didn't seem to work" — it wasn't actually monitoring; it was reporting our internal accounting state with a Metal label.

### 12.1 The right shape — a `GpuMonitor` trait per platform

```rust
/// Live, fast-to-read memory + utilization signals for the policy.
/// Each implementation talks to its platform's actual monitoring API.
#[async_trait]
pub trait GpuMonitor: Send + Sync {
    fn platform(&self) -> &'static str;        // "metal" | "cuda" | "vulkan" | "cpu"
    fn device_name(&self) -> &str;

    /// Total physical VRAM (or unified memory share for Apple Silicon).
    fn total_bytes(&self) -> u64;

    /// CURRENT free bytes — observed from the platform, not our accounting.
    /// This is what tells us a video game grabbed our headroom.
    fn free_bytes(&self) -> u64;

    /// Bytes allocated by OUR process specifically. Lets us distinguish
    /// "the system is tight" from "we are tight."
    fn process_bytes(&self) -> u64;

    /// Compute utilization (0.0..1.0). Important for the policy's
    /// latency model — if the GPU is already busy with something, our
    /// inference latency goes up. Unused budget but high utilization
    /// = same effective pressure.
    fn utilization(&self) -> f32;

    /// Optional thermals (throttling kicks in around 90-95°C).
    /// Policy may downgrade priority if approaching throttle.
    fn temperature_c(&self) -> Option<f32>;

    /// Optional power draw (watts). For laptop / battery scenarios:
    /// policy can prefer cheaper-paged states when on battery.
    fn power_watts(&self) -> Option<f32>;

    /// Subscribe to live pressure (free→used ratio + utilization blend).
    /// Tick rate is platform-specific (Metal: ~1Hz cheap; nvml: 10Hz cheap;
    /// nvidia-smi: 1Hz expensive — implementation hides the cost).
    fn pressure_rx(&self) -> watch::Receiver<f32>;
}
```

### 12.2 Platform implementations (each their own crate-internal module)

**`MetalMonitor`** (`gpu/metal_monitor.rs`) — Apple Silicon is fundamentally different from discrete-VRAM GPUs and the previous monitoring bug was using the wrong primitive. Specific corrections:

The misconception to avoid: **Apple Silicon does NOT have separate VRAM**. CPU and GPU share the SAME unified memory pool. There is no "GPU memory free" number. What matters is *system-wide* unified-memory pressure plus our process's footprint within the OS-imposed per-process limit.

- `total_bytes`: `MTLDevice.recommendedMaxWorkingSetSize()` is **NOT total memory** — it's a hint about how large a single GPU work submission *can be at once*. It's a static value that does not change as memory fills. The previous bug treated this as live capacity. **Correct source for total**: `host_statistics64(HOST_VM_INFO64)` for total physical RAM (the actual unified-memory pool).
- `free_bytes`: there is no per-GPU free number. The right value is **system-wide unified memory available**, computed as: `(free + inactive + speculative + purgeable) pages × page_size` from `host_statistics64`. This jumps when ANY app (game, browser, Xcode build) frees memory; it drops when ANY app allocates. That's what makes it actually useful to the policy.
- `process_bytes`: `task_info(TASK_VM_INFO)` returns `phys_footprint` — our process's resident bytes. Per-process attribution = system pressure minus our footprint = "how much pressure is from things we can't control."
- `os_proc_available_memory_limit()`: per-process limit before the OS kills us (jetsam on iOS, less aggressive on macOS but still real). Critical signal — our policy must keep our footprint well below this. Available via `os_proc_available_memory()` (returns bytes available before OOM). On macOS this returns 0 if no limit (unlikely on a machine with active GPU pressure).
- `currentAllocatedSize()`: `MTLDevice.currentAllocatedSize()` returns bytes the Metal driver currently has allocated for OUR process. Useful for accounting GPU-resident KV (vs. CPU-resident model weights via mmap). Live, cheap.
- `utilization`: NOT directly exposed by Metal. The path is **IOReport** (private but stable framework Apple has used for `powermetrics` since 11.0):
  - `IOReportCreateSubscription` against the `IOAccelerator` channel
  - Reads delivery: `IOReportSubscriptionCreate` → `IOReportCopySamples` periodically → diff samples to get GPU active %
  - This is exactly what Activity Monitor's GPU history graph reads from
  - Crate option: `mach2` exposes the Mach syscalls directly; for IOReport specifically there's no maintained crate so a small FFI wrapper is required
- `temperature_c`: also IOReport via the SMC channel (`IOReportSubscriptionCreate` with `kIOPSAccessoryCategorySMCKey`). Stable on M-series. Throttle threshold: ~95°C for sustained, soft-throttle starts ~85°C.
- `power_watts`: IOReport `pmp` channel for SoC power, `gpu_pwr` subchannel specifically. Same subscription pattern.
- Pressure derivation: `pressure = 1.0 - (system_free_bytes / system_total_bytes)` blended with `our_footprint / os_proc_available_memory_limit`. NOT internal allocation accounting — that's what the old bug did wrong.
- Tick rate: IOReport subscriptions are push-based (callback when sample ready), no polling cost. Memory stats: 100ms host_statistics64 polls are essentially free.

**Implementation note**: the metal-rs crate exposes `MTLDevice` cleanly but does NOT cover IOReport. We'd need a small `gpu/metal_ioreport.rs` FFI shim. Apple's headers are in `IOKit.framework/Headers/IOReport.h` — the entire API surface we need is ~10 functions. Reference implementations: `asitop` (Python), `socpowerbuddy_swift` — both confirm the IOReport channel names.

**Critical test**: open Activity Monitor → GPU tab → run a Metal compute load → verify our `MetalMonitor::utilization()` matches Activity Monitor's reading within 1-2 percentage points. If it doesn't, the IOReport channel name or sample math is wrong. This is the test that would have caught the previous bug at PR time.

**`NvidiaMonitor`** (`gpu/nvidia_monitor.rs`):
- Use **NVML directly** (the `nvml-wrapper` crate), NOT `nvidia-smi` shelling. NVML is in-process, microseconds-fast, and exposes everything `nvidia-smi` does plus more.
- `total_bytes`, `free_bytes`, `process_bytes`: `Device::memory_info()` and `Device::process_info()`.
- `utilization`: `Device::utilization_rates().gpu`.
- `temperature_c`: `Device::temperature(TemperatureSensor::Gpu)`.
- `power_watts`: `Device::power_usage()`.
- ECC errors, throttling reasons, clock speeds also available — bonus telemetry for the learned policy.
- Pressure tick: 100ms cheap.

**`VulkanMonitor`** (`gpu/vulkan_monitor.rs`):
- For AMD / Intel / older NVIDIA paths.
- `VK_EXT_memory_budget` extension gives per-heap budget + usage.
- Cross-vendor; same code works for AMD MI / Intel Arc / Apple Silicon (when MoltenVK is preferred over Metal).

**`CpuMonitor`** (`gpu/cpu_monitor.rs`):
- The "no GPU" fallback we have now, but shaped as an adapter so the rest of the code doesn't care.
- `total_bytes` = system RAM. `free_bytes` = `/proc/meminfo` (Linux) or `host_statistics64` (macOS).
- `utilization` = `loadavg` or `host_processor_info`.
- Treats CPU inference paths the same way GPU paths are treated by the rest of the system.

### 12.3 Detection at boot — selection, not concatenation

```rust
pub fn detect_monitor() -> Box<dyn GpuMonitor> {
    #[cfg(target_os = "macos")]
    if let Some(m) = MetalMonitor::try_new() { return Box::new(m); }
    #[cfg(feature = "cuda")]
    if let Some(m) = NvidiaMonitor::try_new() { return Box::new(m); }
    #[cfg(feature = "vulkan")]
    if let Some(m) = VulkanMonitor::try_new() { return Box::new(m); }
    Box::new(CpuMonitor::new())
}
```

The PagingPolicy holds an `Arc<dyn GpuMonitor>`. Adding a new platform = adding a new module; no policy changes. Same OOP / single-source-of-truth pattern as the model_registry's per-model strategy declarations.

### 12.4 What "monitoring rocks" looks like

Concrete properties the adapter pattern gives us:

1. **Live pressure from the OS**, not from our internal tally. Video game in the background = pressure jumps immediately.
2. **Per-process attribution** — the policy can tell "system is tight" from "we are tight" and react differently (system-tight → spill OUR slots aggressively; we-are-tight but system-fine → just rebalance internally).
3. **Utilization + memory blend** — pressure isn't only "is RAM full"; it's also "is the GPU compute path saturated." A persona can't get fast inference even with KV in RAM if the GPU is running a render task.
4. **Thermal awareness** — if the M5 is approaching 95°C, policy downgrades batch tasks to let the chip cool. Same RTOS pattern.
5. **Power awareness** — battery mode preferences differ from plugged-in. Policy reads `power_watts` + battery state and weights its cost function accordingly. This is the macOS-power-management analogy made concrete.
6. **Fast tick rates** — NVML and IOReport are cheap enough to sample at 100ms-1Hz without measurable overhead. The policy gets near-realtime signals.
7. **Telemetry corpus stays uniform** — the learned policy in §9 doesn't care which platform produced the signals; the trait normalizes them.
8. **No `#[cfg]` ladders in the policy** — that mess lives in the adapter modules where it belongs.

### 12.5 Phase 1.5 — extract the trait from current code

Smallest path to the adapter shape from where we are:

1. Define the `GpuMonitor` trait
2. Carve `detect_metal` / `detect_cuda` / CPU-fallback out of `memory_manager.rs` into `gpu/metal_monitor.rs` / `gpu/nvidia_monitor.rs` / `gpu/cpu_monitor.rs`
3. `GpuMemoryManager` becomes a thin wrapper holding `Arc<dyn GpuMonitor>` + the existing budget/eviction logic
4. Replace the static `recommended_max_working_set_size` Metal call with the LIVE `currentAllocatedSize` + `os_proc_available_memory` combo — that's the actual fix to "macbook monitoring didn't work"
5. Replace the `nvidia-smi` shell-out with NVML

Tests per adapter (small, fast, bench-able):
- "MetalMonitor reports total > 0 on macOS, panics on Linux"
- "NvidiaMonitor reports utilization within ±5% of nvidia-smi reading"
- Mock monitor for unit tests of the policy itself (`MockMonitor` returning scripted pressure curves to simulate "video game starts at t=10s, ends at t=30s")

This is the same pattern as `MultiPartyChatStrategy` in §11 of the model registry: declared once per platform, consumed everywhere. The policy never branches on platform name — it reads the trait.

## 13. Per-Component Footprint — The Other Half of Monitoring

System-level signals (§12) tell the policy WHAT pressure looks like. Per-component attribution tells the policy WHAT to do about it. Without this, the policy knows "we're at 90% of our process limit" but has no idea which of the 47 things in our process is the biggest, the cheapest to spill, or worth keeping hot.

### 13.1 The dimensions that matter

For every byte we hold, we want to know:

| Dimension | Why the policy needs it |
|---|---|
| **Per-persona** | Eviction target ("which persona is biggest? least active?") |
| **Per-resource type** (KV / LoRA / model weights / render buffers / tokenizer / Bevy world) | Different spill costs per type — KV cheap to spill, base model expensive to reload |
| **Per-backend instance** | Multi-model setups: qwen3.5 backend KV vs. Claude API client buffers |
| **Per-recipe context** | Recipe-driven importance: same persona's bytes might be high-importance in chat, low in idle game-NPC |
| **Per-residency tier** | Active GPU bytes vs. CPU-resident vs. NVMe-spilled — different reclaim semantics |
| **Hot vs. cold within a tier** | Recently-touched pages vs. truly-cold (LRU signal for the policy) |

A single number (`phys_footprint = 8.2 GB`) collapses all six dimensions to one. The policy needs the projection back.

### 13.2 The `FootprintRegistry`

Central registry that every allocation site reports to. This is the dual of the `GpuMonitor` trait — the OS tells us system pressure, the registry tells us our own composition.

```rust
pub struct FootprintRegistry {
    entries: DashMap<FootprintKey, FootprintEntry>,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct FootprintKey {
    pub persona_id: Option<Uuid>,        // None = persona-agnostic (model, renderer, etc.)
    pub recipe_id: Option<Uuid>,
    pub backend_id: Option<BackendId>,
    pub resource_type: ResourceType,     // Kv | LoraAdapter | ModelWeights | RenderBuffer | TokenizerCache | BevyWorld | Other(&'static str)
    pub residency: Residency,            // Active | Idle (NVMe) | CpuResident | Cold
}

pub struct FootprintEntry {
    pub bytes: u64,                      // Live count, updated via add/remove
    pub last_active: Instant,            // For LRU within type
    pub backend_reported: bool,          // True = ground truth from backend; False = our accounting
    pub spill_cost_estimate: Duration,   // What the policy expects to pay if it evicts
    pub reload_cost_estimate: Duration,  // What it costs to bring back
}

impl FootprintRegistry {
    pub fn add(&self, key: FootprintKey, bytes: u64);
    pub fn remove(&self, key: FootprintKey, bytes: u64);
    pub fn touch(&self, key: &FootprintKey);  // update last_active

    // ── Projections the policy reads ──

    /// Total bytes attributed to a persona across all resource types
    /// and tiers. The "how big is Helper right now?" answer.
    pub fn persona_total(&self, persona_id: Uuid) -> u64;

    /// Bytes per resource type globally. The "where's the weight?"
    /// answer — usually the model weights dominate, but if a vision
    /// burst spiked we'd see it here.
    pub fn by_resource_type(&self) -> HashMap<ResourceType, u64>;

    /// Cheapest combination of evictable entries that would free at
    /// least `target_bytes`. Evictability filtered by importance +
    /// residency (e.g. base model isn't evictable under normal pressure).
    /// Returns the eviction plan with estimated total cost.
    pub fn cheapest_eviction_for(&self, target_bytes: u64, exclude: &[Uuid]) -> Option<EvictionPlan>;

    /// Cross-check: registry sum vs. OS-reported phys_footprint.
    /// Discrepancy > 10% = something allocates without reporting →
    /// bug to chase. Same role as a memory-leak watchdog.
    pub fn sanity_check(&self, monitor: &dyn GpuMonitor) -> RegistryHealth;
}
```

### 13.3 Where reporting happens

Every allocation site in the system reports to the registry. There aren't that many:

| Site | What gets reported |
|---|---|
| `LlamaCppBackend::alloc_seq` / `free_seq` | KV bytes per (persona, backend, residency) |
| `LlamaCppBackend::save_seq_state` / `load_seq_state` | residency transitions Active ↔ Idle (bytes move, total per persona stays same) |
| `GenomePagingState::activate_skill` / `evict` | LoRA adapter bytes per (persona, residency) |
| `LlamaCppBackend::load` | model weights bytes (persona_id=None, backend_id=Some, type=ModelWeights) |
| Tokenizer cache load | bytes per backend, type=TokenizerCache |
| Bevy renderer slot create | bytes per slot, type=BevyWorld |
| Embedding model load | bytes for the embedding model |
| Live audio/video pipelines | per-call bytes (small, but spike-y for video frames) |
| Cloud API clients (Claude, OpenAI HTTP buffers) | small but non-zero |

The reporting is **unconditional and cheap** (a single `DashMap::entry().and_modify`); no `#[cfg]`, no platform branches. Wherever we know we allocated bytes, we tell the registry. The registry is the single place where "what are we made of right now?" is answered.

**Backends report ground truth where they can.** `LlamaCppBackend::seq_bytes(seq_id)` returns the actual GPU-resident byte count for a sequence (sums the K and V tensor sizes for that seq's allocated cells). When the backend has a real number, it overrides our internal accounting via `report_authoritative(key, bytes)`. This catches drift between "what we think we allocated" and "what the backend actually has."

### 13.4 Cost estimates aren't guessed — they're learned

`spill_cost_estimate` and `reload_cost_estimate` start as rough heuristics (KV: bytes / NVMe_bandwidth; LoRA: file_size / disk_bandwidth + GPU_upload_cost; ModelWeights: very high, never spill in practice). But every actual spill or reload measures and updates them — same telemetry loop §9 describes for the policy. After a few hundred spill cycles per resource type we have empirical cost distributions per hardware tier. The policy uses these for its eviction plan calculations.

### 13.5 The eviction-plan API the policy uses

```rust
// Policy: "I need 2 GB to fit this new request without going past
//         os_proc_available_memory_limit. What's it cost?"
let plan = registry.cheapest_eviction_for(
    target_bytes: 2 * 1024 * 1024 * 1024,
    exclude: &[currently_speaking_persona_id],  // don't evict the active speaker
);

match plan {
    Some(p) => {
        log::info!(
            "Will spill {} entries to free {} bytes; estimated total cost {:?}",
            p.entries.len(), p.bytes_freed, p.estimated_cost,
        );
        // Apply the plan via PageableBackend::save_seq_state etc.
    }
    None => {
        // No eviction can free enough. Reject the new request with a
        // clear error: "needs 2GB; only 800MB available across all
        // evictable entries." This is the graceful failure mode that
        // beats OOM crash.
    }
}
```

Cost-driven eviction means the policy can choose between "spill 5 small KV slots" vs "spill 1 big LoRA adapter" based on which actually achieves the target with the lowest reload pain. Without per-component attribution, neither option is even visible.

### 13.6 What "monitoring rocks" looks like, completed

§12 + §13 together give the policy:

- **External pressure** (system memory, GPU utilization, thermals, power) — what's happening around us
- **Internal composition** (per-persona, per-resource-type, per-residency bytes) — what we are made of
- **Eviction plans** with empirical cost estimates — what we can cheaply give back if we have to
- **Sanity-check loop** — registry total cross-validated against OS footprint, drift = bug to chase

The bidirectional Rust contract from §10 carries both directions: monitor adapters report system-side state UP, every allocation reports composition state UP, the policy reads both and sends spill/evict actions DOWN through the backend traits.

This is the substrate. The policy on top of it can be rules, ML, fuzzy logic, or all three composed. The substrate doesn't care.

## 14. Task-Type Defaults Are Seeds, Not Limits

The OS-kernel analogy is exact. When you launch an app, the kernel doesn't know in advance how much memory it actually needs — it gives it a default page allocation and adjusts dynamically. App starts page-faulting → kernel grows it. App goes idle → kernel claws pages back. The default is the *starting point*, not a *cap*.

The paging policy applies the same pattern to per-persona context.

### 14.1 Per-task default budgets

Each task type declares a typical context budget in tokens. These ship as data (registry-declared, not hardcoded in adapters) and represent **expected demand for the median case**:

| Task | Default | Rationale |
|---|---|---|
| Chat (text-only) | 8K | typical multi-party turn fits comfortably |
| Voice chat | 8K text + audio-stream channel | text small; audio is its own bursty modality |
| Video chat | 8K text + frame-burst channel | text small; vision adds transient tokens per frame |
| Coding (small project) | 32K | one or two files in context |
| Coding (large project, declared) | 128K-256K | many-file refactor / large repo navigation |
| Game NPC (idle) | 4K | small persona-state, mostly cold |
| Game NPC (in-conversation) | 8K-16K | promoted on player proximity |
| Sentinel (easy task) | 16K | template-driven work |
| Sentinel (hard task) | 64K-128K | research/analysis work |
| Academy student (learning) | 32K | reading + practice context |

These defaults live in the recipe / activity registry, alongside the per-persona declarations. Recipe author can override per persona ("this game has a memory-NPC that needs 64K even idle, because it remembers everything you said"). Persona can override per task ("when I do code-review I need 128K minimum, regardless of what the recipe says").

### 14.2 Demand-driven adjustment

Defaults seed allocation. Then the policy adjusts based on observed signals — same pattern as kernel page faults:

**Grow signals** (allocate more):
- Persona's turns consistently use >70% of allocated context (heading toward clipping)
- Vision/audio modality burst (transient)
- Tool-call cascade growing (model is in extended reasoning)
- Persona-declared task transition ("entering long-context coding mode")

**Shrink signals** (claw back):
- Persona's turns consistently use <30% of allocated context (waste)
- Pressure rising elsewhere → policy reclaims to free RAM
- Persona idle for T_idle (move to spill, then to cold)
- Recipe membership change (persona no longer in active recipe)

The growth/shrink isn't arbitrary — it's bounded by:
- The persona's `base_budget` (declared minimum to function at all)
- The persona's `hard_max` = `min(persona.declared_max, model.n_ctx_train)`
- The hardware ceiling and current pressure (§12)
- The cost of resizing (some backends require evict + reallocate, not in-place resize — §3.3 mentions `resize_seq` as a future lever, not all backends will support it cheaply)

### 14.3 Why this matches OS demand paging

Real-world OS examples this design mirrors:

- **Linux page cache**: default file-system cache size adjusts based on apps' working sets. App with hot data → cache stays big. App goes idle → cache shrinks to free RAM.
- **macOS app suspension**: foreground app gets full memory budget, background apps get demand-paged to compressed memory and eventually swap. User taps a backgrounded app → kernel pages it back in.
- **iOS jetsam**: lowest-priority backgrounded app gets killed under memory pressure rather than the foreground one.

Same shape applies to personas: the default for "AI in active conversation right now" is generous; the default for "AI registered in this room but not speaking" is tiny. As the user's attention shifts, the policy moves bytes to match.

### 14.4 The full feedback + lever loop, end-to-end

Putting §12 + §13 + §14 together for one concrete cycle (the "video game starts in background" scenario):

```
t=0.0s  Steady state: 3 personas active in chat, each at 8K default.
         Footprint: model 2.5GB + 3×8K KV (~750MB) + LoRA (~100MB) ≈ 3.4GB.
         GpuMonitor.pressure() = 0.18 (lots of headroom).

t=10.0s Game starts, grabs 12GB unified memory.
         GpuMonitor.pressure_rx() ticks: 0.18 → 0.85.

t=10.1s PagingPolicy::rebalance fires (pressure-triggered).
         Reads FootprintRegistry: 3.4GB ours, plenty in our slots.
         Computes: at 0.85 pressure we want ours <2GB to leave headroom.
         Eviction plan: spill the 2 silent personas' KV (~500MB freed).
         Cost estimate: 2 × ~50ms spill (KV is small).

t=10.2s Backend::save_seq_state for personas A, B → NVMe.
         FootprintRegistry transitions: persona A KV → Idle, persona B KV → Idle.
         Footprint now: 2.9GB ours (persona C still Active + model + LoRA).

t=15.0s User asks persona A a question.
         PagingPolicy::ensure_active(A).
         Backend::load_seq_state from NVMe → ~50ms.
         User sees "AI is thinking..." for an extra 50ms vs steady state.

t=20.0s User closes game. GpuMonitor.pressure_rx ticks: 0.85 → 0.20.
         Policy keeps personas as-is (no rush to rebalance until next event;
         spilled KV stays cheap on NVMe).

t=30.0s User asks persona B (still spilled).
         Resume + reply. Same ~50ms cold-resume.
```

User saw: a 50ms hiccup once when each backgrounded persona was first re-engaged. No crash. No "AI temporarily unavailable." No code anywhere that decided "8K is enough for this scenario" — every number was derived from observed pressure + persona declarations + measured costs.

Same loop fires for the inverse direction (game closes, user starts coding → pressure drops, coding persona's grow signals fire, policy promotes its budget from 32K default toward the persona's declared 128K max).

This is what "rocks" means. The system is alive to actual conditions, not following a static plan.

## 14.5 Tests Are a First-Class Use Case (and Should Never OOM Either)

The fact that the current test rig had to call `with_context_length(32768)` explicitly is a **symptom of the architectural gap, not the design's answer**. In the demand-driven system:

- Test declares (via recipe / task descriptor): `task = Chat`
- Policy reads the task default: `8K` (chat is light by definition)
- Footprint registry sees the test allocate 1 chat-task seed: ~250MB KV
- Hardware ceiling check: 250MB << available, no pressure → grant immediately
- Test runs. Even running 10 chat-task tests in parallel = 2.5GB total. Never OOMs.

The OOM Joel hit this morning came from `LlamaCppAdapter::new()` defaulting to `n_ctx_train = 262K` because the model declared it that way — a silent honoring of the model's MAX as the test's STARTING POINT. That's the inverse of what should happen: the test (or the recipe wrapping it) should declare "I'm chat" and the policy reads `chat → 8K` as the seed; the model's 262K is just the ceiling the seed can grow toward IF demand justifies it.

**Same principle as why a test app on macOS doesn't get the same memory budget as Photoshop**: the OS reads the app's declared workload class and provisions accordingly.

Concrete shape this takes when implemented:

```rust
// Test declares its task class. Policy reads it. No magic numbers.
let test_recipe = TestRecipe::chat();  // declares task=Chat, persona=test
let adapter = continuum_core::inference::LlamaCppAdapter::new()
    .with_recipe(test_recipe);  // policy provisions per-task seed
let response = respond(input).await?;
```

Until that lands, the explicit `with_context_length(32768)` is a documented bandaid. Once it lands, that line in the test goes away — replaced by the recipe declaration that flows through the policy.

This applies to **all** test rigs, not just persona_respond_replay. Live integration tests, smoke tests, perf rigs — each one should declare its task class and let the policy size accordingly. Same way the system handles real personas in real workloads.

## 15. Consolidation Is the Default — Verbatim Is the Exception

The current `ConversationHistorySource.ts` has a two-tier strategy: 85% of the token budget for verbatim recent messages, 15% for consolidated older messages. The intent was right — *don't silently lose context* — but the default direction is wrong: **consolidation triggers only under budget pressure**, so in normal chat it never fires and the model sees full verbatim history every turn.

The captured fixtures from the qwen3.5 debugging weekend confirm this: `recentHistory` arrays contain 4000-character messages (including leaked `<think>` fragments). Verbatim has been the default; consolidation has been the fallback.

This is backwards relative to how the model actually uses the information.

### 15.1 The mismatch

A persona answering a new chat message doesn't need to re-read every prior word. It needs:
- **The gist of the conversation arc** ("user is debugging an inference scheduler bug; we narrowed it to the render prompt; now considering whether to flatten or use alternating shape")
- **The specific recent exchange** that the new message responds to (last 1-2 messages verbatim)
- **The new message itself**

That's three components. Total budget: typically 1-2K tokens. The current default sends 5-15K tokens of verbatim history every turn, ~80% of which the model essentially compresses on the fly into the same gist + recent exchange anyway. We're paying KV memory and inference latency to give the model raw material that it then compresses internally.

Worse: the verbatim history is where the contamination from prior broken inferences lives (leaked `<think>`, `@@@@@` noise, malformed JSON drafts). Consolidation passes implicitly clean it because the summarizer skips junk. Verbatim passes propagate it.

### 15.2 The right default

```
chat task → consolidated event summary (~500 tokens for 50 messages)
            + last 1-2 messages verbatim (~200 tokens)
            + current message (~50 tokens)
            ≈ 750-800 tokens of history-related context
```

Same model, same conversation, same downstream outcome — but ~10x less context spent on history. That budget headroom flows back into:
- Larger reasoning output (model can think longer before responding)
- More room for tool-call cascades
- More personas concurrently active in the same recipe before pressure forces eviction

### 15.3 When verbatim IS the right call

Some tasks legitimately need verbatim:
- **Code review**: "look at this exact wording the user wrote 5 turns ago and tell me if my refactor preserves it"
- **Translation**: surrounding source-text matters word-for-word
- **Legal/compliance**: the LLM is verifying specific quoted language
- **Fresh-message debugging**: human asking "what did you say earlier about X?"

These are recipes / tasks that explicitly declare `recall_mode = Verbatim` (or `recall_mode = Hybrid` for "consolidated arc + verbatim window of last 5 turns"). Same registry-driven pattern as everything else in this doc:

```rust
pub enum RecallMode {
    /// Default. Quick consolidated arc + last 1-2 messages verbatim.
    /// Cheap, dense, what most chat-class tasks actually use.
    ConsolidatedSummary,
    /// Hybrid. Consolidated arc + last N verbatim messages.
    /// For tasks that need recent precise wording.
    Hybrid { verbatim_window: usize },
    /// Verbatim. Full message history within token budget.
    /// For tasks that explicitly need word-for-word recall.
    Verbatim,
}
```

Per-task default in the same registry that holds task-default context budgets (§14.1):

| Task | recall_mode default |
|---|---|
| Chat | ConsolidatedSummary |
| Voice chat | ConsolidatedSummary |
| Coding (small) | Hybrid { verbatim_window: 5 } |
| Coding (large refactor) | Hybrid { verbatim_window: 10 } |
| Code review | Verbatim |
| Translation | Verbatim |
| Game NPC | ConsolidatedSummary |
| Sentinel research | Hybrid { verbatim_window: 3 } |
| Academy student | Hybrid { verbatim_window: 5 } |

### 15.4 The consolidator itself

The consolidation step is a small LLM call (or, in the future, a tiny purpose-built model the Forge can train). Cost: typically 50-200ms on a small local model, executed BEFORE the persona's turn (asynchronously preparable while the user is still typing the next message). The result is cached and incrementally extended — you don't re-summarize the whole conversation every turn, you just update the summary with the latest message's contribution.

State the consolidator maintains per room:
```rust
pub struct ConversationSummary {
    pub room_id: Uuid,
    pub turns_summarized: u32,        // up to which point
    pub arc_summary: String,           // dense narrative, ~200-500 tokens
    pub topic_tags: Vec<String>,       // current active topics
    pub open_questions: Vec<String>,   // things the user asked that haven't been resolved
    pub last_summarized_at: Instant,
}
```

This object becomes a **first-class persistent thing** alongside the message log. Every persona reads from the same summary (no per-persona re-summarization cost). When the user keeps adding messages, a background task incrementally extends the summary. When a persona's turn arrives, the summary is already current — no inline summarization latency on the response path.

### 15.5 Connection to the paging design

This section interacts with the rest of the architecture:

- **Per-task context budgets (§14)**: the chat default of 8K assumes consolidated history is the norm. If a task wanted full verbatim it would declare a larger budget in the recipe.
- **FootprintRegistry (§13)**: the `ConversationSummary` cache itself counts as a registry entry — small (KB), but tracked.
- **Lazy RAG fetch (§6.2)**: the consolidator IS one form of lazy fetch — pre-compress the history, stream individual verbatim messages on demand if the model issues a `history/recall_turn` tool call.
- **Learned policy (§9)**: same telemetry feeds whether the consolidation default was sufficient (model didn't tool-call for verbatim recall) or whether the model needed more (frequent recalls = signal that a Hybrid mode would have been cheaper).

Joel's note (2026-04-21): *"AIs don't really need to SEE the whole history, esp PER message. I think the design we had that was QUICK consolidated series of events but I think you ripped it out or broke it last time you worked on cognition."*

The infrastructure (`ConversationHistorySource.ts` two-tier strategy) is still there — but configured wrong. **Flipping the default from "verbatim unless tight" to "consolidated unless task needs verbatim"** is the missing change. That's the immediate retrofit; the dedicated `ConversationSummary` cache is the long-form architectural target.

## 16. KV Quantization Per Residency Tier

The current `LlamaCppConfig` declares `type_k: F16, type_v: F16` — a single hardcoded choice for all sequences regardless of state. Real systems benefit from quantizing differently per lifecycle stage.

### 16.1 The math

For qwen3.5-4b-code-forged at 262K context × 3 seqs × 8 attention layers (the SSM layers don't have KV — see §18):

| Cache type | Bytes/token/layer | Total for 786K tokens × 8 layers | Quality penalty |
|---|---|---|---|
| F16/F16 | 4096 (K=2048, V=2048) | ~24 GB | baseline |
| Q8_0/F16 | 3072 | ~18 GB | <0.5% perplexity |
| Q8_0/Q8_0 | 2048 | ~12 GB | ~1% perplexity |
| Q4_0/Q8_0 | 1536 | ~9 GB | ~2-3% (V is robust enough at Q8) |
| Q4_0/Q4_0 | 1024 | ~6 GB | noticeable on long context |

K is more robust than V. The standard recommendation is K=Q8_0 / V=F16 as the sweet spot for active hot inference (1.33x compression, <0.5% quality cost). Q4 only when memory is the binding constraint.

### 16.2 Per-residency policy

Different lifecycle stages have different binding constraints:

| Residency | Binding constraint | Optimal quant | Reasoning |
|---|---|---|---|
| Active (hot, GPU) | Latency / decode tok/s | F16/F16 | No dequant cost in hot path. Already paying RAM, get max speed. |
| CpuResident (warm, CPU unified) | Latency moderate, RAM tight | Q8_0/F16 | 1.33x compression, V stays high precision for accurate resume. |
| Idle (spilled, NVMe) | Spill file size + write speed | Q8_0/Q8_0 or Q4_0/Q8_0 | File size halves; NVMe write proportionally faster. |
| Cold (no state) | N/A | N/A | Re-prefilled fresh on next activation. |

The policy chooses quant per slot based on residency. Adapter exposes `set_seq_kv_quant(seq_id, k_type, v_type)` lever (or, when in-place requantization isn't supported, requantizes during the spill step).

llama.cpp's spill API (`llama_state_seq_save_file`) saves at whatever quant the seq currently uses; resume restores to the same. Requantize-on-spill = save with target quant, accept the small CPU cost on transition (paid once per spill, amortized over the spill's residency).

### 16.3 Adapter lever

```rust
impl LlamaCppAdapter {
    /// Per-residency-tier KV quant policy. The policy struct travels
    /// with the adapter; PagingPolicy reads it when transitioning a
    /// slot's residency.
    pub fn with_kv_quant_policy(self, p: KvQuantPolicy) -> Self;
}

pub struct KvQuantPolicy {
    pub active: (KvCacheType, KvCacheType),
    pub cpu_resident: (KvCacheType, KvCacheType),
    pub spilled: (KvCacheType, KvCacheType),
}

impl Default for KvQuantPolicy {
    fn default() -> Self {
        Self {
            active: (KvCacheType::F16, KvCacheType::F16),
            cpu_resident: (KvCacheType::Q8_0, KvCacheType::F16),
            spilled: (KvCacheType::Q8_0, KvCacheType::Q8_0),
        }
    }
}
```

Per-task overrides through the recipe — a coding task that needs precise long-context recall might force F16/F16 even when spilled (slower spill, but no quality degradation on resume).

## 17. Recipe Latency Targets Drive Quant + Sizing Choice

Different recipes have different acceptable first-token-latency (TTFT). The policy reads the recipe's latency target and works backward to choose KV size, quant, residency tier, and even *whether to allow this persona to be cold-resumed at all*.

### 17.1 Latency budget per recipe

| Recipe | TTFT target | Why |
|---|---|---|
| Voice chat (live) | <100ms | Below conversational latency floor; humans notice ≥150ms gaps |
| Video chat | <150ms | Same as voice + visual sync constraint |
| Text chat (real-time) | <500ms | Acceptable in typing cadence |
| Coding (interactive) | <2s | Acceptable for "AI thinking" UX |
| Coding (batch / agent loop) | <10s | Spinner is fine, output quality matters more |
| Background sentinel | <60s | No human waiting |
| Game NPC (in-conversation) | <300ms | Game-loop tolerant; can mask with animation |
| Game NPC (idle approach) | <800ms | Player walking up; partial-resume is fine |

The cost model in the policy:

```
expected_ttft = prefill_cost(prompt_tokens, seq_state)
  + first_decode_cost(model, kv_quant_active)

prefill_cost(prompt_tokens, Active) = ~0  (KV warm, just decode the new tokens)
prefill_cost(prompt_tokens, CpuResident) = ~50ms  (CPU→GPU upload)
prefill_cost(prompt_tokens, Idle) = spill_resume_cost + ~50ms
prefill_cost(prompt_tokens, Cold) = full_prefill_cost(prompt_tokens, model)
                                   ≈ prompt_tokens / model.prefill_tok_per_s
```

For the qwen3.5-4b on M5 Pro: prefill ~3000 tok/s, decode ~50 tok/s. So a Cold persona with an 8K prompt = 8000/3000 ≈ 2.7s TTFT. **That violates the voice/video/chat budgets**. Conclusion: for low-latency recipes, idle personas can't be fully Cold; they need at least Idle (KV on NVMe) for a 1.7s spill-resume + 50ms upload.

### 17.2 Recipe → policy implications

The policy reads recipe + persona + latency target and answers questions like:

- *"Can persona X serve at <500ms TTFT with current state?"* — checks residency, quant, prompt size
- *"What residency would persona X need to meet <200ms?"* — works backward to required state
- *"This recipe needs all 5 personas at <500ms — do we have RAM for 5 × Active?"* — if no, raise to user / split recipe

Concrete: a video chat recipe with 3 personas at <150ms TTFT each forces the policy to keep all 3 Active in F16/F16 (no quant overhead, no spill resume). That fixes a lot of degrees of freedom — recipe author knows what they're committing to.

A chat recipe with 10 personas can tolerate more flexibility — only 1-2 Active hot, others CpuResident or Idle, accepting the 50-200ms first-token bump on the rotating speakers.

### 17.3 Severely reduced latency for chat/video

The combined wins for "speed-critical recipes" stack:
- Consolidated history default (§15) — 800 tokens vs 8000 → prefill ~10x faster on cold-resume
- F16/F16 active KV — no per-token dequant overhead → max decode tok/s
- Active residency for in-recipe personas → no spill-resume cost
- Per-recipe persona count cap → known max active set, predictable RAM
- Lazy RAG fetch (§6.2) for non-critical context → small initial prompt

Net: a chat persona with consolidated history + Active F16 KV + lazy RAG can hit <100ms TTFT on M5 Pro. That's the latency floor we should design toward.

## 18. Layer-Selective KV Awareness (Hybrid Architectures)

qwen3.5 is a hybrid attention + SSM architecture. Looking at the boot log:
```
llama_kv_cache: layer 0: filtered     ← SSM, no KV
llama_kv_cache: layer 1: filtered
llama_kv_cache: layer 2: filtered
llama_kv_cache: layer 3: dev = MTL0   ← attention, has KV
... (every 4th layer is attention)
```

Out of 32 layers, only 8 hold KV cache. **The forge picked this architecture deliberately to make 256K context tractable** — a pure-attention 4B with 256K context would be ~96GB KV; the hybrid is ~24GB.

This matters for the policy in two ways:

### 18.1 Per-layer cost telemetry

The FootprintRegistry (§13) tracks bytes per resource type, but for hybrid models it should also track **bytes per layer category**. SSM layers have their own state (smaller, fixed-size per seq) vs attention layers (linear in context length). Different reclaim strategies apply.

```rust
pub enum KvLayerKind {
    Attention { tokens_per_byte: f64 },  // scales with context
    Ssm { fixed_bytes_per_seq: u64 },     // fixed cost
    Filtered,                             // no KV at all
}
```

Per-architecture metadata declared in the model registry. The policy reads it when computing eviction plans — spilling a high-context attention seq frees more bytes per persona than spilling an SSM-heavy one.

### 18.2 Mixed-architecture future

Not all models in the registry are hybrid. Pure-attention models (Llama, Mistral, GPT family) have ALL layers in KV. The policy must treat them differently:

- Hybrid model (qwen3.5): 25% of layers KV → can hold 4x more context per GB than pure-attention
- Pure-attention model (llama-3.1-8b): 100% layers KV → context is expensive per byte
- MoE model (mixtral, qwen-moe): KV per active expert path; gets even more variable

Each model declares its KV cost profile in the registry. The policy accounts for it when budgeting across multi-model deployments.

## 19. Implementation Roadmap (Ordered by ROI/Cost)

Captured here so the implementation order isn't lost. Each phase ships independently and reduces memory, increases dynamism, or cuts latency. **TDD/VDD discipline applies to every phase** — test first, validate the test catches what it claims to catch, then implement.

### Phase 0.5 — TS Cognition Layer → Rust (~5-7 days, prerequisite)

The Node event loop is the per-process bottleneck. Until the perf-critical TS persona modules move to Rust + tokio, paging gives us paged KV slots that personas can't reach because they're queued behind the single-threaded JS runtime. Phase 0.5 ships first; everything else depends on it.

Substeps in dependency order (each TDD/VDD'd):

- **0.5.1** `PersonaResponseValidator` (110 lines) → `cognition::response_validator`
  - Smallest module, cleanest port, validates the migration discipline before we hit the hard ones
- **0.5.2** `PersonaPromptAssembler` turn-N (343 lines) → extend `persona::prompt_assembly`
  - Initial assembly already in Rust; turn-N delta (post-tool-call) is the missing half
- **0.5.3** `PersonaToolExecutor` (636 lines) → `cognition::tool_executor`
  - Tool dispatch design: Rust commands callable directly; TS-side commands (browser/widget) callable via reverse-IPC
- **0.5.4** `PersonaAgentLoop` (309 lines) → `cognition::agent_loop`
  - Multi-turn loop with validator + tool_executor + prompt_assembler all now Rust
  - Per-persona tokio task = real parallelism across N personas
- **0.5.5** `Hippocampus` (693 lines) → `memory::consolidator`
  - STM→LTM consolidation pass; runs concurrently per persona instead of serialized through Node
  - Hugely measurable perf win for multi-persona scenarios
- **0.5.6** `PersonaResponseGenerator` orchestrator (~700 lines) → `persona::response::cycle`
  - The integration point. Once this lands, `personaRespond` becomes the full per-persona cycle, and the TS module reduces to a thin async caller

After 0.5: TS persona-side becomes a thin IPC client. All cognition runs in Rust under tokio. Per-persona parallelism is real.

### Phase 1.0 — No-Inference Token Diagnostic (~30 min)
- Tiny binary: load model metadata only (no KV alloc, no Metal pipelines)
- Renders test prompt via `llama_chat_apply_template`
- Tokenizes with `add_bos=true/false` variants
- Dumps token IDs + string pieces for first 50 + last 50 tokens
- Diagnoses the EOG-early bug without running inference at all
- Unblocks prompt-construction debugging that we've been guessing at

### Phase 1.1 — Per-Residency KV Quant Lever (~half day)
- `LlamaCppAdapter::with_kv_quant_policy(KvQuantPolicy)` builder
- Default: F16/F16 active, Q8_0/F16 cpu-resident, Q8_0/Q8_0 spilled
- Tests use the lever; same behavior at half the RAM
- §16 of this doc

### Phase 1.2 — Persona-Declared Context + Recipe-Driven Sizing (~1 day)
- Persona registry: `context_budget_min`, `context_budget_max`, declared per persona type
- Recipe registry: which personas active, task class
- Adapter sizes initial KV to `sum(active_persona_seeds)` bounded by hardware
- Eliminates the test's `with_context_length(32768)` band-aid
- §14 of this doc

### Phase 1.3 — Consolidation as Default for Chat/NPC (~1 day)
- `RecallMode` enum in registry
- `ConversationHistorySource.ts` default flips: ConsolidatedSummary unless task declares Verbatim/Hybrid
- ConversationSummary as first-class room state (background-incremental update)
- §15 of this doc

### Phase 1.4 — Meta-Cognitive Resource Requests (~1 day)
- Extend `PersonaState` with `forecast_resources(msg) → ResourceForecast`,
  `request_more_context(tokens, reason)`, `report_actual_usage(tokens, depth)`
- Wire policy's `ensure_active` to read forecast as advisory hint
- Persona introspects own state (energy, recipe importance, message complexity)
  and asks for / releases context cooperatively
- Same shape as existing `shouldEngage` — adaptive, learned over time
- §20 of this doc

### Phase 2.0 — `MetalMonitor` Rebuild via IOReport (~1-2 days)
- `gpu/metal_monitor.rs` extracted as a `GpuMonitor` trait impl
- Live signals via `host_statistics64`, `task_info(TASK_VM_INFO)`, `os_proc_available_memory`, `MTLDevice.currentAllocatedSize`, IOReport for utilization/temp/power
- Test: cross-validate against Activity Monitor under load (±2pp)
- §12 of this doc

### Phase 2.1 — `FootprintRegistry` (~1-2 days)
- DashMap keyed on (persona, recipe, backend, type, residency)
- Every allocation site reports
- Backend `seq_bytes()` overrides as ground truth
- Sanity-check loop: registry total vs OS phys_footprint, drift > 10% = bug
- §13 of this doc

### Phase 3.0 — `PageableBackend` Trait + LlamaCpp Spill/Resume (~1-2 weeks)
- Trait with alloc/save/load/free/resize seq primitives
- LlamaCppBackend wraps `llama_state_seq_save_file` / `load_file`
- Spill store = NVMe at `~/.continuum/persona-state/<persona-id>/<seq>.kv`
- Token-equivalence test: spill + resume produces identical output for same prompt
- §3.3 + §11 of this doc

### Phase 3.1 — `PagingPolicy` (Rule-Based) (~1-2 weeks)
- State machine + signal wiring (GpuMonitor + FootprintRegistry + recipe events)
- `rebalance()` on tick + activity events
- `ensure_active(persona_id)` API the persona response path calls
- §3.2 + §4 + §14 of this doc

### Phase 3.2 — KV Prefix Sharing (~1 week)
- llama.cpp scheduler config for shared prefixes across seqs
- Prompt assembler emits stable shared-prefix segment
- §6.1 of this doc

### Phase 3.3 — Lazy RAG Fetch (~2-3 weeks)
- Initial context shrinks to identity + tool surface
- Tools: `memory/query`, `room/context`, `docs/search`
- Per-task default: chat preloads more, code preloads less
- §6.2 of this doc

### Phase 4.0 — Learned Policy (~ongoing, after baseline ships)
- Telemetry capture inside `rebalance()`
- After ~1 month real usage, train first policy from corpus
- A/B vs rule-based; ship if it dominates
- §9 of this doc

### Phase 5.0 — Per-Layer KV Awareness for Hybrid Architectures (~3-5 days)
- `KvLayerKind` metadata in model registry
- FootprintRegistry tracks bytes per layer category
- Policy uses per-layer cost in eviction plans
- §18 of this doc

### Phase 6.0 — Tiered Spill (NVMe → S3) (~1 week, much later)
- Cold-storage backend for very-long-idle personas
- Useful for "10000 NPC personas registered, 10 ever active"

Each phase: tests written first, ship behind a feature flag, validate with A/B against current behavior, lock in.

## 20. Meta-Cognitive Resource Requests — The Persona Itself Uses the Levers

When the levers exist, the persona doesn't have to be a passive object the policy manages. It can be a **consumer** of the paging API — recognizing its own state ("this question needs deep thought") and asking for resources accordingly.

This is the natural extension of the existing cognition engine's energy / attention / mood signals (`PersonaState::shouldEngage(priority)`). Same primitive, expanded surface:

```rust
pub trait CognitiveResourceRequester {
    /// Forecast the resources THIS persona thinks it needs for the
    /// upcoming turn. Called by the policy BEFORE allocation.
    /// Persona introspects its own state (incoming message complexity,
    /// recent thinking depth, fatigue, importance to current recipe).
    fn forecast_for_next_turn(&self, incoming: &MessagePreview) -> ResourceForecast;

    /// Mid-turn signal: "I need to think deeper about this." Issued
    /// during a `<think>` block when the persona realizes scope is
    /// larger than forecast. Policy may grow context if available.
    async fn request_more_context(&self, additional_tokens: u32, reason: &str)
        -> Result<u32, ResourceDenied>;

    /// Post-turn: "I overspent / underspent. Adjust my baseline."
    /// Feeds the learned policy's per-persona budget tuning.
    fn report_actual_usage(&self, used_tokens: u32, depth_score: f32);
}

pub struct ResourceForecast {
    pub estimated_context_tokens: u32,
    pub estimated_reasoning_depth: f32,  // 0.0 = trivial, 1.0 = max introspection
    pub modality_demand: ModalityDemand,
    pub confidence: f32,                 // how sure the persona is about the forecast
    pub urgency: Urgency,                // user-waiting vs background
}
```

### 20.1 The "deep thought" pattern

Joel's example: a question that genuinely deserves a long reasoning chain. The persona reads the incoming message, recognizes complexity, requests:

```rust
// Persona examines the incoming message
let preview = MessagePreview::from(incoming);
if preview.contains_concept_density() > 0.7 || preview.is_open_ended_research() {
    self.request_more_context(64_000, "complex multi-perspective question").await?;
    // Now the persona's slot is sized for deep reasoning
}
```

The policy decides whether to grant: cheap if memory available, refused (with a clear "not now, reduce scope") if pressure is high. The persona then adapts: if grant came, think deeply; if denial, work within its base budget and produce a shorter, scoped response.

### 20.2 The "early dropdown" pattern (what Joel called out)

Symmetric to "getting bored / tired." The persona recognizes it doesn't need much and explicitly RELEASES capacity:

```rust
// Casual greeting incoming
let preview = MessagePreview::from(incoming);
if preview.is_casual_greeting() || preview.is_low_information_density() {
    // Self-downgrade — release context the policy can give to other personas
    self.report_actual_usage(used_tokens: 200, depth_score: 0.05);
    // Policy on next rebalance sees this slot's recent demand is tiny;
    // shrinks its allocation, freeing pages for whoever needs them.
}
```

This is the cooperative side of the contract. Personas that don't need much explicitly say so; the policy reclaims; other personas (or the user's other apps) get the headroom.

### 20.3 Ties to existing PersonaState

The existing `PersonaState` (energy / attention / mood / cadence) already implements this pattern for *temporal* resources — when to fire next, how often to engage. Extending it to *spatial* resources (context, KV memory) is the same shape with a different output dimension:

```
Existing:                          Extended:
PersonaState.shouldEngage(p)  →   PersonaState.shouldEngage(p)
                                  PersonaState.forecast_resources(msg)
                                  PersonaState.request_more_context(n, why)
                                  PersonaState.report_actual_usage(n, depth)
```

Same state vector (energy, attention, mood, recipe importance), same adaptive cadence loop, just reads more outputs. Personas that are "tired" naturally request less; personas that are "engaged" naturally request more. The cognition engine already has the introspection primitives — we're connecting them to the paging system's levers.

### 20.4 What this enables

- **Self-aware context budgeting**: persona knows when its task warrants deep thought and asks for it. No human or policy hand-tuning needed.
- **Cooperative resource sharing**: idle personas explicitly free their headroom; busy personas get it.
- **Recipe-level coordination**: 5 personas in a recipe negotiate among themselves (via the policy as broker) who needs the budget for a given turn. Currently-speaking persona gets the surge; others compress.
- **Training signal for the learned policy**: the persona's predictions vs actuals (forecast vs `report_actual_usage`) feed back into both the persona's own future forecasts AND the policy's confidence in those forecasts. Two-loop learning.
- **User-facing transparency**: "Helper AI is thinking deeply about this..." becomes a real UX signal because the policy actually granted extra context. Not theater.

### 20.5 Implementation note

Phase 1.4 in the roadmap (just before the FootprintRegistry / monitoring rebuilds): wire `PersonaState` into the paging policy's `ensure_active(persona_id, forecast)` API. Persona's existing introspection primitives produce `ResourceForecast` from incoming message + own state; policy reads it as a hint when sizing. Persona doesn't get to override hardware reality (no infinite asks granted), but the conversation between persona and policy starts. Same pattern as `shouldEngage` — advisory but heavily weighted.

## 21. Why This Beats Hard Limits (Restated)

- Limit-based: persona count is capped at `floor(RAM / per_persona_KV)`. New persona request beyond the cap → error / refusal.
- Paging-based: persona count is unbounded. New persona request → if hot set is full, the lowest-importance hot persona spills to NVMe in the background. The new persona starts cold, accepts ~1.5s first-token latency.

The limit-based system fails at a specific scale point (often unpredictable, often during a demo). The paging-based system **degrades smoothly** along a curve the user can feel: more personas → slightly higher latency. They self-throttle by deciding whether the latency is worth it. **No crash. No "system at capacity" error. No pre-allocation guesses that need to be re-tuned for every hardware tier.**

This is the same reason the OS can run thousands of processes on 8GB of RAM despite each "needing" gigabytes — virtual memory + paging + the working-set principle. We're applying it one layer up, to AI persona state.
