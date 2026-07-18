# Genome, Foundry, Sentinel-AI: The Artifact-Sharing Economy On Consumer Hardware

> **Substrate contract:** [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) — the runtime contract every Rust concern inherits. This document specifies the *artifact economy* that flows on top of that contract.
> **Lane-shaped roadmap:** [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — implementation lands per Lane H (Substrate Governor + Tiered Genome Cache) once the design here is reviewed.
> **Status:** design proposal. No code in this document; every API shape shown is a proposed Rust trait targeted at `core/continuum-core/src/genome/`, `foundry/`, and `sentinel/`.

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
// PROPOSED — Lane H deliverable, targeted at core/continuum-core/src/genome/provenance.rs
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

The cache is a sequence of **tier roles** parameterized by hardware class. Discrete-GPU hardware has five distinct tiers; unified-memory hardware collapses the top two into one. The Rust code is identical across hardware; only the `Vec<TierConfig>` per-policy differs.

> **Crit incorporated** from `claude-tab-1` (vHSM-scope, 2026-05-16): the v1 sketch used a fixed `L1..L5` enum. That's wrong on UMA hardware (M-series Macs, M5 Pro, iOS, Vision Pro, embedded) where the "L1 accelerator-resident" and "L2 system RAM" bytes are the same physical pool. An L1→L2 eviction is a no-op. The substrate code stays uniform; the tier count varies. Vision Pro and iOS will be UMA-class — locking 5-as-universal now would force a refactor when those land. This section now uses **tier roles**, not ordinal positions.

### Tier Roles

```rust
// PROPOSED — core/continuum-core/src/genome/tier.rs
pub enum TierRole {
    /// Bytes the accelerator can read at peak bandwidth.
    /// Discrete GPU: VRAM. UMA: the hot portion of unified memory.
    Fast,

    /// Bytes the accelerator can reach with a copy or a tier-promotion.
    /// Discrete GPU: host RAM (PCIe-attached, copy required to use).
    /// UMA: same physical pool as Fast — this tier is omitted on UMA hardware.
    Warm,

    /// Bytes the host can read at memory speed; cold to the accelerator.
    /// Discrete GPU + UMA: a designated portion of system RAM held for the
    /// genome catalog + recently-used artifacts.
    Bench,

    /// Bytes on local SSD. The full genome pool lives here on every class
    /// of hardware. Read latency is milliseconds; bandwidth is mmap-bound.
    Cold,

    /// Bytes on archive storage. Append-only with provenance preserved.
    /// Reads are sub-second but never on the hot path. GC during sleep.
    Frozen,
}

pub struct TierConfig {
    pub role:        TierRole,
    pub capacity:    TierCapacity,         // current_used, configured_limit
    pub eviction:    EvictionPolicy,       // policy varies by role (see below)
    pub backing:     TierBackingRef,       // implementation handle
}

pub trait TierStore: Send + Sync {
    fn role(&self) -> TierRole;
    async fn read(&self, page: PageRef) -> Result<PageHandle, TierError>;
    async fn write(&self, page: PageRef, blob: ArtifactBlob, prov: Provenance) -> Result<(), TierError>;
    async fn evict(&self, target_free_bytes: usize) -> Vec<EvictionRecord>;
    fn capacity(&self) -> TierCapacity;
    fn observe_access(&self, page: PageRef);
}
```

The governor's policy file (Part 11) declares a `Vec<TierConfig>` — typically four entries on UMA hardware, five on discrete-GPU hardware. Subsystems index into the vec by `TierRole`, not by ordinal position. Page-fault reports name the source and destination by role:

```rust
pub struct PageFault {
    pub page:          PageRef,
    pub from_role:     Option<TierRole>,   // None = true cold miss (page does not exist yet)
    pub to_role:       TierRole,
    pub persona:       PersonaId,
    pub elapsed_us:    u64,
    pub eviction_cost: Option<EvictionRecord>,
}
```

### Eviction Policy Per Role

| Role | Policy | When eviction fires |
|---|---|---|
| `Fast` | LRU within current turn | sub-step needs a page not resident |
| `Warm` (discrete-GPU only) | LRU across last N turns (governor sets N; default 100) | `Fast` spill |
| `Bench` | LFU + recency; broad-use pages get retention bonus | `Warm` spill (discrete) or `Fast` spill (UMA) |
| `Cold` | Demand-aligned with sentinel-refined preference (refined wins ties over imported) | `Bench` spill |
| `Frozen` | Append-only with provenance preserved; GC only during sleep | never in hot path |

Eviction is *always* typed: every evicted page emits an `EvictionRecord` to the trace bus. Recurring evictions of the same page across turns are exactly the signal sentinel uses to upgrade the page's tier policy.

### Hardware Anchors

Two anchor configurations; everything else interpolates. The substrate *detects* the hardware class at boot and the governor writes a `Vec<TierConfig>` of the right shape. **On UMA hardware, `Warm` is omitted** — the vec has four entries; an `Fast`→`Warm` eviction is structurally absent because there is no separate `Warm` tier to evict to.

**MacBook Air, M-series, 16 GB unified memory** — UMA-class, four tiers:

```
[ Fast(2 LoRA layers + 2k KV tokens; LRU-within-turn)
, Bench(12 layers + ~1k engrams; LFU + recency)
, Cold(SSD genome pool; demand-aligned, sentinel-refined preferred)
, Frozen(longterm.db; append-only, GC during sleep)
]
```

**RTX 5090, 32 GB VRAM + 64 GB system RAM** — discrete-GPU, five tiers:

```
[ Fast(8 LoRA layers + 16k KV tokens; LRU-within-turn)
, Warm(16 layers; LRU across last 100 turns)
, Bench(40+ layers + ~10k engrams; LFU + recency)
, Cold(SSD genome pool; demand-aligned, sentinel-refined preferred)
, Frozen(longterm.db; append-only, GC during sleep)
]
```

Other axes that vary per anchor:

| | **Air (UMA, 4 tiers)** | **5090 (discrete, 5 tiers)** |
|---|---|---|
| Concurrent personas | 1–2 | 6–8 |
| Speculative composition | conservative (only on idle slack) | aggressive (every turn) |
| Sleep / consolidation cadence | nightly, opportunistic on idle/plugged-in | nightly + partial during day |
| Cross-instance federation pull | manual / explicit | automatic on idle |

M-Pro/Max are UMA-class with larger pools (still four tiers, bigger numbers). Discrete AMD/Intel via Vulkan match the 5090 shape with smaller numbers. Vision Pro and iOS are UMA-class with aggressive eviction + reduced concurrency + simpler composition (still four tiers; the `Warm` role is structurally absent, not just configured to zero). Embedded targets may drop to three tiers (`Fast`, `Cold`, `Frozen`) if `Bench` would compete with foreground responsiveness.

**The Rust code is identical across all of them.** The architectural beauty: subsystems address tiers by role, the governor writes a `Vec<TierConfig>` of the right length, and the type system makes "L1→L2 eviction on UMA" structurally impossible because there is no `Warm` tier to evict to.

## Part 3: Paging, Working Set, And Page Faults

A persona's `WorkingSet` is the set of pages currently hot in L1+L2 for that persona. Pages can be LoRA layer pages, MoE expert pages, KV cache pages, or engram pages.

```rust
// PROPOSED — core/continuum-core/src/genome/working_set.rs
pub struct WorkingSet {
    pub persona: PersonaId,
    pub pages: HashMap<PageRef, ResidentPage>,
    pub capacity: WorkingSetCapacity,              // from governor
    pub last_composition: Option<CompositionPlan>,
}

pub struct ResidentPage {
    pub page: PageRef,
    pub role: TierRole,                            // Fast (or Warm on discrete-GPU hardware)
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

When the persona's composition needs a page not in its working set, that's a **page fault** (the typed struct is defined in Part 2 alongside `TierRole`):

```rust
pub trait WorkingSetManager: Send + Sync {
    /// Promote a page into this persona's working set. May trigger eviction.
    async fn page_in(&self, persona: PersonaId, page: PageRef) -> Result<PageHandle, PageFault>;

    /// Demote a page out of the working set toward the named tier role.
    async fn page_out(&self, persona: PersonaId, page: PageRef, to: TierRole) -> Result<(), TierError>;

    /// Current working set for read-only inspection.
    fn working_set(&self, persona: PersonaId) -> &WorkingSet;

    /// Enforced MMU-style audit: persona is asking for a page.
    /// Returns AccessDenied if the page is private to another persona.
    fn audit_access(&self, persona: PersonaId, page: PageRef) -> Result<(), AccessDenied>;
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
// PROPOSED — core/continuum-core/src/foundry/mod.rs
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
// PROPOSED — core/continuum-core/src/sentinel/mod.rs
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

The substrate's *default lookup* is not "load adapter by name." It is "I need help with this; give me a ranked pool I can compose from." Recall is the single most-used substrate primitive in this design and the place where consumer-hardware federation either earns its keep or doesn't — every cell touches it, every turn, and the ingenuity of how it spans local cache → cross-instance grid → federated peers is what makes the underdog architecture competitive.

### Trait Surface

```rust
// PROPOSED — core/continuum-core/src/genome/recall.rs
pub trait DemandAlignedRecall: Send + Sync {
    /// The hot-path lookup. Sub-ms target on local L1/L2 hits; grid-aware
    /// budget when results must come from a peer or federation pull.
    async fn recall(
        &self,
        query: &CapabilityQuery,
        context: &PersonaContext,
    ) -> Result<RankedPool, RecallError>;

    /// Replay a previous recall deterministically from its trace record.
    /// Used by sentinel for outcome attribution and by VDD for regression
    /// testing. Replay produces the same RankedPool the live recall did,
    /// using snapshotted scoring weights + artifact set at that time.
    async fn replay(
        &self,
        trace: &RecallTrace,
    ) -> Result<RankedPool, RecallError>;
}

pub struct CapabilityQuery {
    pub task_kind:        TaskKind,                // Chat | Code | Vision | ToolUse | Memory | Plan | ...
    pub domain_hints:     Vec<DomainHint>,         // free-form tags from the persona's plan
    pub budget:           ResourceBudget,          // memory + time budget for the composition
    pub must_include:     Vec<ArtifactRef>,        // hard pins (persona-private LoRA, sticky engrams)
    pub prefer_refined:   bool,                    // default true; sentinel-refined > foundry-imported
    pub scope:            RecallScope,             // Local | LocalThenGrid | Federation { ... }
    pub freshness_target: FreshnessTarget,         // BestEffort | FreshAsOf(ts) | Strict
}

pub struct PersonaContext {
    pub persona:                 PersonaId,
    pub current_composition:     Option<CompositionRef>,   // what's already hot
    pub recent_outcomes:         OutcomeWindow,            // last N turns of outcomes (sentinel input)
    pub conversation_trajectory: TrajectoryHint,           // for speculative weight on probable next-task
    pub trust_overrides:         Vec<(PeerId, TrustClass)>,// user-explicit trust adjustments
}

pub struct RankedPool {
    pub layers:           Vec<(LoRALayerRef,  RecallScore, ResidencyHint)>,
    pub experts:          Vec<(MoEExpertRef,  RecallScore, ResidencyHint)>,
    pub engrams:          Vec<(EngramRef,     RecallScore, ResidencyHint)>,
    pub composition_hint: CompositionHint,         // suggested stack order + weights
    pub trace_ref:        RecallTrace,             // sentinel + VDD replay handle
}

pub enum RecallScope {
    Local,                                          // never leave this machine
    LocalThenGrid { max_grid_pulls: usize },        // local first; grid pulls bounded
    Federation { peers: Vec<PeerId>, max_latency_ms: u32 },
}

pub enum ResidencyHint {
    Hot { role: TierRole },                         // already Fast (or Warm on discrete-GPU)
    Local { role: TierRole },                       // Bench / Cold / Frozen on this machine; promotable
    GridPeer { peer: PeerId, est_latency_ms: u32 }, // resident on a federated peer
    NotResident { acquirable_from: AcquireSource }, // foundry would have to import or sentinel refine
}
```

`ResidencyHint` is the load-bearing addition: the persona doesn't just see *what's relevant*, it sees *where it lives* and *what it costs to use*. A persona on a MacBook Air running tight on VRAM can pick the local L3 layer over a slightly-higher-scoring layer on a peer's 5090 — because the scoring already incorporates `tier_proximity`, but the explicit `ResidencyHint` lets the persona make the cost trade-off visibly.

### The Scoring Function — Explicit, Tunable, Sentinel-Refined

The combined score is a weighted sum, but the weights are dynamic — governor-tunable per hardware class and sentinel-refined per persona over time. The base function is intentionally simple so its behavior is auditable:

```rust
// PROPOSED — core/continuum-core/src/genome/recall/scoring.rs
pub fn score(
    artifact: &ArtifactCandidate,
    query:    &CapabilityQuery,
    ctx:      &PersonaContext,
    weights:  &RecallScoreWeights,
) -> RecallScore {
    let semantic         = cosine(query.embed(), artifact.embed());
    let outcome_history  = outcome_window_score(artifact.id, ctx.recent_outcomes);
    let recency          = recency_decay(artifact.last_used, now(), HALF_LIFE);
    let tier_proximity   = match artifact.residency {
        ResidencyHint::Hot   { .. }           => 1.0,
        ResidencyHint::Local { role }         => local_role_score(role),
        //                                       Bench  ≈ 0.6
        //                                       Cold   ≈ 0.3
        //                                       Frozen ≈ 0.1
        ResidencyHint::GridPeer { est_latency_ms, .. } => grid_penalty(est_latency_ms),
        ResidencyHint::NotResident { .. }     => 0.0,
    };
    let provenance_trust = trust_score(artifact.provenance, ctx.trust_overrides);

    let combined =
          weights.semantic         * semantic
        + weights.outcome_history  * outcome_history
        + weights.recency          * recency
        + weights.tier_proximity   * tier_proximity
        + weights.provenance_trust * provenance_trust;

    RecallScore { semantic, outcome_history, recency, tier_proximity, provenance_trust, combined }
}
```

Each factor has a clean definition:

- **`semantic`** is cosine similarity between query embedding and artifact metadata embedding. The embedding model is itself a foundry-imported artifact in v1 (bootstrap), sentinel-refined in v2 (Open Question 2 in this doc).
- **`outcome_history`** scores how well this artifact performed in the persona's last N turns of similar tasks. `outcome_window_score` is exponentially-decayed weighting of explicit outcomes (user signal) and implicit outcomes (downstream tool success, conversation continuation length).
- **`recency`** is exponential decay over time-since-last-use. Half-life is governor-tunable; default 24h.
- **`tier_proximity`** penalizes cost-to-promote. Hot artifacts score 1.0; cold archive scores 0.2; grid peers score a function of estimated latency (see `grid_penalty` below).
- **`provenance_trust`** is the artifact's trust score adjusted by the persona's trust overrides. Sentinel-refined-locally > sentinel-refined-by-trusted-peer > foundry-imported > anonymous-public.

`grid_penalty(latency_ms)` is the load-bearing cost function for federated recall:

```rust
fn grid_penalty(est_latency_ms: u32) -> f32 {
    // Same-LAN peer (< 10 ms):   ~0.55  — slightly worse than local L3
    // Same-region (< 50 ms):     ~0.35
    // Cross-region (< 200 ms):   ~0.15
    // Slow / unreliable:         ~0.05
    0.6 * (-(est_latency_ms as f32 / 100.0)).exp()
}
```

The penalty is *steep* — a peer's slightly-better artifact has to be substantially better to overcome the latency cost. This is the architectural choice: on consumer hardware, **a hot local L3 hit usually wins**, and that's why a federated swarm of MacBook Airs can compete with a single datacenter — the swarm's local cache wins on latency, the swarm's diversity wins on coverage, and the substrate's recall makes both visible to the persona without it having to know the topology.

### Dynamic Weights — Governor And Sentinel Both Tune

`RecallScoreWeights` is part of `GovernorPolicy` (Part 11). The governor sets it per hardware class:

```toml
[recall_weights]
# Air: cache locality matters more (smaller hot set)
semantic         = 0.40
outcome_history  = 0.30
recency          = 0.10
tier_proximity   = 0.15
provenance_trust = 0.05

[recall_weights]
# 5090: semantic match matters more (room to hold more artifacts hot)
semantic         = 0.50
outcome_history  = 0.20
recency          = 0.10
tier_proximity   = 0.05
provenance_trust = 0.15
```

Sentinel observes which `recall → composition → outcome` chains produced good results and refines the weights *per persona over time*. A persona that consistently does better with sentinel-refined artifacts than foundry-imported ones gets a higher local `provenance_trust` weight. A persona that does better with semantically-distant-but-recently-used artifacts gets higher `recency`. This is profile-guided optimization of the recall function itself.

Sentinel writes its refinements to the governor as `RecallScoreWeights` updates with provenance. The governor applies them per persona (the policy carries a per-persona override table) and they propagate through the normal `arc_swap`-published policy. Sentinel-refined recall weights are also a publishable artifact in the genome pool — federated peers can adopt another instance's weights with the usual `provenance_trust` gating.

### Indexing — Sub-ms Local, Coordinated Grid

The recall index is a layered structure:

| Layer | Purpose | Backed by | Lookup cost |
|---|---|---|---|
| Working-set index | "is this artifact ref hot for this persona right now" | `HashMap<PersonaId, BTreeSet<ArtifactRef>>` | O(log n), in-memory |
| Local catalog | All artifacts in tiers L1–L5 with embeddings + metadata | sqlite + on-disk ANN index (hnsw) over embeddings | < 1 ms for top-K |
| Grid catalog | Federated peers' artifact summaries (id + embedding + provenance + last_seen) | gossip-propagated via the sharing protocol | < 5 ms cached; cross-peer fetch if cold |
| Federation catalog | The broader hive (opt-in) | pull-based, governor-rate-limited | bounded by `federation_pull_cadence` |

A recall query touches the layers in order. The first that satisfies the budget + freshness target wins. Most queries return from the local catalog (or even the working-set index for repeat-within-turn queries). Grid + federation catalogs are consulted only when the local set is insufficient or when the persona's `RecallScope` explicitly asks for them.

### Within-Turn Caching And Coalescing

A persona doing one turn often issues multiple recalls — initial context-gather, then re-recall after a tool-use, then again for response composition. These should not re-execute the full pipeline:

```rust
// PROPOSED — core/continuum-core/src/genome/recall/cache.rs
pub struct WithinTurnRecallCache {
    persona:    PersonaId,
    turn_id:    TurnId,
    by_query:   HashMap<QueryFingerprint, Arc<RankedPool>>,
    in_flight:  HashMap<QueryFingerprint, BroadcastReceiver<Arc<RankedPool>>>,
}
```

Two behaviors:

1. **Memoization within the turn.** Identical `CapabilityQuery` from the same persona in the same turn returns the cached `RankedPool` immediately. Cleared when the turn frame is released.
2. **Coalescing of concurrent identical queries.** If two cells in the same persona's turn issue the same query milliseconds apart, the second one subscribes to the first's in-flight `BroadcastReceiver` rather than re-executing.

Across personas, similar queries may not be identical (different `must_include` pins, different `PersonaContext`) so cross-persona coalescing is at the *sub-query* level: the embedding generation step coalesces (one embed call per unique query text), the catalog lookup step coalesces (one ANN query per unique embedding), the scoring step does not (each persona's `PersonaContext` differs).

### Cross-Instance Recall — The Grid Coordination Layer

When a recall's `RecallScope` is `LocalThenGrid` and the local catalog doesn't satisfy the budget, the substrate consults the grid. This is the ingenuity layer — the federated swarm has to coordinate without becoming a chatter storm.

Three rules:

1. **No instance queries the grid more often than its `federation_pull_cadence` allows.** Set per-hardware-class by the governor: Air ≈ once per 10 minutes; 5090 ≈ once per minute. This is the same cadence that publishes new artifacts; pull and push share a budget.
2. **Grid catalog is gossip-propagated, not query-on-demand.** Each instance publishes its artifact summaries (not the artifact blobs) on its `federation_pull_cadence`. Other instances cache the summaries. A recall query against the grid catalog hits the *local cache of the gossip*, not the live peer — sub-ms latency for what would otherwise be a multi-hop network query.
3. **Fetching a grid artifact blob requires explicit promotion.** A `RecallResult` containing a `ResidencyHint::GridPeer` does *not* fetch the blob until the persona's composition pins it. The substrate pulls the blob into the local L4 with provenance preserved; subsequent recalls find it locally.

The win condition: **a swarm of Airs gossiping summaries every 10 minutes produces a federated artifact catalog that's effectively realtime for the recall scoring function**, because the scoring function uses the cached summary, not the live blob. Only on pin does the blob move. This is how the architecture stays performant on cellular-class bandwidth while still letting the swarm coordinate at the level of "what exists, what's been refined, what's been retired."

### Replay Semantics

Sentinel attribution and VDD regression both require replaying a previous recall and getting the same `RankedPool`. The trait's `replay(trace)` method does this:

```rust
pub struct RecallTrace {
    pub trace_id:           TraceId,
    pub query:              CapabilityQuery,            // snapshot at recall time
    pub context_snapshot:   PersonaContextSnapshot,     // snapshot at recall time
    pub policy_version:     u64,                        // governor policy at recall time
    pub catalog_snapshot:   CatalogSnapshotRef,         // content-hashed; deterministic replay
    pub timestamp:          SystemTime,
    pub returned_pool:      RankedPool,                 // for outcome attribution
}
```

A replay re-runs `score()` over the snapshotted catalog with the snapshotted weights. The result is deterministic and bit-equal to the original `returned_pool`. Sentinel uses this to attribute "did the artifact I refined actually win the ranking on the turn it should have?" — without it, sentinel can't tell the difference between "my refinement helped" and "the artifact I refined just happened to be hot when it ran."

### Recall Under Pressure

The governor's cascade (Part 11) affects recall in defined ways:

| Cascade step | Effect on recall |
|---|---|
| 0 (normal) | full pipeline; grid + federation as requested |
| 1 | speculation deprioritized; recall returns slightly smaller pools (top-K reduced) |
| 2 | grid pulls deferred unless `RecallScope::Federation` explicit; otherwise local-only |
| 3 | working-set index is the only fast layer; ANN index falls back to higher-error / faster K |
| 4 | federation pulls suspended; grid catalog stale-served |
| 5 | recall caps at L1+L2 only; cold-archive lookups return `Deferred(MemoryPressure)` |

Recall under pressure is *correct* — it doesn't lie, doesn't return placeholders. It returns smaller, more-conservative pools with explicit `ResidencyHint::Deferred` entries when an artifact exists but can't safely be promoted. The persona's composer sees this and either narrows its composition or defers the turn — never silently degrades.

### Performance Budget

Recall is in the hot path. The budget is tight:

| Operation | Air target | 5090 target |
|---|---|---|
| Within-turn cache hit | < 50 μs | < 30 μs |
| Working-set index hit | < 200 μs | < 100 μs |
| Local catalog (ANN top-K) | < 5 ms | < 2 ms |
| Grid catalog (cached gossip) | < 5 ms | < 5 ms |
| Federation catalog (cached) | < 10 ms | < 10 ms |
| Federation pull (cold) | bounded by `federation_pull_cadence`, off hot path |

The first three rows cover ≥ 95% of recalls. The substrate's acceptance criteria includes a smoke test that verifies P50/P99 against these budgets on both anchors.

### Why This Earns Its Space In The Doc

Recall is where the architecture wins or loses on consumer hardware. A naive recall that hit GitHub or HuggingFace for every query would make the system unusable on cellular bandwidth. A purely local recall would forfeit the federation's collective intelligence. The substrate's win is that recall is **local-first, gossip-aware, sentinel-refined, governor-tuned, cost-visible to the persona, and deterministic in replay** — five properties that together let an Air running solo, a 5090 running solo, and a swarm of Airs + 5090s all use the same Rust code path and all benefit from each other's evolved genome. That's the dynamicism-across-the-grid claim made concrete.

### Live Wiring Status & Completion Plan (2026-07-13)

The *design* in this Part is complete; the doc predates the genome actually running in live cognition, so this is the completion — "make it work," dogfooded in the benchmarked path.

**What is real in tree:** `LocalDemandAlignedRecall::rank(candidates) -> RankedPool` (`genome/recall_impl.rs`) does the cosine + five-factor `RecallScore` ranking. The `CandidateSource` trait is the seam between the ranker and the substrate sources. `CapabilityQuery` (`task_kind`, `domain_hints`, `budget`, `scope: RecallScope`, `prefer_refined`, `freshness_target`) is the query. `ArtifactId` = content-hash (the Merkle line); `ResidencyHint` = cache level; `RecallScope` (`Local` / `LocalThenGrid` / `Federation`) = locality domain. Cognition already holds an `EmbeddingProvider` (`cognition/persona_workspace.rs`), so query- and layer-metadata embeddings are available.

**The hardcode being removed (the whole point):** live cognition builds the persona's `genome` handle from `empty_genome()` (`persona_workspace.rs`) and the actually-served LoRA is *pinned at llama-server spawn* by a per-persona/per-skill **path/skill-name string** — the non-cosine, non-UUID form. The genome is never *recalled*; there is no cosine/UUID selection of the best-fitting layer for the current task. This is the thing to complete, and it lives inside the cognition we benchmark, so completing it moves the measured number.

**The unifying frame (why this is sound):** the whole market is a **content-addressed, fuzzy, multi-level cache**. `TierRole` Fast/Warm/Bench/Cold/Frozen = cache levels; `RecallScope` repo→home→grid→HF = locality domains; `ArtifactId` content-hash = the tag; **cosine = associative lookup** (nearest line, not exact tag); cost-to-promote = miss penalty; repo-local `.continuum/` shadows home like L1 shadows L2 (nests like `.gitignore`, per REPO-GENOME-AND-COURSES §8). Two ways richer than silicon: lookup is *fuzzy* (cosine), and **writeback improves the line** — fork+enhance+score, write the better one back up (federated learning in a cache's clothes). "New layer vs improved layer" is not a storage division — it's git-for-weights: one append-only content-addressed DAG, lift-gated promote, first-class fork (REPO-GENOME-AND-COURSES §16). **The load-bearing keystone that makes cross-peer federation work is a *standard generic exam*** — scores are only comparable on the same ruler; it's the shared coordinate system, not a side feature.

**Completion slices (each ships alone, tested; dogfooded in the benchmark):**
1. **`GenomeStoreCandidateSource`** — a `CandidateSource` impl over the local genome store (`TierStore`) + the paging engine's known adapters: enumerate the persona's actual layers as `CandidateArtifact`s (`artifact_id`, `residency`, `last_used_ms`, trust). Unit-tested against a fixture store.
2. **Embed metadata + query** — embed each layer's card/keywords (title/domain) and the current task/need via the resident `EmbeddingProvider`; fill `semantic_factor` (cosine) + `outcome_history_factor` (from sentinel/benchmark). Content-embeddings computed once, shared ([[embeddings-are-per-content-computed-once-shared]]).
3. **Recall→genome page-in** — on task/`Situation` change, `recall(CapabilityQuery)` → top `LoRALayerRef` → page into the `genome` ArcSwap (the existing page-in wire), replacing `empty_genome()`/the pinned path. `RecallScope::Local` first; the repo-local vs home split (§8) is the storage refinement behind it.
4. **Prove** — with/without-recall benchmark arms (REPO-GENOME §11) on the same generic exam; recall-selected genome must beat the pinned/empty baseline on the measured number.
5. **Publish/adopt (L4/L5)** — forge/publish a lift-gated layer to `continuum-ai` on HF with its card + lineage attestation (#99); fetch+cosine-rank candidates from HF through the same `DemandAlignedRecall` interface (#100). P2P is a later transport swap behind the unchanged interface.

## Part 8: Composition

A persona's effective model at any moment is a **dynamic composition** of base + tiered LoRA + MoE expert routing + engram-conditioned context. Composition is recomputed when the task / context / pressure shifts; otherwise the substrate caches it.

```rust
// PROPOSED — core/continuum-core/src/genome/composition.rs
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
    pub role_at_plan: TierRole,                    // which tier role this layer occupied when planned
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
// PROPOSED — core/continuum-core/src/genome/speculation.rs
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
// PROPOSED — core/continuum-core/src/genome/sharing.rs
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
// PROPOSED — core/continuum-core/src/governor/mod.rs
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
// PROPOSED — core/continuum-core/src/governor/detect.rs
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
// PROPOSED — core/continuum-core/src/governor/cascade.rs
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
// PROPOSED — core/continuum-core/src/governor/runtime.rs
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
