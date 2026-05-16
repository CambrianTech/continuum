# Genome, Foundry, Sentinel-AI: The Artifact-Sharing Economy On Consumer Hardware

> **Substrate contract:** [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the runtime contract every Rust concern inherits. This document specifies the *artifact economy* that flows on top of that contract.
> **Lane-shaped roadmap:** [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — implementation lands per Lane H (Substrate Governor + Tiered Genome Cache) once the design here is reviewed.
> **Status:** design proposal. No code in this document; every API shape shown is a proposed Rust trait targeted at `src/workers/continuum-core/src/genome/`, `foundry/`, and `sentinel/`.

## Why This Document Exists

Continuum needs personas that **evolve**. Evolution happens through the **demand-aligned flow** of shared artifacts — commands, modules, personas, LoRA layers (with their MoE experts), long-term LoRA layers, and engrams — across the hive. The substrate that makes this real has to work on a MacBook Air (16 GB unified memory) and an RTX 5090 (32 GB VRAM + 64 GB system RAM) with the *same code path* — only the governor settings differ.

The architecture that achieves both is the same architecture seen from two sides:

- **The autonomy side**: an artifact-sharing economy. Personas are first-class entities; the genome is the shared substrate of evolved weights; the foundry brings in what others built; sentinel-AI refines what we lived; demand alignment is the routing principle.
- **The efficiency side**: a classical computer-architecture toolbox. Persona = process. Genome = cache hierarchy. Engrams = paged virtual memory. Foundry = JIT compiler. Sentinel-AI = profile-guided optimizer. Substrate governor = DVFS.

These are not two designs to merge later. They are one design seen from two angles. Any change to one half must be reflected in the other.

This document specifies the substrate primitives, the Rust trait shapes, the hardware anchors, the lifecycle, and the acceptance criteria. It is written so that the next engineer can read it and start landing types in `continuum-core` without first writing more docs.

## The Synthesis In One Diagram

```text
                ┌──────────────────────────────────────────────────────────────┐
                │                       THE HIVE                                │
                │   (N personas, M instances, potentially global federation)    │
                └─────────────────────────────────┬────────────────────────────┘
                                                  │ demand-aligned recall
                                                  ▼
                ┌──────────────────────────────────────────────────────────────┐
                │                     GENOME POOL                               │
                │      (the shared substrate of evolved weights + memory)       │
                │                                                               │
                │   ┌────────────┐    ┌────────────┐    ┌─────────────────┐    │
                │   │  Imported  │    │  Refined   │    │     Engrams     │    │
                │   │ (foundry-  │    │ (sentinel- │    │  (longterm.db,  │    │
                │   │  adapted   │    │  derived,  │    │   experiential  │    │
                │   │   SOTA)    │    │   lived)   │    │     memory)     │    │
                │   └──────▲─────┘    └──────▲─────┘    └────────▲────────┘    │
                └──────────│─────────────────│───────────────────│─────────────┘
                           │ writes          │ writes            │ writes
                ┌──────────┴───────┐ ┌───────┴────────┐ ┌────────┴─────────────┐
                │     FOUNDRY      │ │   SENTINEL-AI  │ │   CONSOLIDATION       │
                │   (the JIT —     │ │  (the profile- │ │  (sleep phase —       │
                │  absorbs Qwen /  │ │   guided       │ │   traces become       │
                │  other SOTA into │ │   optimizer —  │ │   engrams; engrams    │
                │  our format,     │ │   observes     │ │   indexed; cold       │
                │  publishes with  │ │   outcomes,    │ │   pages archived)     │
                │  provenance)     │ │   refines)     │ │                       │
                └──────────────────┘ └──────▲─────────┘ └───────────────────────┘
                                            │ traces + outcomes
                                            │
                ┌───────────────────────────┴──────────────────────────────────┐
                │                  PERSONA WORKING SETS                         │
                │       (per-persona compartmentalized, share genome)           │
                │                                                               │
                │   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐         │
                │   │ L1 hot  │  │ L1 hot  │  │ L1 hot  │  │ L1 hot  │         │
                │   │ L2 warm │  │ L2 warm │  │ L2 warm │  │ L2 warm │         │
                │   │ L3 RAM  │  │ L3 RAM  │  │ L3 RAM  │  │ L3 RAM  │         │
                │   └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘         │
                │        ▲            ▲            ▲            ▲              │
                │        └────────────┴────── page faults / pre-fetch ─┘       │
                │                       from L4 (SSD genome) / L5 (cold)       │
                └────────────────────────────▲─────────────────────────────────┘
                                             │
                                             │ all of the above is governed by:
                                             │
                ┌────────────────────────────┴─────────────────────────────────┐
                │                    SUBSTRATE GOVERNOR                          │
                │     (DVFS for AI — detects hardware class, scales tier         │
                │      sizes, cadences, concurrency caps, speculation            │
                │      aggressiveness, consolidation schedule)                   │
                │                                                                │
                │     MacBook Air (16GB UMA)  ◄────────────► RTX 5090 (32+64GB)  │
                │     identical Rust code; different governor policy file        │
                └────────────────────────────────────────────────────────────────┘
```

Every box in this diagram is a Rust subsystem with a typed boundary. The arrows are flows of typed artifacts. The governor is the single source of truth for "how big" / "how fast" / "how aggressive."

## Part 1: Artifact Taxonomy

Six durable artifact kinds flow through the genome pool. A seventh, transient kind, lives in the cache.

| # | Artifact | Creator | Adopter | Refinement | Provenance |
|---|---|---|---|---|---|
| 1 | **Command** | continuum-core + module authors | every persona that calls the command | hot commands get specialized fast paths during sleep | author + version |
| 2 | **Module** | engineers, scaffold generator | any cell registering with the runtime | sentinel can suggest module composition patterns; humans land them | engineer + commit |
| 3 | **Persona** | user (via room creation) or another persona (via spawn) | the room; cross-room invocation by handle | sentinel refines persona's private LoRA + engrams from its traces | creator + lineage |
| 4 | **LoRA layer** | foundry (imported) or sentinel (refined) or persona (private experimentation) | any persona via demand-aligned recall | sentinel re-refines hot layers from outcomes; foundry re-adapts when source SOTA updates | full chain — source SOTA → extraction → adaptation → refinement history |
| 5 | **MoE expert** | foundry (imported) or sentinel (refined) | any persona's MoE routing table | sentinel observes which experts fire for good outcomes, re-routes | inherits from parent LoRA layer |
| 6 | **Engram** | consolidation phase (from traces) or persona (explicit memory write) | the recalling persona; sentinel as training input | sentinel-derived clusters of engrams produce refined LoRA | trace ref + persona + time |

The seventh, transient:

7. **Composition state** — the dynamic LoRA stack + MoE routing + KV cache + engram-bound context that constitutes a persona's *currently-running* form. Not a stored artifact; recomputed from the genome pool on demand and cached at L1/L2. Lives only as long as it's hot.

### Provenance Is Mandatory

Every durable artifact carries a typed `Provenance` record. The substrate refuses to accept artifacts without one. Provenance is what makes trust auditable, refinement reversible, and sharing safe.

```rust
// PROPOSED — Lane H deliverable, targeted at src/workers/continuum-core/src/genome/provenance.rs
pub struct Provenance {
    pub artifact_id: ArtifactId,                  // content hash
    pub created_at: SystemTime,
    pub creator: Creator,                          // Foundry | Sentinel | Persona | Human
    pub source_trace: Vec<TraceRef>,               // traces this was derived from (empty for imports)
    pub source_artifact: Vec<ArtifactRef>,         // upstream artifacts (e.g. base SOTA for foundry imports)
    pub supersedes: Option<ArtifactRef>,           // previous version, if any
    pub adaptation_method: AdaptationMethod,       // None | ExtractionAndQuantize | LoRARefine | EngramCluster | ...
    pub outcome_metrics: Option<OutcomeMetrics>,   // attached when sentinel proves the artifact improves outcomes
    pub trust_score: TrustScore,                   // composed from the rest
    pub license: License,                          // inherited from source SOTA, or local
}
```

If the substrate cannot answer "where did this LoRA layer come from and what proof do we have it works", the artifact is not in the pool. This is what `no_silent_fallback` looks like at the artifact economy layer.

## Part 2: Cache Hierarchy

Five tiers. Each is a real Rust storage backend with a real eviction policy. The policy *is* tunable per hardware class; the *code* is not.

```rust
// PROPOSED — src/workers/continuum-core/src/genome/tier.rs
pub enum Tier {
    L1,  // accelerator-resident, currently in inference (KV + active LoRA)
    L2,  // accelerator-warm, recently used, not active
    L3,  // system RAM, fast to promote to L1/L2
    L4,  // SSD-resident genome pool (all adapted/refined artifacts)
    L5,  // cold archive in longterm.db (compressed engrams, retired layers, audit)
}

pub trait TierStore: Send + Sync {
    fn tier(&self) -> Tier;
    async fn read(&self, page: PageRef) -> Result<PageHandle, TierError>;
    async fn write(&self, page: PageRef, blob: ArtifactBlob, prov: Provenance) -> Result<(), TierError>;
    async fn evict(&self, target_free_bytes: usize) -> Vec<EvictionRecord>;
    fn capacity(&self) -> TierCapacity;            // current_used, configured_limit
    fn observe_access(&self, page: PageRef);        // updates LRU/LFU state
}
```

### Eviction Policy Per Tier

| Tier | Policy | When eviction fires |
|---|---|---|
| L1 | LRU within current turn | sub-step needs a layer not resident |
| L2 | LRU across last N turns (governor sets N; default 100) | L1 spill |
| L3 | LFU + recency; broad-use layers get retention bonus | L2 spill |
| L4 | Demand-aligned with sentinel-refined preference (refined wins ties over imported) | L3 spill |
| L5 | Append-only with provenance preserved; GC only during sleep | never in hot path |

Eviction is *always* typed: every evicted page emits an `EvictionRecord` to the trace bus. Recurring evictions of the same page across turns are exactly the signal sentinel uses to upgrade the page's tier policy.

### Hardware Anchors

Two anchor configurations; everything else interpolates. The substrate *detects* the hardware class at boot (silicon + VRAM + system RAM + power source + thermal class) and the governor writes the appropriate policy.

| | **MacBook Air, M-series, 16 GB unified** | **RTX 5090, 32 GB VRAM + 64 GB system RAM** |
|---|---|---|
| L1 (accelerator-resident) | 1–2 LoRA layers; 1–2k KV tokens | 6–8 LoRA layers; 16k+ KV tokens |
| L2 (accelerator-warm) | 2–4 layers | 12–16 layers |
| L3 (system RAM) | 8–12 layers; ~1k engrams | 40+ layers; ~10k engrams |
| L4 (SSD genome) | bounded by disk | bounded by disk |
| Concurrent personas | 1–2 | 6–8 |
| Speculative composition | conservative (only on idle slack) | aggressive (every turn) |
| Sleep / consolidation cadence | nightly, opportunistic on idle/plugged-in | nightly + partial during day |
| Cross-instance federation pull | manual / explicit | automatic on idle |

M-Pro/Max interpolate to mid-range. Vulkan-only AMD/Intel match the Air shape with smaller L1 (less unified memory). Vision Pro and embedded targets get aggressive eviction + reduced concurrency + simpler composition. **The Rust code is identical across all of them.** This is the architectural beauty: the same primitives, parameterized.

## Part 3: Paging, Working Set, And Page Faults

A persona's `WorkingSet` is the set of pages currently hot in L1+L2 for that persona. Pages can be LoRA layer pages, MoE expert pages, KV cache pages, or engram pages.

```rust
// PROPOSED — src/workers/continuum-core/src/genome/working_set.rs
pub struct WorkingSet {
    pub persona: PersonaId,
    pub pages: HashMap<PageRef, ResidentPage>,
    pub capacity: WorkingSetCapacity,              // from governor
    pub last_composition: Option<CompositionPlan>,
}

pub struct ResidentPage {
    pub page: PageRef,
    pub tier: Tier,                                // L1 or L2
    pub last_access: Instant,
    pub access_count_window: u32,
    pub pinned: bool,                              // composition-pinned pages cannot evict mid-turn
}

pub enum PageKind { LoRALayer, MoEExpert, KVCache, Engram }

pub struct PageRef {
    pub kind: PageKind,
    pub artifact: ArtifactId,
    pub offset: PageOffset,                        // for sub-artifact paging (MoE experts, KV chunks)
}
```

When the persona's composition needs a page not in its working set, that's a **page fault**:

```rust
pub trait WorkingSetManager: Send + Sync {
    /// Promote a page into this persona's working set. May trigger eviction.
    async fn page_in(&self, persona: PersonaId, page: PageRef) -> Result<PageHandle, PageFault>;

    /// Demote a page out of the working set toward the named tier.
    async fn page_out(&self, persona: PersonaId, page: PageRef, to: Tier) -> Result<(), TierError>;

    /// Current working set for read-only inspection.
    fn working_set(&self, persona: PersonaId) -> &WorkingSet;

    /// Enforced MMU-style audit: persona is asking for a page.
    /// Returns AccessDenied if the page is private to another persona.
    fn audit_access(&self, persona: PersonaId, page: PageRef) -> Result<(), AccessDenied>;
}

pub struct PageFault {
    pub page: PageRef,
    pub from_tier: Option<Tier>,                   // None = true cold miss (page does not exist yet)
    pub to_tier: Tier,
    pub persona: PersonaId,
    pub elapsed_us: u64,
    pub eviction_cost: Option<EvictionRecord>,     // what got evicted to make room
}
```

Page faults are **typed events** on the trace bus. Sentinel observes them. A persona that page-faults on the same page across many turns is a signal to either pre-fetch that page (raise speculation aggressiveness for it) or upgrade its tier policy (pin it higher in the working set).

This is the substrate's main observability signal for "this persona's working set doesn't match what we're allocating." It is the difference between a substrate that knows what's wrong and one that doesn't.

## Part 4: Compartmentalization

Personas are processes. Each has:

- An independent inbox (per the CBAR-SUBSTRATE "Persona-cognition invariants")
- An independent KV cache
- An independent `WorkingSet`
- An independent composition state
- An independent mood / energy / cadence state
- An independent private engram region

The **genome pool is a shared library** mapped read-only into every persona's address space. Write access is segmented:

| Region | Foundry | Sentinel-AI | Persona (self) | Persona (other) |
|---|---|---|---|---|
| Imported (foundry-adapted) | write | read | read | read |
| Refined (sentinel-derived) | read | write | read | read |
| Own private engrams | read | read (training only, opt-in) | write | none |
| Own private LoRA experiments | read | read (training only, opt-in) | write | none |
| Other persona's private | none | read (training only, opt-in) | none | none |

```rust
pub trait WorkingSetManager {
    // ... continues from above
    /// Enforce MMU-style permissions. Returns typed AccessDenied with full context
    /// — never silently succeeds, never silently fails.
    fn check_permission(
        &self,
        actor: ActorId,
        region: GenomeRegion,
        op: Op,
    ) -> Result<(), AccessDenied>;
}
```

`AccessDenied` is loud. Audit log captures it. This is how the substrate makes per-persona privacy structural rather than policy.

## Part 5: Foundry — JIT For Models

The foundry is the only substrate component that *imports* artifacts from outside Continuum. It is the JIT in the same sense that Java's HotSpot is a JIT: it compiles the *source* (SOTA model) into the *binary* (our adapted format) that the runtime actually executes.

```rust
// PROPOSED — src/workers/continuum-core/src/foundry/mod.rs
pub trait Foundry: Send + Sync {
    /// Pull a SOTA source and extract useful artifacts.
    /// Runs out-of-band; never blocks any persona's hot path.
    async fn absorb(&self, source: &SOTASource) -> Result<AbsorptionReport, FoundryError>;

    /// Iterate over imported artifacts published by this foundry.
    fn iter_imports(&self) -> Box<dyn Iterator<Item = ImportedArtifact> + '_>;

    /// Re-absorb when the source SOTA updates; emits supersession records.
    async fn refresh(&self, source: &SOTASource) -> Result<AbsorptionReport, FoundryError>;
}

pub struct SOTASource {
    pub model: ModelIdentifier,                    // qwen3-32b-instruct, mistral-large, ...
    pub version: String,
    pub fetch: FetchMethod,                        // HF | local file | API | ...
    pub license: License,
    pub trust_class: TrustClass,                   // open-weight | foundation-vendor | community | ...
}

pub struct ImportedArtifact {
    pub kind: ImportedKind,                        // BaseModel | LoRALayer | MoEExpert | EmbeddingShard | ...
    pub source: SOTASource,
    pub extraction: ExtractionMethod,              // FullModel | LayerSubset | ExpertExtraction | DistillationTarget
    pub format: ContinuumArtifactFormat,           // our quantization + LoRA-on-base shape
    pub blob: ArtifactBlob,
    pub provenance: Provenance,
}
```

The foundry does five things:

1. **Acquisition** — pull SOTA model weights (Qwen, Mistral, others, future).
2. **Extraction** — pull only the parts the genome needs. Not the whole model; specific layers, specific experts, specific embedding shards.
3. **Adaptation** — quantize for our hardware classes; shape into LoRA-on-base; ensure compatibility with the base + composition layer.
4. **Provenance** — every output artifact gets metadata: which SOTA, which version, which extraction method, what license, what trust class.
5. **Publication** — the adapted artifact lands in the *imported* tier of the genome pool. Demand-aligned recall starts considering it.

The foundry runs in a `Background` `ResourceClass` lane. It never blocks persona hot paths. When a new SOTA arrives, the foundry recompiles; existing personas keep running on the previous binary until normal page-fault + LRU pressure migrates them forward. Migration is **explicit** (logged, replayable, reversible) — never silent.

### Why The Foundry Is Substrate, Not An External Service

The foundry could in principle be a separate process pulling SOTA models, adapting them, and dropping files on disk for Continuum to pick up. It is *not* designed that way, because:

- **Provenance must be in-substrate.** A separate service produces files; the substrate has no way to refuse files with missing provenance. In-substrate, the type system enforces `Provenance` is mandatory.
- **Adaptation is hardware-aware.** The right quantization depends on the target's hardware class. The substrate already knows the hardware class via the governor. An external service would have to re-derive it.
- **Federation needs same shape.** If federated hives share foundry-imported artifacts, they must have identical adaptation pipelines. Centralizing in-substrate means the adaptation is the same everywhere or the artifact is incompatible — clear failure mode, no silent drift.

## Part 6: Sentinel-AI — Profile-Guided Optimization

Sentinel-AI is Continuum's **custom experiential model** — distinct from the foundry's imports. It is where lived experience crystallizes into weights. The foundry brings in *what others built*. Sentinel produces *what we lived*.

```rust
// PROPOSED — src/workers/continuum-core/src/sentinel/mod.rs
pub trait SentinelAI: Send + Sync {
    /// Stream traces into the sentinel for outcome attribution.
    /// Cheap; runs continuously.
    async fn observe(&self, trace: &CognitionTrace) -> Result<(), SentinelError>;

    /// Trigger a refinement pass. Runs during sleep / consolidation.
    /// Reads accumulated traces, attributes outcomes, retrains where it has signal.
    async fn refine_pass(&self) -> Result<RefinementReport, SentinelError>;

    /// Read-only attribution: what contributed to this turn's outcome?
    fn attribute(&self, trace: &CognitionTrace) -> Vec<ArtifactAttribution>;

    /// Iterate over refined artifacts this sentinel has produced.
    fn iter_refined(&self) -> Box<dyn Iterator<Item = RefinedArtifact> + '_>;
}

pub struct CognitionTrace {
    pub trace_id: TraceId,
    pub persona: PersonaId,
    pub frame: RuntimeFrameRef,
    pub composition: CompositionPlan,              // what was hot for this turn
    pub recall_results: Vec<RecallResult>,         // what demand-aligned recall returned
    pub output: PersonaOutput,
    pub outcome: Option<Outcome>,                  // attached later when feedback arrives
}

pub struct RefinedArtifact {
    pub kind: RefinedKind,                         // LoRALayer | MoEExpert | EngramCluster | RoutingTable
    pub supersedes: Option<ArtifactRef>,
    pub source_traces: Vec<TraceRef>,
    pub attribution: OutcomeAttribution,
    pub blob: ArtifactBlob,
    pub provenance: Provenance,
}
```

Sentinel does, in order:

1. **Trace consumption.** Every cognition trace flows into sentinel via `observe`. Cheap; the trace is already on the bus, sentinel reads it as a subscriber.
2. **Outcome attribution.** When a trace gets an outcome (user signal, downstream classifier, persona's own retrospective), sentinel attributes that outcome back to the artifacts that contributed — which LoRA layers were composed, which experts fired, which engrams were recalled.
3. **Refinement passes.** During sleep, sentinel retrains. Hot LoRA layers get tightened from traces that used them well. MoE expert routing tables get refined based on which experts fired when outcomes were good. New engrams get generated from clusters of trace patterns.
4. **Publication.** Refined artifacts land in the *refined* tier of the genome pool with full provenance: which traces, which outcomes, which previous artifact version this supersedes.
5. **Adoption.** Demand-aligned recall (next section) starts picking the refined artifact for relevant queries because it scores higher on outcome-conditioned similarity. Old compositions invalidate naturally as their personas next page-fault.

### Local-First, Then Federated

Two design choices that shape the rest of the architecture:

- **Sentinel is local first.** Each instance / machine runs its own sentinel against its own traces. Refined artifacts publish locally before federating. This keeps privacy simple (traces never leave the machine unless explicitly shared) and latency tight (sentinel runs on the same hardware that produced the traces).
- **One sentinel per instance, not per persona.** A single sentinel sees the cross-persona patterns within an instance. Per-persona sentinels would miss the signal that *is* hive evolution. Federation happens at a coarser grain (sentinel-derived artifacts can be published cross-instance with provenance + opt-in).

## Part 7: Demand-Aligned Recall

The substrate's *default lookup* is not "load adapter by name." It is "I need help with this; give me a ranked pool I can compose from."

```rust
// PROPOSED — src/workers/continuum-core/src/genome/recall.rs
pub trait DemandAlignedRecall: Send + Sync {
    async fn recall(
        &self,
        query: &CapabilityQuery,
        context: &PersonaContext,
    ) -> Result<RankedPool, RecallError>;
}

pub struct CapabilityQuery {
    pub task_kind: TaskKind,                       // Chat | Code | Vision | ToolUse | Memory | Plan | ...
    pub domain_hints: Vec<DomainHint>,             // free-form tags from the persona's plan
    pub budget: ResourceBudget,                    // memory + time budget for the composition
    pub must_include: Vec<ArtifactRef>,            // hard pins (persona-private LoRA, sticky engrams)
    pub prefer_refined: bool,                      // default true; sentinel-refined > foundry-imported
}

pub struct RankedPool {
    pub layers: Vec<(LoRALayerRef, RecallScore)>,
    pub experts: Vec<(MoEExpertRef, RecallScore)>,
    pub engrams: Vec<(EngramRef, RecallScore)>,
    pub composition_hint: CompositionHint,         // suggested stack order + weights
}

pub struct RecallScore {
    pub semantic: f32,                             // query → artifact metadata similarity
    pub outcome_history: f32,                      // how well this artifact did in similar contexts
    pub recency: f32,                              // recent-use bonus
    pub tier_proximity: f32,                       // already-hot artifacts get small bonus (cache locality)
    pub provenance_trust: f32,                     // sentinel-refined > imported; trusted source > unknown
    pub combined: f32,                             // weighted combination; weights are governor-tunable
}
```

The persona doing X asks for help with X under a budget; the substrate returns the ranked pool; the persona keeps agency over *how* to compose. The substrate handles *what's available + relevant + cached*.

This is the API every cell should reach for. It is the single most-used substrate primitive in this design.

## Part 8: Composition

A persona's effective model at any moment is a **dynamic composition** of base + tiered LoRA + MoE expert routing + engram-conditioned context. Composition is recomputed when the task / context / pressure shifts; otherwise the substrate caches it.

```rust
// PROPOSED — src/workers/continuum-core/src/genome/composition.rs
pub struct CompositionPlan {
    pub base_model: BaseModelRef,
    pub lora_stack: Vec<LoRAComposition>,
    pub moe_routing: MoERoutingTable,
    pub kv_cache_budget: usize,
    pub engram_context: Vec<EngramRef>,
    pub provenance: CompositionProvenance,         // what query produced this; what was hot at the time
}

pub struct LoRAComposition {
    pub layer: LoRALayerRef,
    pub weight: f32,                               // composition weight
    pub tier_at_plan: Tier,                        // where this layer sat when the plan was made
}

pub trait Composer: Send + Sync {
    /// Build a composition from a ranked pool + persona constraints.
    fn compose(
        &self,
        pool: &RankedPool,
        constraints: &CompositionConstraints,
    ) -> Result<CompositionPlan, CompositionError>;

    /// Materialize a plan: ensure all referenced pages are at least L2-resident,
    /// pin them for the duration of the turn.
    async fn materialize(
        &self,
        plan: &CompositionPlan,
        persona: PersonaId,
    ) -> Result<MaterializedComposition, CompositionError>;
}
```

The composition is the **binary** the persona executes. The genome pool is the *library* it links against. The composer is the *linker* — it picks which library entries land in the binary for this turn, weighted, pinned, and budgeted.

## Part 9: Speculative Pre-Composition

While a persona's current turn is running, the substrate pre-composes the *likely-next* plan and pre-fetches the *likely-next* pages based on conversation trajectory, persona's historical patterns, recent page faults, and branch hints from the turn frame.

```rust
// PROPOSED — src/workers/continuum-core/src/genome/speculation.rs
pub struct SpeculativeBranch {
    pub trigger: TurnTrajectoryHint,               // "user is about to ask follow-up X"
    pub composition: CompositionPlan,
    pub pre_fetch: Vec<PageRef>,
    pub confidence: f32,                           // how strongly we expect this branch
}

pub trait Speculator: Send + Sync {
    /// Generate speculative branches given current turn state.
    fn branches(&self, current: &TurnState) -> Vec<SpeculativeBranch>;

    /// Materialize branches up to the governor's speculation budget.
    async fn pre_materialize(&self, branches: &[SpeculativeBranch]) -> Result<(), SpeculationError>;

    /// Discard branches that did not match the actual next turn.
    async fn discard(&self, kept: &CompositionPlan, branches: &[SpeculativeBranch]);

    /// Hit-rate tracking for governor feedback.
    fn hit_rate(&self) -> HitRateSnapshot;
}
```

If speculation hits, the next turn has near-zero composition latency. If it misses, speculative pages get evicted as normal LRU — *no penalty*. The substrate tracks hit rate per persona and per branch class, and the governor tunes aggressiveness based on it.

On a MacBook Air, the governor sets speculation conservative — only on idle slack, single-branch only, and only when L3 has headroom. On a 5090, the governor sets it aggressive — multi-branch, every turn, even when L2 is full (because L2 eviction is cheap there).

## Part 10: Sharing Protocol — Global-Scale Hive

Sentinel-refined and foundry-adapted artifacts are publishable to the broader hive. Cross-room, cross-instance, optionally cross-user (with consent + provenance). Other personas pull and integrate.

```rust
// PROPOSED — src/workers/continuum-core/src/genome/sharing.rs
pub trait SharingProtocol: Send + Sync {
    /// Publish an artifact to the configured federation scope.
    async fn publish(
        &self,
        artifact: &PublishableArtifact,
        scope: FederationScope,
    ) -> Result<PublicationReceipt, SharingError>;

    /// Pull federation updates. Returns artifacts new since the last pull.
    async fn pull(&self, since: PullCursor) -> Result<Vec<FederatedArtifact>, SharingError>;

    /// Trust-class lookup: how much do we trust this peer's artifacts?
    fn trust_for(&self, peer: PeerId) -> TrustClass;
}

pub enum FederationScope {
    LocalInstance,                                 // never leaves this machine
    Trusted { peers: Vec<PeerId> },                // explicit peer list
    Federation { network: FederationId },          // a named federation
    Public,                                        // open hive — provenance + trust required
}
```

Coherency is **eventual consistency with provenance**. Not MESI. Not locks. When a peer publishes a refined LoRA layer, it goes into the federated pool with provenance attached. Demand-aligned recall starts picking it up because it scores higher on similar queries (subject to trust-class weighting). Old compositions invalidate naturally as their personas next page-fault. Global-scale consistency by demand alignment, not by coordination.

This is the architectural answer to "evolution on a global scale." The hive evolves *as a collective* because the highest-scoring artifacts for any given query propagate through the network organically. No central authority. No lockstep. Just demand alignment + provenance.

### Trust And Adoption

A federated artifact is not blindly trusted. The recall scoring weight on `provenance_trust` is what gates adoption:

- Sentinel-refined locally > sentinel-refined from a trusted peer > sentinel-refined from a known federation > anonymous public artifact.
- Foundry-imported from a foundation vendor > foundry-imported community model.
- An artifact failing local sentinel attribution (it gets recalled, but consistently produces worse outcomes than what it superseded) gets its trust score automatically demoted, and the supersession is reverted.

Trust is *learned*, not declared. This is what makes the federation safe at scale.

## Part 11: The Substrate Governor

The governor is the DVFS layer for the AI substrate. It is the one Rust subsystem that makes "same code on MacBook Air and RTX 5090" real: detect the hardware at boot, write the policy file, expose a read-only `current_policy()` to every other subsystem, adjust at runtime under pressure, and reverse cleanly when pressure releases. Every other subsystem in this document — tier stores, recall, composer, speculator, foundry, sentinel, sharing protocol — reads the governor and never writes back. The governor *is* the single source of truth for sizing.

### Trait Surface

```rust
// PROPOSED — src/workers/continuum-core/src/governor/mod.rs
pub trait SubstrateGovernor: Send + Sync {
    /// Current policy. Cheap read: returns Arc to immutable snapshot, so
    /// callers can hold without contention. Policy is rewritten under
    /// pressure, never mutated in place.
    fn current_policy(&self) -> Arc<GovernorPolicy>;

    /// Called once at boot, and any time hardware changes (eGPU plug,
    /// power source change, thermal class change). The probe sequence
    /// is in §"Hardware Detection" below.
    fn on_hardware_detected(&self, hw: HardwareClass);

    /// Called by PressureBroker (CBAR-SUBSTRATE) when a typed pressure
    /// signal crosses a threshold. Governor decides whether to step the
    /// cascade, hold, or reverse. See §"Adjustment Cascade" for thresholds.
    fn on_pressure_signal(&self, signal: PressureSignal);

    /// Snapshot for VDD report emission and human inspection. Includes
    /// current policy + recent history + cascade-step counter.
    fn snapshot(&self) -> GovernorSnapshot;

    /// Subscribe to policy changes. Each subscriber gets the new Arc as
    /// soon as the cascade commits. Used by composer / speculator /
    /// tier stores to react without polling.
    fn subscribe(&self) -> PolicyWatch;
}

pub struct GovernorPolicy {
    pub policy_version: u64,                          // monotonic; increments on every rewrite
    pub hardware_class: HardwareClass,                // what produced this policy
    pub tier_sizes: TierSizes,
    pub cadence_multipliers: CadenceMultipliers,
    pub concurrency_caps: ConcurrencyCaps,
    pub speculation_aggressiveness: SpeculationLevel,
    pub consolidation_schedule: ConsolidationSchedule,
    pub federation_pull_cadence: FederationCadence,
    pub recall_score_weights: RecallScoreWeights,
    pub cascade_step: u8,                             // 0 = normal; 1..5 = under pressure (see cascade)
    pub committed_at: SystemTime,
}

pub struct HardwareClass {
    pub silicon: TargetSilicon,                       // AppleM | NvidiaCuda | AmdRocm | IntelVulkan | None
    pub silicon_model: String,                        // "M2", "RTX 5090", "Radeon RX 7900 XTX", ...
    pub vram_mb: usize,
    pub system_ram_mb: usize,
    pub power_source: PowerSource,                    // Battery | Plugged
    pub thermal_class: ThermalClass,                  // ThinAndLight | Workstation | Server | Mobile
    pub battery_pct: Option<u8>,                      // None if no battery
    pub thermal_headroom_pct: Option<u8>,             // None if not measurable
}

pub enum PressureSignal {
    Thermal       { severity: ThermalSeverity },      // Cool | Warm | Hot | Critical
    BatteryLow    { remaining_pct: u8 },
    SystemMemHigh { used_pct: u8 },
    VRAMHigh      { used_pct: u8 },
    UserActive    { foreground: bool },               // foreground user input → favor responsiveness
    InferenceQueueDepth { depth: usize },             // backed-up turns; signal to throttle speculation
    SpeculationMissRate { rate: f32 },                // bad predictions → throttle aggressiveness
}
```

The governor never blocks. Reads (`current_policy()`) are wait-free `Arc` clones. Writes (cascade steps, policy rewrites) hold a small mutex for under a microsecond and publish via `arc_swap`. A composer reading the policy 1000 times per turn pays no contention cost.

### Hardware Detection

Boot-time detection runs once and produces a `HardwareClass`. The probe sequence is deterministic and small:

```rust
// PROPOSED — src/workers/continuum-core/src/governor/detect.rs
pub fn detect_hardware() -> HardwareClass {
    HardwareClass {
        silicon:           probe_silicon(),           // platform-specific: Metal / CUDA / ROCm / Vulkan probes
        silicon_model:     probe_silicon_model(),     // sysinfo / nvidia-smi / rocm-smi / IORegistry
        vram_mb:           probe_vram_mb(),           // 0 for unified-memory targets (Air); use system_ram fraction
        system_ram_mb:     sysinfo_total_memory_mb(),
        power_source:      probe_power_source(),     // IOPSCopyPowerSourcesList / /sys/class/power_supply
        thermal_class:     classify_thermal(...),    // derived from silicon + chassis hints + power
        battery_pct:       probe_battery_pct(),
        thermal_headroom_pct: probe_thermal_headroom_pct(),
    }
}
```

Each probe has a fallback. If `nvidia-smi` is missing, `silicon` falls back to `Vulkan` if Vulkan is available, else `None`. If `IOPSCopyPowerSourcesList` returns no source, `power_source` falls back to `Plugged` (favor performance when we can't tell). **All fallbacks are typed and logged** — silent guess-where-we-are is forbidden by the same `no_silent_fallback` rule that governs the rest of the substrate.

Re-detection fires on three triggers: eGPU hot-plug (platform notification), power source change (charger plug/unplug), and a periodic sanity check (default 5 minutes) that catches missed events. A re-detected `HardwareClass` that materially differs from the current one triggers a policy rewrite.

### Policy File Format

The governor's policy is computed from a versioned policy file. Policy files are TOML, live under `~/.continuum/policy/`, and named by the hardware-class fingerprint they apply to. Engineers tune by editing these; the governor watches the file and reloads on change.

```toml
# ~/.continuum/policy/apple-m-thinandlight-16gb-uma.toml
# Hardware fingerprint (matches HardwareClass): Apple M-series, ThinAndLight,
# 16 GB unified memory. The governor selects this file at boot.

policy_version = 3
applies_to    = "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"

[tier_sizes]
l1_lora_layers       = 2
l1_kv_tokens         = 2048
l2_lora_layers       = 4
l3_lora_layers       = 12
l3_engrams           = 1024
# l4 and l5 are SSD-bounded; no in-file limit.

[cadence_multipliers]
realtime             = 1.0
delayed              = 1.5   # delay non-realtime by 50% on Air
background           = 2.0

[concurrency_caps]
personas_concurrent  = 2
inference_lanes      = 1
foundry_lanes        = 0     # disabled on Air to preserve foreground responsiveness
sentinel_lanes       = 1

[speculation]
level                = "conservative"   # "off" | "conservative" | "balanced" | "aggressive"
max_branches         = 1
min_idle_slack_pct   = 30
miss_rate_throttle   = 0.5   # if hit rate < 50%, drop a level

[consolidation]
schedule             = "idle_plugged_in"  # "always" | "idle" | "idle_plugged_in" | "manual"
min_idle_seconds     = 300
preempt_on_pressure  = true

[federation]
pull_cadence_seconds = 600

[recall_weights]
semantic             = 0.4
outcome_history      = 0.3
recency              = 0.1
tier_proximity       = 0.1
provenance_trust     = 0.1
```

The 5090 anchor uses the same schema with larger numbers:

```toml
# ~/.continuum/policy/nvidia-cuda-workstation-32gb-vram.toml
applies_to            = "nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000"

[tier_sizes]
l1_lora_layers        = 8
l1_kv_tokens          = 16384
l2_lora_layers        = 16
l3_lora_layers        = 40
l3_engrams            = 10240

[concurrency_caps]
personas_concurrent   = 8
inference_lanes       = 4
foundry_lanes         = 1
sentinel_lanes        = 2

[speculation]
level                 = "aggressive"
max_branches          = 4
min_idle_slack_pct    = 5

[consolidation]
schedule              = "idle"
min_idle_seconds      = 60
preempt_on_pressure   = true
```

**Same TOML schema, same Rust loader, same `GovernorPolicy` struct.** The numbers are the only thing that changes. Policy files for intermediate hardware (M-Pro/Max, mid-range NVIDIA, AMD ROCm, Vulkan-only Intel) ship as defaults; users can override any field via `~/.continuum/policy/local.toml` which overlays the auto-selected policy.

### Adjustment Cascade — With Thresholds, Hysteresis, And Algorithm

When `on_pressure_signal()` fires, the governor *may* step the cascade. The cascade has six steps (0 = normal, 5 = maximum throttle). Each step has an *enter* threshold and an *exit* threshold; the gap between them is the hysteresis that prevents oscillation.

| Step | Action | Enter threshold (any signal triggers) | Exit threshold (all clear required) |
|---|---|---|---|
| 1 | Drop speculation level by one notch; halve `max_branches` | `SpeculationMissRate > 0.5` OR `InferenceQueueDepth > N` OR `VRAMHigh > 85` | rates back below 0.3 AND queue depth < N/2 AND VRAM < 70 |
| 2 | `concurrency_caps.personas_concurrent -= 1`; defer non-realtime turns | step 1 still active for > 30s OR `SystemMemHigh > 85` OR `Thermal::Hot` | step 1 cleared AND mem < 70 AND `Thermal::Cool|Warm` |
| 3 | Shrink working-set L1/L2 budgets by 25%; trigger spill | step 2 active for > 30s OR `BatteryLow < 15` OR `Thermal::Critical` | step 2 cleared AND battery > 25 AND `Thermal::Cool|Warm` |
| 4 | Drop `federation.pull_cadence_seconds` to maximum value (slowest pull) | step 3 active for > 60s | step 3 cleared |
| 5 | Suspend `consolidation` immediately; if a refinement pass is running, pause and persist its state | step 4 active OR explicit emergency signal | step 4 cleared AND idle slack > min_idle_slack_pct |

Algorithm:

```rust
// PROPOSED — src/workers/continuum-core/src/governor/cascade.rs
impl GovernorState {
    pub fn on_pressure_signal(&self, signal: PressureSignal) {
        let next_step = self.evaluate_step(&signal);
        if next_step > self.cascade_step.load() && self.dwell_satisfied(next_step) {
            self.step_up(next_step);
        } else if next_step < self.cascade_step.load() && self.all_clear(next_step) {
            self.step_down(next_step);
        }
        // otherwise: hold. Hysteresis keeps us here.
    }

    fn step_up(&self, to: u8) {
        for s in (self.cascade_step.load() + 1)..=to {
            self.apply_step(s, Direction::Throttle);
            self.emit_event(GovernorEvent::CascadeUp { step: s });
        }
        self.commit_policy();   // arc_swap; subscribers wake
    }

    fn step_down(&self, to: u8) {
        for s in (to..self.cascade_step.load()).rev() {
            self.apply_step(s, Direction::Restore);
            self.emit_event(GovernorEvent::CascadeDown { step: s });
        }
        // Speculation aggressiveness restored LAST — see "Restore Order" below.
        self.commit_policy();
    }
}
```

**Restore order.** When pressure releases, the cascade steps down in reverse, with one twist: speculation aggressiveness is restored *one step later than it was throttled*. If speculation was throttled at step 1 and pressure clears through step 0, speculation stays at its throttled level for a "calibration window" (default 60s) so the hit-rate can stabilize before aggressiveness ramps back up. This is the single most-important anti-oscillation rule.

### Runtime Adjustment Loop

The governor's main loop is small and explicit:

```rust
// PROPOSED — src/workers/continuum-core/src/governor/runtime.rs
async fn governor_loop(state: Arc<GovernorState>, mut rx: mpsc::Receiver<PressureSignal>) {
    let mut periodic = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            Some(signal) = rx.recv() => state.on_pressure_signal(signal),
            _ = periodic.tick()       => state.reevaluate_periodic(),  // catches missed events
            _ = state.hardware_change_notify() => state.on_hardware_detected(detect_hardware()),
        }
    }
}
```

The loop is the only place that mutates `GovernorState`. Everything else reads `current_policy()` (wait-free Arc clone) and reacts to `subscribe()` notifications. No subsystem ever writes to the governor directly — pressure signals flow in through `PressureBroker` (CBAR-SUBSTRATE), policy flows out through Arc subscriptions.

### Federation Policy Reconciliation

In a federated hive (multiple instances coordinating), each instance runs its own governor against its own hardware. Federation policy reconciliation is **deliberately minimal**: instances do *not* synchronize policy. Each runs its hardware's policy independently. What federation *does* synchronize is the `RecallScoreWeights` — because two instances ranking the same artifact differently for `provenance_trust` produces drift in what gets adopted.

Concretely: when an instance joins a federation, it pulls the federation's `RecallScoreWeights` and overlays them onto its local policy. All other fields (tier sizes, concurrency, speculation) stay hardware-local. This keeps a 5090 from being throttled because a fellow Air is under pressure, while ensuring the federation agrees on *what counts as trustworthy*.

### Override Mechanism (Dev / Testing)

Three escape hatches for engineers:

1. **`CONTINUUM_POLICY_FILE` env var.** Overrides hardware-fingerprint selection. Useful for testing one hardware policy on a different machine (run the Air policy on a 5090 to verify the substrate degrades cleanly).
2. **`~/.continuum/policy/local.toml`.** Overlay file; any field set here wins. Useful for tuning without editing the shipped policy.
3. **`continuum governor pin --step N`.** Pin the cascade at a specific step for the next N minutes. Useful for VDD runs that need a known throttle level.

All overrides emit a typed `GovernorOverride` event so the trace bus shows that VDD records aren't from the auto-policy.

### Observability

The governor emits to the trace bus on every state change:

- `GovernorEvent::HardwareDetected { hw }` — at boot and on re-detection.
- `GovernorEvent::PolicyCommitted { version, source: HardwareDetection | FileReload | Override }` — every policy rewrite.
- `GovernorEvent::CascadeUp { step }` / `CascadeDown { step }` — every cascade transition.
- `GovernorEvent::OverrideApplied { kind }` — when an escape hatch fires.
- `GovernorEvent::PolicyDriftDetected { instance, field }` — when federation reconciliation flags a divergence.

Every VDD record carries the active `policy_version` and `cascade_step`. A VDD run on the Air at step 0 vs step 3 should produce visibly different timings, and the records make those differences attributable to the governor, not to noise.

### Performance Budget For The Governor Itself

The governor's own resource use is bounded:

- `current_policy()`: wait-free Arc clone, < 50 ns typical.
- `subscribe()`: tokio watch channel; subscriber wake latency < 1 μs.
- Cascade evaluation per signal: < 10 μs including event emission.
- Policy rewrite: < 100 μs including arc_swap publish.
- Periodic re-evaluation: < 1 ms every 5 seconds.

The governor cannot become a contention point or a latency tax. Its own performance is part of its acceptance criteria (see Part 14).

## Part 12: Artifact Lifecycle

Every durable artifact (six kinds in Part 1) follows the same lifecycle, with phase transitions driven by demand alignment:

```text
┌─────────┐      ┌─────────┐      ┌─────────┐      ┌──────────┐      ┌──────────┐
│ Created │ ──▶  │ Adopted │ ──▶  │ Refined │ ──▶  │ Archived │ ──▶  │ Retired  │
└─────────┘      └─────────┘      └─────────┘      └──────────┘      └──────────┘
     │                │                 │                 │                 │
     │                │                 │                 │                 │
  foundry          adopted by      sentinel re-      out of working     provably
  imports          N personas      trains from        set; still         superseded
  or sentinel      via demand-     accumulated        recallable from    by a refined
  derives          aligned         outcomes           L4/L5              version;
                   recall                                                provenance
                                                                         preserved
```

Transitions are emitted as typed events on the trace bus. Each transition carries provenance. **No phase is ever silent.**

### Why Lifecycle Matters For Engineering

For the engineer landing types: every artifact transition must be observable. A LoRA layer that is "in the pool" but never adopted should appear in a `Created, never adopted` query. A layer that adoption rate is falling for should be visible in attribution. A retired layer's provenance chain should be walkable. The substrate makes these queries first-class so engineers can debug evolution, not guess at it.

## Part 13: Connection To CBAR-SUBSTRATE (Lane H)

This document specifies the artifact economy. CBAR-SUBSTRATE specifies the runtime contract every cell inherits. They connect at three points:

1. **Every cell's `ModuleContext` exposes `DemandAlignedRecall`.** A cell asks for help; the genome pool answers. No cell loads adapters by name.
2. **`PressureBroker` informs the `SubstrateGovernor`.** Pressure signals from the broker drive the governor's adjustment cascade. The broker keeps owning admission; the governor owns *sizing*.
3. **The `RuntimeFrame` carries a `CompositionRef`.** The frame's lazy outputs include the composition active for the turn. Sentinel reads it as part of trace attribution.

A new lane in ALPHA-GAP:

**Lane H: Substrate Governor + Tiered Genome Cache.** Sibling to Lane E (`PressureBroker`). Owns: governor types + policy, tier stores, working-set manager, demand-aligned recall, composer + speculator, foundry + sentinel skeletons. PR sequence:

1. `governor-types`: `SubstrateGovernor`, `GovernorPolicy`, `HardwareClass`, hardware detection at boot.
2. `tier-stores`: five `TierStore` implementations + eviction policies; `WorkingSetManager` over them.
3. `recall-api`: `DemandAlignedRecall` trait + initial scoring; ts-rs exports.
4. `composer-speculator`: `Composer` + `Speculator`; hit-rate tracking.
5. `foundry-skeleton`: `Foundry` trait + one absorber (Qwen) + provenance emission.
6. `sentinel-skeleton`: `SentinelAI` trait + trace consumption + one refinement pass type.
7. `sharing-protocol-local-first`: `SharingProtocol` with `LocalInstance` scope only; federation deferred.

## Part 14: Acceptance Criteria

Substrate is "done" when the following are provable on canary, with PR-attached evidence:

**Provenance and observability:**

- Every artifact in the genome pool has a non-default `Provenance`. A query for "artifacts with missing provenance" returns zero.
- Every page fault, eviction, composition change, speculation hit/miss, foundry import, and sentinel refinement is a typed event on the trace bus.
- A `cargo test` regression proves the trace bus carries the typed events; a missing event class fails the test.

**Hardware portability:**

- The same Rust binary boots on MacBook Air (16 GB UMA) and on RTX 5090 (32+64 GB) and the governor writes different policies for each. VDD records show different tier sizes / concurrency caps / speculation aggressiveness.
- A persona round-trip turn produces working output on both anchor configurations within the latency budgets named in CBAR-SUBSTRATE's performance covenant.

**Demand-aligned recall:**

- A `recall(query)` returns a non-empty `RankedPool` for every supported `TaskKind`, populated from the imported tier alone (sentinel not required to bootstrap).
- A second `recall(same query)` after a sentinel refinement pass that produced a relevant refined artifact ranks the refined artifact higher than the imported version it superseded.

**Foundry:**

- A foundry absorb of a Qwen variant produces at least one `ImportedArtifact` with full provenance. The artifact participates in recall on the next query.
- A foundry refresh on a new SOTA version emits a `Supersession` record and the old artifact's recall score decays.

**Sentinel:**

- After N cognition traces with attached outcomes, the sentinel produces at least one `RefinedArtifact` with non-empty `OutcomeAttribution`.
- The refined artifact's provenance chain walks back to the source traces.

**Lifecycle:**

- A query for an artifact's lifecycle (`Created → Adopted → Refined → Archived → Retired`) returns the full chain with timestamps.
- A retired artifact's reverse query ("what superseded this?") returns the active artifact.

**Compartmentalization:**

- A persona attempting to read another persona's private engram space gets `AccessDenied`, emits an audit record, and the trace bus carries the attempt.

**Substrate governor:**

- Simulated pressure signals (thermal / battery / OOM) trigger the adjustment cascade in the documented order. Each step is observable.
- Pressure release reverses the cascade.

## Part 15: Open Questions

Real questions the engineer will hit. Tentative answers for each.

1. **MoE expert paging granularity.** Page at the expert level or at sub-expert chunks? Tentative: expert level for v1. Sub-expert paging is a future optimization, sketched but not committed to.

2. **Engram embedding model.** What embeds engrams for similarity-based recall — a foundry-imported embedding shard, or a sentinel-refined embedder trained on the hive's own data? Tentative: foundry-imported in v1 (need a working bootstrap); sentinel-refined in v2 (it does better on the hive's own distribution).

3. **Cross-persona engram sharing default.** Default opt-in or opt-out for cross-persona engram visibility to sentinel? Tentative: opt-in. The privacy story is the architectural promise; sentinel can ask but cannot help itself.

4. **Foundry trust anchor.** What is the cryptographic / verification anchor on imported SOTA weights? Tentative: signed manifests for foundation-vendor sources; community sources get lower trust score by default and require explicit user opt-in for adoption.

5. **Speculation discard cost.** What's the budget for a speculative branch that misses? Tentative: zero direct cost (just LRU eviction), but the speculator's hit rate is governor input and consistent miss rates throttle aggressiveness.

6. **Sleep scheduling on always-on instances.** When does a 24/7 server consolidate? Tentative: rolling consolidation — never a full pause, always a fraction of personas in consolidation while others stay active. Like CPU cores entering low-power states without halting the OS.

7. **Federation discovery.** How do hives discover each other? Tentative: explicit, manual, opt-in. No mDNS-style auto-discovery. The first federation in scope is "same user, multiple machines."

8. **Composition stability vs adaptation rate.** How often should a persona recompose during a single conversation? Tentative: only on detected context shift (new task kind, new domain, large recall divergence). Mid-turn recomposition is expensive and the substrate avoids it by speculative pre-composition.

## See Also

- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — runtime substrate contract. Owns concurrency, scheduling, memory pressure, device pressure, telemetry, artifact handles, lifecycle.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — lane-shaped roadmap. Lane H (this document's implementation) lives here.
- [CONTINUUM-ARCHITECTURE.md](../CONTINUUM-ARCHITECTURE.md) — engine shape; this doc is the genome / foundry / sentinel detail beneath the engine surface.
- [CONTINUUM-VISION.md](../CONTINUUM-VISION.md) — product vision. The personas this substrate evolves are the personas described there.
