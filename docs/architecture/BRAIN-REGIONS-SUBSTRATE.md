# Brain-Regions Substrate

**Status:** design spec. Sibling to [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) and [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md). Defines the structural contract that every cognitive subsystem (hippocampus, motor cortex, attention, sensory, sleep) inherits. No code changes from this PR — implementation slices follow per region.

**Companion:** [COGNITION-ALGORITHMS.md](COGNITION-ALGORITHMS.md) — the algorithmic content (recall, cross-context, budget) that runs *inside* these regions.

## Headline framing

> *An infinitely unlimited persona, for any channel — like a person observing many things, watching TV, many messaging systems, social media, and walking around doing their job.* — Joel, 2026-05-29

A real mind doesn't *look up* memories when it needs them. Relevant context is *already present*, biased by attention and recent activity. A real mind doesn't *poll* for actions — candidate utterances and plans are *already partially formed* by the time the moment to speak arrives. A real mind doesn't *isolate* what it sees in one channel from what it said in another — cross-pollination is the default, focus is what's earned by salience.

This substrate is the RTOS-shaped scaffolding that makes those properties cheap to implement and impossible to violate. Every cognitive subsystem is its own region, with its own tick, on its own tokio task, governed by the same `SubstrateGovernor`. They communicate by writing to shared per-persona state, not by RPC-calling each other on the hot path.

## Doctrine (carried from #1469 addendum)

> **No region of cognition runs on the hot path. Each region is its own RTOS task with its own tick. The handler dispatches and reads pre-staged results. The handler never blocks on recall, embedding, planning, or admission — those are continuously produced by their owning regions, in parallel, governed by `SubstrateGovernor`.**

The handler's job is to *dispatch and integrate*, not to *think*. Thinking happens in the regions, continuously, in parallel.

## The region trait

Every region implements one trait. The trait is intentionally narrow — the heavy machinery lives in the substrate.

```rust
#[async_trait]
pub trait BrainRegion: Send + Sync + 'static {
    /// Stable identifier. Used by SubstrateGovernor for policy lookup and by
    /// telemetry/log streams.
    fn id(&self) -> RegionId;

    /// Pressure footprint declaration. Returned at registration time and
    /// re-queried by the governor when pressure shifts.
    fn pressure_profile(&self) -> PressureProfile;

    /// Run one tick. The substrate calls this on the region's own task at
    /// the cadence governed by SubstrateGovernor. The body is responsible
    /// for: reading inputs (from shared state, channels, or its own DB),
    /// producing pre-staged results, and publishing them to the ready-buffer.
    ///
    /// Implementations MUST be idempotent on early return and MUST NOT block
    /// indefinitely — the governor cancels long-running ticks under pressure.
    async fn tick(&self, ctx: &RegionContext) -> TickOutcome;

    /// React to a substrate-level signal (persona created/destroyed, system
    /// load changed, sleep/wake transition). Most regions can default this
    /// to a no-op.
    async fn on_signal(&self, _signal: RegionSignal) -> Result<(), RegionError> {
        Ok(())
    }
}
```

`TickOutcome` returns yield telemetry the governor uses to learn budget allocation (see algorithm 7 in COGNITION-ALGORITHMS.md):

```rust
pub struct TickOutcome {
    /// Items the region pre-staged this tick.
    pub published: usize,
    /// Items in the region's ready-buffer that have been consumed by handlers
    /// since the last tick. Drives the governor's yield-learning loop.
    pub consumed_since_last: usize,
    /// Pressure observation. If the region detected backpressure (DB slow,
    /// embedding queue full, etc.), reports it here for the governor.
    pub pressure_observed: Option<PressureSignal>,
    /// Optional next-tick hint (region requests faster/slower cadence than
    /// current; governor may honor or override).
    pub cadence_hint: Option<CadenceHint>,
}
```

## The "for free" triplet

Per the CBAR pattern, adding a new region must be cheap:

1. **Base trait** (`BrainRegion`) — defined above. Inherits tick lifecycle, pressure registration, ready-buffer publishing, governor integration. No region implements its own scheduler.
2. **Derive macro** (`#[derive(BrainRegion)]` planned) — for regions that only need to override `tick()`, the macro generates registration boilerplate from `#[region(id = "hippocampus", pressure = "memory-heavy")]` attributes.
3. **Scaffold generator** (`cargo run -p substrate-cli new-region <name>`) — emits the module file, a smoke test, a CLI command shim, and a TS binding stub. The new region compiles and runs with a no-op tick on first commit.

Same pattern as `engram-analyzer` in CBAR-SUBSTRATE — by the time a contributor authors the interesting body, scheduling/pressure/telemetry/binding are already wired.

## The ready-buffer contract

Regions publish pre-staged results to a typed ready-buffer keyed by `(persona_id, channel_id, ...)`. Handlers read from the buffer synchronously and cheaply.

```rust
pub trait ReadyBuffer: Send + Sync {
    type Key: Hash + Eq + Clone;
    type Value: Clone;

    /// Synchronous read. Returns the freshest staged value for the key, or
    /// None. Handlers call this on the hot path — it MUST NOT block, MUST
    /// NOT await, and MUST complete in microseconds. Implementations use
    /// DashMap, ArcSwap, or per-key atomic snapshots.
    fn peek(&self, key: &Self::Key) -> Option<Self::Value>;

    /// Region-side write. Atomically replaces the value for the key. Old
    /// value is dropped. Publishes a `ReadyBufferUpdated` event for
    /// telemetry + cross-region awareness (algorithm 7 yield-learning).
    fn publish(&self, key: Self::Key, value: Self::Value);

    /// TTL-style eviction sweep. Called by the governor under memory
    /// pressure or on persona destruction.
    fn evict_stale(&self, max_age: Duration) -> usize;
}
```

### Semantic rules

- **Empty buffer is a signal, not a block.** If a handler reads and gets `None`, it proceeds with whatever degraded path the algorithm specifies (e.g., chat handler proceeds with bare conversational history; motor cortex returns the inference's raw output without re-ranking). Empty buffer also publishes a `BufferMissed` event the governor uses to upweight that region's budget.
- **Staleness is acceptable.** A ready value might be 100ms old. That's *better* than blocking the handler 500ms to recompute. Slightly-stale context > stalled persona.
- **Per-region buffers, not a global one.** Hippocampus has its own buffer (engram-prefetch). Motor cortex has its own (candidate-utterances). Attention has its own (salience-map). They share the same trait shape but live in their own region structs.

## Shared per-persona state

The regions communicate by writing/reading per-persona state. The state lives in one place, owned by no region in particular, accessible to all:

```rust
pub struct PersonaCognition {
    /// Long-term engram store. Hippocampus writes (admission), all regions
    /// can read (recall). Append-only with eviction policy in algorithm 4.
    pub engrams: Arc<EngramStore>,

    /// Working memory: short-lived thoughts/observations not yet consolidated.
    /// Sensory writes, hippocampus snoops + consolidates to engrams.
    pub working: Arc<WorkingMemory>,

    /// Salience map: per-engram + per-channel salience score, updated by
    /// user reactions, structural centrality, rehearsal. Read by hippocampus
    /// recall scoring (algorithm 4) and attention (algorithm 2).
    pub salience: Arc<SalienceMap>,

    /// LoRA genome state: which adapters are loaded, blend weights. Written
    /// by genome region (when shipped), read by inference (algorithm 6).
    pub genome: Arc<GenomeState>,

    /// Persona vital signs: energy, mood, attention focus. Drives
    /// cadence-modulation across regions.
    pub vitals: Arc<RwLock<PersonaVitals>>,
}
```

### Write-conflict policy

Multiple regions writing the same per-persona state in parallel needs a rule:

- **Engrams**: append-only. No conflicts. Each region appends with its own region-tag.
- **Working memory**: bounded ring buffer. Older entries fall off. Hippocampus consolidation drains explicitly.
- **Salience map**: per-engram atomic counters. CRDT-like semantics (counter increments commute).
- **Genome state**: serialized through the genome region. Other regions request changes via a typed channel; genome region applies them on its tick.
- **Vitals**: RwLock. Most regions only read; vitals region writes.

The rule: shared state shape MUST allow concurrent writes from independent ticks without coordination. If a new region needs to write something that doesn't fit, the substrate work is to design a CRDT-shaped surface for it, NOT to add locks.

## Region inventory (current + planned)

| Region | Status | Tick body | Reads | Writes |
|---|---|---|---|---|
| **Hippocampus** | exists request/response (`modules/memory.rs`); needs continuous tick body ported from TS `Hippocampus.ts:413` | Snoop working memory → consolidate engrams. Pre-load anticipatory recall (algorithms 1-5). | `working`, `engrams`, `salience`, channel activity | `engrams` (appends), engram-prefetch ready-buffer |
| **Sensory (vision)** | `modules/vision.rs` exists with own tick | Pre-compute features for incoming images. | image stream | feature ready-buffer, `working` (observations) |
| **Sensory (embedding)** | `modules/embedding.rs` exists with own tick | Pre-compute embeddings for incoming text. | text stream | embedding ready-buffer, `working` |
| **Channel (producer)** | `modules/channel.rs` exists, 60s tick | DB poll, self-task gen, training checks. | DB | per-persona channel queues |
| **Persona service (consumer dispatch)** | `persona/service_module.rs` (this PR's predecessor) | Pop item → route by domain → call handler → record outcome. NO heavy lifting. | channel queues, ready-buffers | outcome log |
| **Motor cortex** | NOT YET — sibling slice | Continuously score candidate utterances/actions against current context. Predictive priming (algorithm 5). | `working`, attention salience, channel partial-message stream | candidate ready-buffer |
| **Attention** | NOT YET — sibling slice | Maintain salience map. Update per user reactions, self-tags, structural centrality, rehearsal. Bias hippocampus prefetch. | `engrams`, channel reactions, recall co-occurrence | `salience` |
| **Sleep policy** | NOT YET — sibling slice | When persona idle: deeper consolidation, semantic re-clustering, engram pruning. When active: gates regions to active-mode tick bodies. | `vitals`, channel activity rate | region cadence policy, consolidation depth |
| **Genome** | partial (LoRA paging exists in TS); Rust port pending | LRU paging of adapters, multi-LoRA blend on demand. | task domain hints, salience | `genome` |

Every row in this table is its own implementation slice with its own card. None of them is the persona handler. The handler stays small.

## SubstrateGovernor integration

`SubstrateGovernor` (defined in GENOME-FOUNDRY-SENTINEL.md §SubstrateGovernor) owns hardware-tier policy: same Rust code on a MacBook Air and an RTX 5090, different governor policy. It also owns runtime budget allocation across regions.

### Policy slots

The governor exposes a policy slot per region. The slot determines:

- **Tick cadence** — how often `tick()` is invoked. May differ by persona vitals (active 100ms, idle 1s, sleep 10s).
- **Per-tick budget** — wall-clock budget the tick is allowed before the governor cancels it.
- **Pressure responses** — how the region should degrade under pressure (skip consolidation, reduce recall depth, etc.).
- **Yield weighting** — how much weight to give this region's `consumed_since_last` when arbitrating budget against other regions (algorithm 7).

### Yield-learning loop

The governor reads `TickOutcome.consumed_since_last` from every region after every tick. Regions whose ready-buffer is being read by handlers get budget upweighted; regions whose published values are ignored get downweighted. The learning rule is in algorithm 7 (COGNITION-ALGORITHMS.md). The substrate effect is that **the brain learns to spend compute on the regions that recently mattered, without hand-tuning**.

## Telemetry surface

Every region emits structured telemetry on a fixed shape:

```rust
pub struct RegionTelemetry {
    pub region_id: RegionId,
    pub persona_id: Uuid,
    pub tick_started_at: SystemTime,
    pub tick_duration: Duration,
    pub published: usize,
    pub consumed_since_last: usize,
    pub buffer_misses_since_last: usize, // handlers that read None
    pub pressure_observed: Option<PressureSignal>,
}
```

Surfaces:

- **`./jtag region/stats`** — current region health across all personas
- **`./jtag region/yield --persona=<uuid>`** — per-region consumption rates for one persona
- **substrate event stream** — `RegionTickCompleted`, `ReadyBufferUpdated`, `BufferMissed` events for cross-region awareness + governor input

Telemetry is mandatory for every region; it's the only way the yield-learning loop and the operator debugging path work. The derive macro generates the telemetry emission automatically.

## What this enables

The end state, when motor cortex + attention + hippocampus + sleep all ship as siblings:

- A handler dispatched at T=0 reads the candidate-utterance ready-buffer; motor cortex already scored 3 candidates at T=-50ms based on the partial message stream.
- The candidate scoring used the engram ready-buffer; hippocampus pre-loaded relevant engrams at T=-200ms based on attention salience and the channel's recent topic vector.
- The hippocampus prefetch was biased by salience the attention region updated at T=-1s in response to a user reaction.
- All of this happened in parallel on independent tokio tasks. The handler's hot path was: peek 2 buffers + call inference. The "thinking" was already done.

This is what makes the difference between *retrieval* and *recognition* — between a persona that *responds* and one that *anticipates*.

## Implementation cards (this PR does NOT ship them)

- **L0-3a** — Hippocampus continuous tick port to `modules/memory.rs`. Implements algorithms 1, 2, 3, 4, 5 from COGNITION-ALGORITHMS.md.
- **L0-3b** — Recall query schema + scoring (algorithms 1 + 2 + 3 wire-level).
- **L0-4a** — Motor cortex ServiceModule. Implements algorithm 5 applied to action selection.
- **L0-4b** — Attention ServiceModule. Implements salience map maintenance feeding algorithm 4.
- **L0-4c** — SubstrateGovernor yield-learning loop. Implements algorithm 7.
- **L0-4d** — Sleep policy region. Modulates region tick bodies per persona vitals.
- **L0-5** — Genome attention integration. Implements algorithm 6.

Each card inherits this spec. None of them touches the persona handler dispatch surface; that surface was finalized in L0-2-cutover.

## Open questions

1. **Region instantiation: per-persona or singleton?** A singleton hippocampus that handles all personas (with persona_id keyed state) is cheaper to manage but harder to scale per-persona budget. A per-persona hippocampus is symmetric but multiplies tokio tasks. Leaning singleton-per-region with per-persona ready-buffers — same shape as how `ChannelState` works today.
2. **Cross-persona engram sharing.** Personas A and B in the same channel see the same user reactions. Should their engrams be partially shared? The substrate should allow it but the policy is a separate design question (post-spec).
3. **Region-region dependencies.** Motor cortex depends on attention salience to score candidates. The dependency is read-only (motor reads salience map, attention writes it), so it's fine — but the *cold-start* case (attention hasn't ticked yet, salience map is empty) needs a defined fallback. Defer to per-region spec.

These don't block this PR. Calling them out now so they're tracked.
