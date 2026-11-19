# The substrate: CBAR in Rust

**Status:** the runtime contract every Rust module inherits: stages, the for-free triplet, RTOS rules,
the declared dataflow graph, and the measurement every stage gets for free. Moved here whole from
CONTINUUM-ENGINE.md (its former sections 2, 3 and 7) so that chapter describes only the engine.
This chapter is next to be refined in place (owner: Fable). Until then its text is as it was, and its
"section N" references still use CONTINUUM-ENGINE.md's former numbering: that chapter's "Former section
numbers" table resolves them.
The stage types themselves are specified in [STAGE-CONTRACT.md](STAGE-CONTRACT.md); budgets and
residency in [SHARED-RESOURCES.md](SHARED-RESOURCES.md); a mind's concurrent regions in
[CONCURRENT-MIND-AND-GOVERNOR.md](CONCURRENT-MIND-AND-GOVERNOR.md).

## 1. Non-negotiable: every optimization, exhausted

Joel, 2026-11-13: "use the graphics pipeline approach and study the unreal engine optimizations";
"Carmack level efficiencies, avoiding squares even in the GPU"; "never parse"; "avoid rasterization
and cpu/gpu transfer whenever possible"; "use raw blobs and bytes unless you truly need the
encoding"; "do not waste cpu ever for anything it is unnecessary for". This is the review standard
for every data, storage, retrieval and serving change. A change that cannot say which row it
follows, and show its end-to-end timing and output quality before and after, is not ready.

### 1.1 What the CPU does per byte

The first question for any data path: what does the CPU touch per byte, and can it be zero?

- **Bytes stay bytes.** A producer's native bytes are stored and moved as they are (a BLOB beside a
  typed descriptor: element type, shape, byte order, producer identity). No translation on write,
  read or in between. A typed view over bytes, never a walk that converts them.
- **No per-element CPU loops.** A load is one bulk memcpy or zero copies. Converting, normalizing or
  validating element by element on the CPU is a design bug.
- **Translation, when unavoidable, happens on the GPU** inside the kernel that consumes the data
  (f16/int8 to f32, dequantization, layout), never as a CPU pass and never per use.
- **Never parse until the absolute last step.** JSON only at a true external boundary, or where the
  store must index inside a document (then the store's binary JSON). Rows decode straight to the
  typed columns a caller asked for; a payload stays opaque until its addressed consumer opens it.
- **No SQL outside the ORM, no Python anywhere.** The schema declares the indexes and projections, so
  filter, sort, count and limit run inside the store. Rust only; bash only for instance scripts.
- **No transfers that are not needed.** On unified memory (the M5) the stored pages are the GPU
  buffer (mmap + a no-copy Metal buffer). On a discrete GPU (the 5090) one asynchronous DMA upload,
  overlapped with compute and double-buffered. The CPU never stages data and reads back only the
  answer.

### 1.2 Carmack rules for the math

- **Never compute what does not change the answer.** Ranking needs order, not distance: squared
  distances, never a square root; for cosine, store unit vectors (normalize once at write) so
  similarity is a dot product with no norms, roots or divides; normalize the query once per search.
- **Hoist everything invariant out of the inner loop.** A per-candidate recompute of the query's
  norm, or of a candidate's norm that never changes, is the anti-pattern
  (`continuum-kernel/src/orm/vector.rs::similarity::cosine`, `continuum-store` `data_state.rs`
  cosine_similarity, both found 2026-11-13).
- **Coarse first, exact only where it decides.** A sign-bit sketch with popcount (12 64-bit ops for
  768 dims) culls; exact dot products run on the shortlist; partial top-k selection, never a sort.
- **Spend only the precision the consumer perceives**, with the error bound stated and checked by
  a validator (recall@k against the exact answer), never assumed.
- **No branches, asserts or allocation in inner loops.** Validate once at the boundary.

### 1.3 Unreal Engine systems and their counterparts here

| Unreal | What it does | Ours |
|---|---|---|
| Render Dependency Graph (RDG) | Passes declare the resources they read and write; the graph schedules them, inserts barriers, aliases transient memory, overlaps independent passes on async compute | An act is a graph: perception, retrieval, prefill, decode, tools. Transient buffers come from per-act arenas, prefill overlaps the tool call, no hand-sequenced awaits (section 3) |
| Virtual texturing / streaming pool | Only visible pages resident inside a fixed pool; under pressure the mip bias rises (coarser), never a failure | KV pages and context resident by predicted need inside one memory budget; under pressure the view collapses to coarser levels and the engine's host cache gives memory back (fork #110, card 494c99f1). A fixed `--cache-ram` is a texture pool that never evicts (the M5, 2026-11-13: 60 MB free, prefill 92 tok/s against ~190) |
| Nanite clusters, LOD by screen-space error | Detail chosen per cluster by how much it matters on screen | The context codec's levels (whole, summary, folded) chosen per item by predicted relevance (section 5) |
| GPU-driven culling (HZB, cluster culling, indirect draws) | The GPU culls millions of instances hierarchically; the CPU never touches per-object work | Vector search as one GPU dispatch over the resident matrix: sketch cull, exact dot on survivors, in-kernel top-k; only k indices and scores come back |
| Instancing | One mesh, many draws | One shared prefix (system prompt, tools) in KV, copied across minds (`seq_cp`), never re-prefilled |
| Temporal reuse (TAA, TSR) | Reuse last frame, render only the delta | Epoch context: an act prefills only its appended tail on the cached prefix |
| Dynamic resolution, scalability profiles | Hold the frame-time target by scaling quality; settings per hardware tier | Hold the act-time target by scaling window and precision; capability profiles per node (M5, 5090, IntelMac) |
| Async loading | Assets stream in behind handles; nothing blocks the frame | Handles and events for every long operation; nothing blocks an act |
| Compute over rasterization | A compute pass where a raster pass would redraw | Avatar frames from compute or reused tiles; nothing redrawn that did not change |

### 1.4 The exhaustive checklist (take every one that applies; a skipped one is named with its reason)

Joel: "It's totally crucial we take every optimization. I always exhaust them." Before a data,
compute or serving change is reviewed, its author walks this list and states, for each line that
could apply, either that it is taken or the measured reason it does not pay.

**Data and memory**
- Native bytes, typed descriptor, no transcoding; zero copy where alignment allows, one bulk memcpy otherwise.
- Structure of arrays for anything scanned (vectors as one contiguous matrix, not a Vec of structs).
- Cache-line alignment for hot data; no false sharing between threads' counters.
- Arenas and pools: buffers owned once, passed by handle, recycled; no allocation on a hot path.
- Bounded resident sets by predicted reuse; everything that accumulates drains; nothing holds what it is not using.
- Precision chosen per consumer (f16, bf16, int8 with scale) with a stated, validated error bound.
- Compression only where it is cheaper than the bandwidth it saves, measured.

**Compute**
- Hoist invariants; precompute at write what every read would recompute (unit norms, sketches, prefix sums).
- No roots, divides or transcendental functions where a comparison, a reciprocal multiply or a lookup decides the same.
- Branchless inner loops; no asserts, logging or allocation inside them.
- SIMD on the CPU only when the GPU is not the better owner; fused GPU kernels over separate passes.
- Coarse then exact: sketch, cull, shortlist, exact on survivors; partial top-k, never a full sort.
- Incremental update over recompute: append to the resident matrix, the cached prefix, the index.

**Transfers and I/O**
- Unified memory: the stored pages are the GPU buffer (mmap + no-copy). Discrete: one async DMA, double-buffered, overlapped with compute.
- Read back only the answer (k indices and scores, a pooled vector, a token), never whole intermediates.
- Sequential, mmap-backed reads; write coalescing; drained writers behind ring buffers, never on the request path.
- Binary frames on every wire; JSON only at a true external boundary.

**Scheduling and concurrency**
- The act is a dependency graph: independent passes overlap (prefill during tools, retrieval during perception).
- Batch across minds (decode, embedding, search); never queue minds one at a time in front of a batching engine.
- Wake on events and handles; never poll, never spin, never hold a lock across an await.
- Drop or defer stale work instead of draining obsolete queues.
- Hold the act-time target by scaling window and precision, never by refusing.

**Reuse**
- Instancing: one shared prefix in KV, copied, never re-prefilled.
- Temporal reuse: prefill only the appended tail; render only what changed.
- Content-addressed caches: compute once, keep while predicted reuse beats recompute.

### 1.5 How a change proves itself

Frame time is act time. Every change here posts, on a live node and on a replay validator:
act latency (median and p90), the stage it targets (prefill tok/s, decode tok/s, search latency,
store op p50/p99), memory (free, compressed, swap, phys_footprint), and output quality (recall@k,
review outcome). A health line's label is not a measurement.

### 1.6 Order of work (2026-11-13)

1. The streaming pool: fork #110 (live `/prompt-cache/limit`) + the host deficit's shed path, so the
   engine's cache gives memory back first under pressure and takes it back when the OS reads normal
   (card 0f631c46; the deficit is judged by the OS's own pressure verdict, never by our counters).
   Measured on the M5: prefill tok/s, decode tok/s, free memory, act latency.
2. GPU-driven vector culling: unit vectors at write as raw f16/f32 bytes, a no-copy GPU buffer, one
   GEMV dispatch with fused top-k and a popcount prefilter. Measured: 10k/100k search latency and
   recall@k.
3. The act as a dependency graph with transient arenas and prefill overlapped with tools.

## 2. The substrate

**Authoritative precedent**:
`/Users/joelteply/Development/cambrian/cb-mobile-sdk/cpp/cbar`

CBAR matters because of its engineering philosophy, not because Continuum
should copy every class literally. It is a small-code, high-throughput,
RTOS-style runtime where each concern gets threading, cadence, shared frame
artifacts, logging, lifecycle, and performance behavior almost for free.
Continuum needs that same shape for persona cognition, inference, memory,
WebRTC, Bevy/rendering, ORM/data, and grid work.

### Core Philosophy

CBAR's lesson is:

- Put the hard machinery in the substrate.
- Keep each concern small.
- Give modules a narrow contract.
- Pass handles and shared frames, not copied memory.
- Let independent work run independently.
- Wake work from dependency readiness, state change, cadence, or explicit
  events.
- Drop or defer stale work instead of draining obsolete queues.
- Use GPU/SIMD/BLAS where available inside the artifact/module, not in wrappers.
- Make low-end hardware viable by reducing cadence and precision under
  pressure, not by turning the architecture into synchronous FIFO.

That is the target for Continuum. Rust owns the substrate. TypeScript and other
wrappers ask for work and display results.

### What CBAR Actually Does

The important C++ pieces:

- `CBAR_VideoFrame`: one frame object with raw input plus cached derived
  artifacts. It lazily imports/derives RGB, HSV, upright images, edges,
  optical-flow scale images, enhanced images, and metadata.
- `CBAR_VideoThread`: a bounded `QueueThread<CBAR_VideoFramePtr>` base that
  gives subclasses queueing, thread lifecycle, timing/FPS, flush, abort, join,
  and a tiny `handleFrame` override.
- `CBP_AnalyzerThread`: a concern class that declares whether it needs color,
  realtime, or video-only frames and implements only the relevant analysis.
- `CBP_Analyzer`: the fanout coordinator. Realtime analyzers run immediately;
  delayed analyzers run on cadence. Analyzer threads can be appended or removed
  without rewriting the engine.
- `CBP_RenderingEngine`: the opaque runtime owner. Public methods stay small;
  implementation state, frame state, scene state, locks, caches, rendering, and
  analyzer lifecycle stay behind `Impl`.
- `RawFrame.textureID`: proof of the handle-first mindset. The frame can carry
  a GPU/texture identity instead of forcing every boundary to copy pixels.

The result is a performant system where adding a new concern is usually short:
derive from the base, declare needs/cadence, implement `handleFrame`, and let
the substrate do queueing, lifecycle, logging, and scheduling.

### Continuum Translation

Continuum already has the first half of this pattern in
`core/continuum-core/src/runtime/`. The shipped substrate is:

```rust
// core/continuum-core/src/runtime/service_module.rs
pub trait ServiceModule: Send + Sync + Any {
    fn config(&self) -> ModuleConfig;
    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String>;
    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String>;
    async fn handle_event(&self, event_name: &str, payload: Value) -> Result<(), String>;
    async fn tick(&self) -> Result<(), String>;
}

pub struct ModuleConfig {
    pub name: &'static str,
    pub priority: ModulePriority,
    pub command_prefixes: &'static [&'static str],
    pub event_subscriptions: &'static [&'static str],   // string globs today
    pub needs_dedicated_thread: bool,
    pub max_concurrency: usize,
    pub tick_interval: Option<Duration>,
}
```

`ServiceModule` already gives Continuum: registry-mediated discovery
(`ModuleContext::registry`), event bus pub/sub (`ModuleContext::bus`), the
shared lazy-compute cache that fills the role `CBAR_VideoFrame`'s lazy getters
played (`ModuleContext::compute` over `SharedCompute`), a tokio runtime
handle, a periodic tick, and command routing. `ResourceClass` and
`TargetSilicon` are shipped under `cognition/adaptive_throughput.rs`.
`PressureBroker` and `ThroughputLease` are shipped under `paging/broker.rs`
and `cognition/throughput_lease.rs`. Bootstrap PR-1/2/3 (#1307 / #1308 /
#1310) put the broker on the runtime; PR #1313 added the lease broker.

What's missing is the *richer* contract — the one CBAR analyzers had through
`CBAR_VideoFrame` artifact pulls plus `needsColorFrames`/`needsRealTime`/
`videoOnly` routing flags. Continuum needs that contract because N personas,
RAG builders, model planners, memory jobs, and bridge observers may all be
waiting on different artifacts from the same turn:

```rust
// PROPOSED — extends ServiceModule, does not replace it. Each new type below
// is a Lane D deliverable; see section 8.1 (rows SB-1 to SB-8) for assignment.
pub trait RuntimeModule: ServiceModule {
    /// Typed artifact subscriptions, replacing the string-glob
    /// `event_subscriptions` field. The runtime uses this to wake only the
    /// useful work and to coalesce duplicates across personas.
    fn subscriptions(&self) -> &[ArtifactSelector];

    /// Typed cadence policy, generalizing the present
    /// `tick_interval: Option<Duration>` + `ModulePriority` pair. Encodes
    /// realtime / delayed / on-dependency-ready / on-pressure-change.
    fn cadence(&self) -> CadencePolicy;

    /// Frame-shaped handler. Receives the immutable per-turn frame and the
    /// existing `ModuleContext`. Returns a typed result that includes
    /// `Deferred(reason)`, `Coalesced(into)`, and `Failed(typed_error)` so
    /// silence is never a success.
    async fn handle_frame(
        &self,
        frame: Arc<RuntimeFrame>,
        ctx: &ModuleContext,
    ) -> ModuleResult;
}
```

The richer contract is the smallest superset of `ServiceModule` that lets the
substrate wake work from dependency readiness instead of pub/sub strings and
treat the persona turn as a single shared frame instead of N independent
event handlers. `ArtifactSelector`, `CadencePolicy`, `RuntimeFrame`, and
`ModuleResult` are the four proposed-new types this lane lands.

The substrate provides — today and after Lane D — the following. The "after"
column is the target; the "today" column is what is already in canary:

| Today, on `ServiceModule`                            | After Lane D, on `RuntimeModule`                                       |
|------------------------------------------------------|-------------------------------------------------------------------------|
| String-glob event subscriptions                      | Typed `ArtifactSelector`                                                |
| `tick_interval` + `ModulePriority`                   | `CadencePolicy` (realtime / delayed / on-ready / on-pressure)           |
| Command + event routing                              | Frame-shaped handler over `RuntimeFrame`                                |
| `ResourceClass` + `TargetSilicon` declared per module| unchanged                                                               |
| `PressureBroker` admission                           | unchanged                                                               |
| `SharedCompute` lazy artifacts                       | promoted into `RuntimeFrame`'s lazy fields                              |
| Per-module logs/metrics via `module_logger`          | unchanged, now also keyed by frame id                                   |
| Flush/abort/shutdown via `ModuleRegistry`            | unchanged                                                               |
| ts-rs exported contracts                             | unchanged                                                               |

The module author provides — at either layer — only:

- what artifacts it needs (subscriptions)
- what resource lane it uses (`ResourceClass` + `TargetSilicon`)
- how often it should run (cadence)
- the small piece of actual work (`handle_frame` body)

That is the "for free" architecture. The next section makes it concrete.

### The "For Free" Triplet

Inheritance from a trait is not enough on its own. The CBAR pattern only feels
"free" because three things ship together:

1. **A base trait** that every module implements. (Today `ServiceModule`;
   tomorrow `RuntimeModule`.) Provides the contract.
2. **A derive macro** that wires the base contract's required behavior —
   timing spans, structured logging, metric emission, pressure-response,
   lease renewal — onto the module type at compile time. The author writes
   `#[derive(RuntimeModule)] struct EngramAnalyzer { ... }` once; the macro
   emits the boilerplate that would otherwise be ten files of glue.
3. **A scaffold generator** (`just scaffold-module <name>`) that drops a new
   module file pre-populated with the base trait impl, default `ModuleConfig`,
   a doc comment template, and the matching test file. The author edits four
   lines (name, subscriptions, cadence, handler body) and has a working
   module.

Today Continuum has piece (1) only. Pieces (2) and (3) are the rest of the
"for free" triplet — without them, every new module re-declares its own
concurrency, retry, logging, and pressure-response, which is the friction
Lane D and this section exist to remove.

#### Worked Example: A New Engram Analyzer

A reader should be able to trace exactly what the developer wrote, what they
got for free, and what tests they inherited. This is the test of the doc.

The developer types one command:

```bash
just scaffold-module engram-analyzer --lane Background \
    --target Cpu \
    --subscribes "memory.consolidation.window"
```

The generator emits `core/continuum-core/src/modules/engram_analyzer.rs`:

```rust
//! Engram analyzer — consolidates recent memory writes into compressed
//! engram artifacts on each consolidation window.

use continuum_runtime::{
    ArtifactSelector, CadencePolicy, ModuleContext, ModuleResult,
    ResourceClass, RuntimeFrame, RuntimeModule, TargetSilicon,
};

#[derive(RuntimeModule)]
#[runtime(
    name = "engram-analyzer",
    lane = ResourceClass::Background,
    target = TargetSilicon::Cpu,
    cadence = CadencePolicy::OnReady,
)]
pub struct EngramAnalyzer {
    // ... module-owned state, e.g. a handle to the engram store
}

impl EngramAnalyzer {
    pub fn new() -> Self { Self {} }
}

#[runtime::handler]
impl RuntimeModule for EngramAnalyzer {
    fn subscriptions(&self) -> &[ArtifactSelector] {
        &[ArtifactSelector::MemoryConsolidationWindow]
    }

    async fn handle_frame(
        &self,
        frame: Arc<RuntimeFrame>,
        ctx: &ModuleContext,
    ) -> ModuleResult {
        let window = frame.memory_consolidation_window().await?;
        let engram = self.compress(window).await?;
        ctx.engram_store().write(engram).await?;
        ModuleResult::ok()
    }
}
```

That is the entire file. Everything else is inherited:

| Concern                                  | Source                                                        |
|------------------------------------------|---------------------------------------------------------------|
| Module name, lane, target, cadence       | `#[runtime(...)]` macro attribute → `ModuleConfig`            |
| Registration with `ModuleRegistry`       | macro-generated `inventory::submit!` at module load           |
| Tokio worker / dedicated thread choice   | derived from `ResourceClass::Background` → tokio default pool |
| Memory pressure response                 | `PressureBroker` admits / defers `handle_frame`; if VRAM/RSS pressure rises, the macro-generated wrapper returns `Deferred(MemoryPressure)` before `handle_frame` is called |
| CPU pressure / device pressure response  | `ThroughputLease` renewal on lane `Background`; degrades cadence under pressure with a visible reason |
| Concurrency cap                          | from `ResourceClass`; `Background` is non-realtime so cap is shared with peer background work, not invented per-module |
| Queue / dedupe / coalesce                | `ArtifactSelector::MemoryConsolidationWindow` → shared frame; if 3 windows arrive in 100ms, the runtime coalesces and `handle_frame` runs once with the newest |
| Span / timing / structured log           | macro wraps `handle_frame` in `vdd_scope!`; first-token / queue-wait / execution-ms / RSS-delta land in the Standard VDD Record automatically |
| Failure path                             | `?` on any inner call → typed `ModuleResult::Failed(reason)`; the runtime emits the failure to the trace bus, never silently |
| `Deferred(reason)` and silence reporting | macro-emitted; `Deferred` is a first-class return, not an absence |
| Replay test fixture                      | scaffold drops `engram_analyzer_test.rs` with one replay fixture covering happy path + one `Deferred` case |
| ts-rs exported contract for UI/command   | `#[derive(RuntimeModule)]` registers the module name with the generated TS catalog; admin UI sees it without code edits |
| Flush / abort / shutdown                 | `ModuleRegistry` lifecycle; analyzer is dropped cleanly when broker enters shutdown |

Joel's framing was: *"need a new engram analyzer? works in its own thread
with zero effort, responds to memory and cpu pressures, runs when it is
needed."* The example above is the literal materialization of that sentence.
The developer wrote four config attributes and a handler body. They got
concurrency, scheduling, memory/CPU pressure response, observability,
coalescing, typed failure, replay fixture, and TS exposure for free.

If a new module ever has to hand-roll any of the inherited concerns, the
substrate is missing a base capability and the fix is in the substrate, not
the module.

### Extension Bar

Put each workflow on the lowest owner shared by its callers. The owner implements the
sequence once. Give each operation whose implementation varies a narrow, typed trait
method. A required method has no body. Supply a default when it gives every implementer
a correct result. When an ordering rule protects custody or correctness, put the public
entry point on the owner and have it call the hooks. A trait default can be overridden.

Use the substrate for subscriptions, queues, backpressure, completion, retries, metrics
and shutdown. Pass handles to independent workers and release registry or queue guards
before invoking a hook. Only the work that consumes a result depends on its completion.
When two modules implement the same sequence, move it to their common owner, migrate
both callers and remove their copies. Review the resulting module against the
[substrate acceptance criteria](#acceptance-criteria-for-substrate-done).

### Runtime Frame

`CBAR_VideoFrame` becomes a broader `RuntimeFrame` / `CognitionTurnFrame`.
The frame owns stable keys and lazy artifacts for one unit of work:

- chat trigger
- canonical room snapshot
- conversation history window
- RAG source bundle
- model/capability selection
- media frame handles
- embedding handles
- prompt fragments
- KV cache leases
- LoRA leases
- response envelopes
- trace/metrics

Multiple personas handling one room event share one frame. They do not each
rebuild RAG, model selection, prompt context, embeddings, or media decoding.

### Resource Classes And Targets

The runtime already has a useful two-axis shape:

- `ResourceClass` describes what kind of work is being scheduled:
  `Cpu`, `Data`, `Gpu`, `Embedding`, `LocalGeneration`, `CloudProvider`, `Io`,
  `Media`, `Render`, `Memory`, and `Background`.
- `TargetSilicon` describes where the work wants to run: `Cpu`, `Gpu`,
  `UnifiedMemory`, `Network`, `Disk`, `Cloud`, or `Background`.

Those shipped names are the source of truth for implementation. Docs may use
"lane" informally, but code should converge on `ResourceClass` plus
`TargetSilicon` rather than inventing a second enum.

Background lanes never silently consume the visible chat generation lane.
If a lane is saturated, work is deferred with a reason, coalesced, or dropped if
stale.

### Handles, Leases, And No Bulk Copies

Pipes carry control messages and handles:

- media frame ids
- texture ids
- buffer leases
- embedding ids
- model residency leases
- KV page ids
- LoRA page ids
- room/entity handles
- artifact hashes and offsets

Large payloads stay resident in the owner pool. Copy only at the final edge
where there is no better representation.

### RTOS Rules

Continuum runtime work must follow these rules:

1. The hot path cannot block on background work.
2. Realtime work runs first; slow work runs on cadence or explicit dependency
   readiness.
3. Work declares dependencies and wakes when they are ready.
4. CPU workers stay busy with independent work.
5. GPU/model work is admitted by Rust from current pressure and residency
   evidence.
6. Low-end devices degrade by cadence, precision, context length, subscriber
   count, or modality, with visible reasons.
7. No module owns an ad hoc queue/throttle/retry/cache when the substrate can
   provide the shared version.
8. No silent fallback to CPU, random providers, placeholder models, stale room
   ids, or swallowed command errors.
9. Extension code should be short because the base substrate is doing the hard
   work.
10. **Stages emit what they deliver; anything that needs it subscribes. No
    timeout, sleep or poll is ever the mechanism that moves work forward**
    (Joel, 2026-11-14: "Timeouts are dumb ... every stage emits what it delivers
    and anything else can consume those via subscriptions to any number of
    dependencies"; "every timeout is gonna be different and flaky. All of the
    sudden you're on a faster machine"). A wait names the event it waits for
    (process exited, port bound, checkpoint written, engine ready, artifact
    built), so a faster machine simply arrives sooner and a slower one is never
    failed by a stopwatch. Dependencies come from the data a stage consumes,
    never from a hand-written order. A timeout NEVER decides deterministic flow
    (Joel: "You only use timeouts to trigger errors like a last resort like a web
    request failing and even then ideally you're tuned into errors, interrupts
    and other events too. You NEVER use timeouts [for] deterministic flow"). Its
    only place is the error path, as the last resort when a remote party may
    never answer (a hung network request); even there the first line is the
    error event itself (connection reset, process-exit signal, interrupt, the
    peer's typed failure), and the timeout only names the event that never came. This holds in production, in tests (assert on the completion
    event, never on elapsed time) and in our own tooling (a deploy is a graph:
    artifact built -> stop -> core-exited -> start -> engine-ready -> minds
    resume, with the build outside the dark window). Measured cost of breaking
    it, 2026-11-13/14 on the M5: `continuum stop` hung 100 s on four deploys
    waiting on a timer instead of the core's exit (the core's own shutdown is
    2.2 s), and two CI tests (gateway port rebind, adopted-checkpoint save) fail
    on whatever machine is slower than the one their duration was tuned on
    (cards b5b9bef3, 13590777). The `waits_that_decide_flow` ratchet counts
    sleeps, intervals, ticks, `Duration` constants and timeouts used as control
    flow, tests included (below, after rule 11).
11. **Rust is deterministic only when states and failures are types** (Joel,
    2026-11-14: "Rust is HIGHLY deterministic if you use it right"; "Even when
    you code rust it looks like js"). A review of five PRs that night
    (`~/.continuum/evidence/js-shaped-rust-2026-11-14/review.md`) found ~30
    sites in three recurring shapes; every reviewer asks these, and a ratchet
    counts them in touched files:
    - **Unknown stored as a value** (0, 0.0, `usize::MAX`, `""`, `.max(1)`
      meaning "not measured"): can this number be zero or MAX for a reason
      other than the measurement? Absence is `Option`, `Probed` or an enum.
    - **A typed failure turned into a `String`** (`Result<_, String>`,
      `map_err(|e| format!(..))`, `Err(())`, variants holding a `String`,
      `{success, error: Option<String>}` envelopes): can the caller `match` on
      why it failed?
    - **One state spread across `Option`s and `bool`s** (flags rebuilt from an
      enum, parallel `Option`s matched as a tuple with a `_ =>` fallback): does
      a combination exist the code never handles? Make it unrepresentable.
    - **The contract carries the shape** (Joel: "group params as part of
      structs when they belong together. Pass by reference and assign
      mutability and optional at contract"): parameters that travel together
      are one struct, never a long list of same-typed arguments a caller can
      swap; a function borrows (`&T`, `&mut T` only where it mutates) instead
      of taking ownership or cloning; what may be absent is `Option` in the
      signature and what must be present is a required field, so a caller can
      read mutability and optionality from the contract and the compiler
      enforces both, never a runtime check inside.
    - And **strings as identities** (verbs, field names, producer labels,
      `format!` keys beside a typed id, `serde_json::Value.get("field")` where
      a typed input exists): an identity is an enum or newtype.

#### The shape ratchets (rules 10-11)

Joel, 2026-11-14: "I can't even keep track of the thousands of these you've
made. This is 90% your code and the rest is codex." Agents wrote these shapes,
so a gate tracks them, not a person. `core/source-hygiene/src/shapes/` holds
six token rules over the shared lexer (strings and comments are never code).
Each counts every `core/` workspace member, not only the audited set, and holds
two ratchets: `<name>` (production) and `<name>_in_tests`. A total may only fall,
judged against the PR's own base tree by the PR's rules; every crate's count is
recorded beside it. A touched file keeps its old sites and may add none. Only a
touched file's sites can move, so the total rises exactly when the touched files
gain sites. Crates are not judged one at a time, because a crate cut moves sites
between crates with the total unchanged. Every CI run prints every crate of
every ratchet, folded in its log ("every crate of every ratchet").

Baseline totals (canary `958ea3056`, CI run 43547453878, production / tests):

| Ratchet | What it counts | Baseline |
|---|---|---|
| `waits_that_decide_flow` | `sleep`/`sleep_until`, `interval`, `tick_interval: Some`, `Duration` constants, a `timeout`/`timeout_at`/`recv_timeout` whose `Err` side carries on. The only exemption is `// event-exempt: <the remote that may never answer>` | 398 / 291 |
| `unknown_as_value` | `map_or(0` / `map_or(0.0`, `unwrap_or(0)` / `unwrap_or_default()` on a measurement-named read, `.max(1)` guarding a division, `usize`/`u32`/`u64::MAX` as a returned sentinel | 218 / 34 |
| `stringly_errors` (library code) | `Result<_, String>`, `Result<_, ()>`, `map_err` to `format!`/`to_string`/`to_owned`, `..Error`/`..Failure` variants holding a bare `String` | 2,923 / 238 |
| `state_as_flags` | a struct with 2+ `bool`/`AtomicBool` fields; `match (a, b, ..)` over 2+ `Option` fields of one struct declared in the same crate | 145 / 9 |
| `string_identities` | a `.get("..")` on JSON outside wire code; a `HashMap`/`BTreeMap<String, _>` struct field in library code | 839 / 184 |
| `contract_shape` | a library fn with 5+ parameters or 3+ consecutive parameters of one primitive type; `.clone()` passed straight as a call argument; a `pub fn` taking an owned `String`/`Vec` | 2,192 / 1,250 |

The rules are lexical, so they approximate. A clone or an owned buffer the
callee keeps is counted too. A timeout bound to a variable counts, because its
`Err` side is out of sight. A fallback on a read whose name says nothing is
missed. The full per-crate table is
`docs/planning/SHAPE-RATCHET-BASELINE-2026-11-14.md`. The ten crates where a
sweep pays most, by total sites: continuum-core 1,481, continuum-engine 1,444,
continuum-mind 984, continuum-kernel 835, continuum-cognition 726,
continuum-live 482, continuum-academy 337, continuum-dreaming 309,
continuum-cli-lifecycle 262, continuum-grid 211. The floor, where phase 5 starts:
`continuum-base` 15 stringly errors and 1 string identity; `continuum-host` 7 waits
(5 production, 2 test), 2 unknowns and 1 contract; `continuum-pack` is not cut yet.

### Domain Mapping

| CBAR Concept | Continuum Equivalent |
|---|---|
| `CBAR_VideoFrame` | `RuntimeFrame` / `CognitionTurnFrame` |
| lazy derived image | lazy RAG/model/media/embedding/prompt artifact |
| `textureID` | GPU/media/model/embedding/KV/LoRA handle |
| `CBAR_VideoThread` | `ResourceClass` worker lane |
| `CBP_AnalyzerThread` | recipe, RAG source, memory job, bridge, renderer |
| realtime analyzer | visible chat, media heartbeat, transport health |
| delayed analyzer | memory consolidation, semantic compression, slow learning |
| `CBP_RenderingEngine::Impl` | opaque Rust runtime state |
| Swift/Kotlin/ObjC wrappers | TS UI, command adapters, Docker process shell |

### What is already shipped

The Rust substrate is not greenfield. Several core primitives are already
shipped and should be extended rather than replaced:

- `ResourceClass` and `TargetSilicon` in
  `workers/continuum-core/src/cognition/adaptive_throughput.rs`.
- `ThroughputLease` and `ThroughputLeaseRevocationPolicy` in
  `workers/continuum-core/src/cognition/throughput_lease.rs`.
- `PressureBroker` and `PressureSource` in
  `workers/continuum-core/src/paging/broker.rs` (bootstrap landed via
  PR #1307 / #1308 / #1310; runtime lease broker via PR #1313).
- `ServiceModule`, `ModuleConfig`, `ModuleRegistry`, `MessageBus`,
  `SharedCompute`, `ModuleContext`, metrics, and structured logging under
  `workers/continuum-core/src/runtime/`.
- `ChannelQueue` and related persona queue consolidation primitives under the
  persona runtime.

The genuinely missing pieces are plan rows SB-1 to SB-8 (section 8.1).

### The Declared Dataflow Graph (gap #2, the missing half)

**Joel, 2026-09-25:** *"That's why I liked CBAR. Every component basically declared what
events it depended on (subscribed to) and emitted, then dependencies and flow of
information was in all directions and parallelized automatically."* And: *"Rust is all
about encoded predictable behavior."*

#### Why it matters now

The recurring defect in this codebase is not bad logic inside a component. It is a
wire that exists on one side only. The same day this section was written, three of them:
the decode knee was measured but the allocator's lane path never read it; the resident
checkpoint primitive was built but only one caller used it; the teacher batch was built
but the curriculum path still bypassed it. Earlier ones are in the project memory ("the
measurement already exists and nothing consumes it", "the learning flywheel was dead
because a doc comment said boot wires it"). A compiler cannot see these. A declared graph
can.

#### What exists

Consumption is half-declared. `ModuleConfig::event_subscriptions` (globs on the message
bus) and `ServiceModule::artifact_subscriptions` (`ArtifactSelector`s) name what a module
wants, and the runtime auto-wires delivery. Artifact delivery rides the message bus: a
published event name IS the artifact key. Production is not declared anywhere. Measured
on canary 2026-09-25:

| Channel | Sites | Declared? |
|---|---|---|
| Message-bus publishes (`publish`, `publish_async_only`) | ~91 | no emitter declaration |
| Bus event subscriptions (`event_subscriptions`) | 2 of 94 module configs | yes |
| Artifact subscriptions (`artifact_subscriptions`) | 39 | yes |
| `watch` channels (snapshots passed between tasks) | 69 | no |
| `mpsc` / `broadcast` channels | ~60 | no |

#### The contract

1. **Every module declares what it produces.** `ServiceModule::emissions()` returns the
   `ArtifactSelector`s (exact keys or prefixes) it publishes. It is the mirror of
   `artifact_subscriptions()` and uses the same matcher, so there is one vocabulary for
   both directions.
2. **The runtime builds the graph at registration** from every module's emissions and
   subscriptions (bus globs + artifact selectors), and reports it once at boot as a probe
   (`runtime.event_graph`): nodes, edges, and every orphan.
3. **Orphans are named, never tolerated silently:**
   - a subscription no module declares it produces (a consumer waiting for nothing);
   - an emission no module subscribes to (a measurement nobody reads), unless it is
     declared as leaving the process (a positron ViewState, the airc wire, a client).
     That external sink is itself a declaration, not an exception.
4. **Declarations are checked against behavior.** The bus records each distinct event
   name actually published and reports one `runtime.event_graph.undeclared` probe for a
   name no module declared. Without that check a declaration drifts like a doc comment.
5. **A test walks the graph.** It is built from the registered module set and fails on
   an orphan that has no named external sink. That is the build-time form of "the
   defect lives between components."
6. **The scaffold generator emits both halves.** The generator template already writes
   `event_subscriptions`; it writes `emissions()` too, so a new module is born declared.

`watch` and `mpsc` channels are typed Rust values passed at construction, not named
events. They are wired by the type system, not by name, so they are out of scope for the
name graph. The later step is to record them as edges when the runtime constructs them.

#### Time: every artifact carries two stamps, and time is passed in

**Joel, 2026-09-25:** *"Everything is timestamped and tied back historically into the
substrate ledgers per concern. Shift the result forward or backward in time from
synchronization performed by fast optical flow and later SLAM sensor fusion. We have the
same needs in our minds and systems."* And: *"Rust makes it work deterministically."*

- **Two stamps per artifact.** The first is when the source it describes was captured
  (the frame, the transcript line, the decode sample). The second is when the result was
  computed. Independent algorithms finish at different times about the same moment. A
  consumer fusing them aligns on the source stamp and uses the gap to shift a late result
  to the present. That is how a slow detector's box is re-anchored onto the current frame
  by optical flow, and later by SLAM. The same holds for a mind: a recall, a perception
  fact or a lane measurement is about a moment already past when it is read.
- **Ledgers per concern.** Stamped results land in their concern's ledger (the
  substrate's capture and replay path), so history, replay and later refinement come
  with the stamps rather than by extra plumbing.
- **Determinism is the ownership model plus one discipline.** Rust makes data races
  and unhandled states unrepresentable (ownership, `Send`/`Sync`, exhaustive `match`,
  no unwrap). Replay determinism additionally needs time and inputs passed IN, never read
  from the wall clock inside the logic. `decode_knee::knee(floor, now_ms)` is the shape.
  A recorded run then reproduces exactly, and a new algorithm stacked into the graph can
  be tried against yesterday's recorded inputs before it touches a live mind.
- **Speed is measured, not hoped for.** Declarations are static and built once at boot.
  The undeclared-name check is one set insert per NEW name, not per event. Producers with
  no live consumer can pause, so the net should be less work. Each module moved onto the
  graph shows before-and-after VDD evidence that it is at least as fast (this doc's
  consumer-migration rule).

#### Slices

1. `emissions()` on the trait (default empty), the pure graph builder and orphan
   finder, the boot probe, and unit tests on the builder. No behavior change.
2. The bus records published names and reports undeclared ones. Declare emissions on
   the modules that publish, starting with the ones behind today's three gaps.
3. The graph test in CI, with the external-sink declarations. It fails on new orphans
   and carries an explicit baseline for existing ones that shrinks over time (the
   ratchet shape the source-hygiene gates already use).
4. The generator emits `emissions()`. The first module built fully this way is the
   event-driven deploy tracker (`DeployTrackerModule`), which replaces the 5-minute
   bash poll.

### Acceptance Criteria For Substrate-Done

CBAR-like runtime work is not accepted by browser smoke alone. The substrate
is "done" when all of the following are true on canary, with PR-attached
evidence:

**Author ergonomics (what the engram-analyzer example proves):**

- New modules are small (target: a few hundred lines, including tests).
- The `#[derive(RuntimeModule)]` macro emits the required boilerplate;
  authors do not hand-roll timing spans, structured logs, metric emission,
  lease renewal, or pressure-response.
- The `just scaffold-module` generator produces a working module from one
  command line; the author edits four config attributes and a handler body.
- No new module owns an ad hoc queue, throttle, retry loop, cache, log
  format, or lifecycle when the substrate can provide the shared version.

**Derive-macro acceptance gate (per codex review on #cambriantech):**

The `#[derive(RuntimeModule)]` macro is the load-bearing piece of the "for
free" triplet. If it ships sloppy, every module that uses it inherits the
sloppiness invisibly. Therefore the derive macro must clear five specific
gates before it lands:

1. **Thin.** Generated code per `#[derive(RuntimeModule)]` is bounded —
   target is "what a careful human would write by hand, not a framework's
   worth of indirection." A reviewer should be able to read the generated
   output of a small module in one screen.
2. **Contract-preserving.** The macro emits exactly the `RuntimeModule` /
   `ServiceModule` trait the hand-written version would. No extra behavior
   smuggled in. No silent type coercions. If the hand-written version
   would not compile, the macro-generated version does not compile either
   — the contract is the same.
3. **Inspectable.** `cargo expand --package <crate> --module <m>` must
   produce readable output. A reviewer can audit any module's actual
   runtime behavior in 30 seconds. The macro emits hygenic code, not
   identifier soup.
4. **Tested.** The macro itself has tests (golden-file or trybuild) that
   prove every supported attribute permutation expands to known-good
   code. Tests include the failure modes — e.g. a module declaring two
   `lane`s, or an `ArtifactSelector` that doesn't exist, must fail to
   compile with a useful error.
5. **No hidden behavior.** The macro must NOT hide resource leases,
   scheduling decisions, or fallback behavior. If a module gets a lease
   from `PressureBroker`, it is visible in the macro output. If a module
   has a cadence policy, it is visible. If a module degrades under
   pressure, the degradation path is visible. The macro saves typing,
   not auditability.

The shape of these gates is: anything the macro generates, a reviewer can
see and reason about; nothing the macro generates is doing "magic" that
makes the module's behavior unpredictable.

**Runtime behavior (what the substrate must actually do):**

- Realtime work runs first; delayed work runs on cadence or explicit
  dependency readiness.
- Work declares dependencies (`ArtifactSelector`) and the runtime wakes only
  the useful work.
- N personas handling one room event share one `CognitionTurnFrame`; they do
  not each rebuild RAG, model selection, prompt context, embeddings, or
  media decoding.
- `PressureBroker` admits / defers / drops requests with a typed reason; no
  silent fallback to CPU, random providers, placeholder models, stale room
  ids, or swallowed command errors.
- Background lanes never silently consume the visible chat-generation lane.
- Low-end devices degrade by cadence, precision, context length, subscriber
  count, or modality, with visible reasons.

**Required tests, per module and per substrate change:**

- Unit TDD: dependency wakeups, lane admission, cadence, coalescing,
  `Deferred` / `Failed` return paths.
- Resource VDD: bounded queues, memory leases, no monotonic growth across
  hundreds of frames.
- Performance VDD: first response, all responses, tok/s, queue time, all
  emitted as Standard VDD Record fields.
- Residency VDD: Metal / CUDA / Vulkan local GPU path proven when required.
- Qwen VDD: Qwen 3.5 text/code and Qwen2-VL vision use the expected local
  GPU backend, report layer residency, and fail loud on unsupported layers
  instead of silently running CPU-shaped inference.
- Accuracy VDD: replayed persona / RAG / tool output is reproducible from
  trace records.
- No-CPU-fallback contract: enforced across the whole workers tree, not the
  three currently-whitelisted paths in `no_cpu_fallback_contract.rs`.

The alpha gate is not "it boots." The gate is that the runtime behaves like
an engine: predictable, concurrent, observable, fast, and small to extend.


## 3. Measurement and validators

### 3.0 Simulation first: every design section has its simulator, every fault becomes a scenario

Joel, 2026-11-13: "Simulation needs to be part of each document and our iteration. We can run
realistic load testing using what we learn from our own systems faults or architecture in general."
A live node under real residents is a road test in traffic; the simulator is the dyno. A change is
iterated on the simulator until its numbers move, then confirmed live, and what the live node teaches
goes back into the simulator as a new scenario. No section of this design is complete without the
simulator that exercises it.

**The loop** (Joel, 2026-11-13: "iteration with prod and replicating that in simulation, fixing code to
maximize speed and limit resources, trying and measuring prod again, replicating in simulation"):
1. Measure production: act time, stage times, memory, quality, every mind's progress.
2. Replicate it in simulation: the recorded load and the fault reproduce there, with the same numbers.
3. Fix the code to maximize speed and limit resources, iterating on the simulator until the numbers move.
4. Try it in production and measure again with the same instruments.
5. Replicate what production showed back in simulation, including anything it missed, and repeat.

**The simulator carries every level of production's monitoring.** The same probe classes, the same
health line, the same prompt captures and the same per-stage timings run in simulation as in
production, so a simulated number and a live number are read by one instrument and compared
directly. A metric that exists only in production cannot be tuned; a metric that exists only in
simulation cannot be confirmed.

**The simulators (one per layer; real policy code, recorded inputs, seconds, any machine, no residents):**
- **Admission and capacity: the dyno** (`core/continuum-engine/src/capacity/dyno`). The real
  allocator, grants and conservation (weights + resident KV + staging <= device) driven by a recorded
  day (`fixtures/dyno/<date>.json`, every value beside its receipt). Each recorded failure is a
  failing assertion of the design's invariant; the fixing slice turns it green.
- **Serving: `serving-replay`** (`core/serving-replay`, section 4.5, plan row SV-S0). Captured act
  sequences against a small hybrid model: reuse, fresh prefill, page-ins by kind, evictions with
  page-out and restore milliseconds, relaunches.
- **A mind's context: the context replay** (`core/continuum-context/examples/replay`). Captured
  prompts replayed through the codec: the token prefix shared by consecutive acts, fresh tokens per
  act, levels chosen, collapses.
- **Data path: the vector search bench** (to build with plan step 2 of section 2.6): a recorded
  corpus at 10k/100k/1M vectors, search latency, recall@k against the exact answer, CPU time and
  bytes moved per search.

**A run is a fair comparison or it is not evidence** (shared with the ORM design's section 8): baseline
and candidate run the identical recorded load with identical seeds; latency is measured from arrival
and includes every queue wait; quality and custody are gated, not only speed; and every mind's
progress is reported, because a gain for one mind that starves its neighbours is a regression.

**Load is realistic because it is ours.** Scenarios are recorded from the live nodes: the minds,
their act shapes and prompt sizes, the node's memory and its other residents. A synthetic load is
allowed only as a sweep around a recorded one (more minds, longer contexts, a smaller machine).

**The scenario library (each a recorded fault; the simulator must reproduce it before the fix and
pass after):**

| Scenario | Recorded | Layer | What the simulator must show |
|---|---|---|---|
| 2026-11-04 the M5 day | dyno fixture `2026-11-04.json` | admission | each failure of the day as a failing invariant, green after its slice |
| A share bounds an admitted act | M5 2026-11-11 16:06-16:21Z, 9 of 15 acts refused (#5947) | admission | 0 refusals of an act within its grant |
| A reply sized from the estimate overshoots the grant | M5 2026-11-11 17:47-18:02Z, 7 refusals (#5950) | admission | the reply sized from the measured prompt; 0 refusals when the prompt fits |
| A fixed host cache starves the engine | M5 2026-11-13: 60 MB free, 34 GB compressed, prefill 92 tok/s against ~190 | serving | prefill and decode tok/s held as memory pressure rises; the cache gives memory back (fork #110) |
| Standing starvation reads as clean | M5 2026-11-13 23:40Z: 0.4 GB free, compressor flat at 42 GB, swap 14.5 of 15 GB, kernel critical, deficit 0, cache lever never asked (dyno 11, `serving_daemon/tests/dyno.rs`) | memory + serving | deficit > 0 while the kernel says pressure; the cache (Batch) asked first and lowered; taken back to its grant after two OS-normal polls |
| A reopen too large for the view loops | Eitan, IntelMac, 2026-11-11 to 11-13 (#5966) | context | consecutive reopens of one output advance through its pages |
| A per-element store codec and per-pair norms | 2026-11-13 review of #5954, `similarity::cosine` | data path | bytes moved and CPU time per search fall to the copy and the GPU dispatch |
| A Windows core's private commit runs away | BigMama 2026-11-13, 10.8 to 32.7 GB in 8 min | serving | resident memory flat over a recorded day |
| A restart re-prefills every mind | M5 2026-11-13 23:45Z deploy: ledger restored 0, every first act `cold_turn cause="no_page"`; 29 paged-out minds' pages taken by the page store at volume 0.85 (store 3% of its budget), 22:40Z-00:50Z | serving | dyno fixture `2026-11-13-restart.json`: four recorded residents restore in ~5.6 s (0 tokens re-prefilled) against ~401 s and 253,321 tokens cold; saves and restores issued at once, so on an engine that overlaps page tasks the stop costs its largest save (~4.9 s, not the ~13.5 s sum) and each mind waits only on her own restore (largest ~2.0 s); the pinned fork runs one page task at a time (server-queue.cpp:138-153); a paged-out mind keeps her page under the recorded pressure |

Every row of the plan (section 8) names the scenario that proves it, alongside its gate and
validator. A fault found live and not yet a scenario is the next scenario to record.


Frame time is act time (section 2.5). Every workstream proves itself first on a validator that runs
in seconds with no live residents, then on the live node. This table indexes what each layer measures
and where its validator is specified; the specifications themselves are in the sections named.

| Layer | What it measures | Validator (where specified) |
|---|---|---|
| Every change | act latency (median, p90), the targeted stage (prefill tok/s, decode tok/s, search latency, store op p50/p99), memory (free, compressed, swap, phys_footprint), output quality (recall@k, review outcome) | replay validator plus a live node (section 2.5) |
| Substrate | the Standard VDD Record per module and job | trace data from the substrate; replay fixture per scaffolded module (section 3, the "for free" triplet; 7.1 below) |
| Substrate dataflow | orphan subscriptions and emissions; undeclared published names | the graph test and the `runtime.event_graph` / `runtime.event_graph.undeclared` probes (section 3, "The Declared Dataflow Graph") |
| Serving | reuse, fresh tokens, page-ins, relaunches = 0 | `serving-replay` (S0), captured act sequences against a small hybrid GGUF (section 4.5; plan row SV-S0) |
| Serving, memory owner | where every mind resumes across turns, stop, boot, relaunch, adoption, idle retirement, gene change, transient borrowing | `MindMemory` lifecycle validator (section 4.1a) |
| Serving, reconfigure | token-identical generations across a reconfigure; flat footprint over 100 grow/shrink cycles; weights never reloaded | S2 validator (section 4.5, "S2 in detail") |
| Serving, speed levers | per lever: replay reuse, page-in ms, aggregate tok/s, RAM vs disk restore ms, MTP acceptance, submit-to-first-token, first-act prefill tokens, footprint and quality A/B | section 4.4 |
| Serving, admission | capacity refusals 0; every mind with a turn asked takes an act | section 4.8.5 |
| Context | re-processed tokens per act against appended tokens (healthy about 1:1) | section 5, "The governing law", rule 5 |
| Context, codec | prefix shared between consecutive requests; turnovers; reachability of every pointer; invariant-4 repeats | `continuum-context` `examples/replay.rs` and `examples/replay/validate.rs`; the faculty replay (`fable/faculty-replay`); source-hygiene ratchets (section 5, gates tables) |
| Context, live | repeats per resident-hour; the 15-minute live read | section 5, causality revision |
| Data path | 10k/100k search latency and recall@k | plan row DP-1 (section 2.6) |

### 3.1 Timing, logging and VDD for free

Timing and logging are substrate behavior, not instrumentation added after a
bug. Every runtime concern should inherit the same observability contract that
CBAR gave threads through names, FPS timing, queue ownership, and lifecycle.

Every module/job must automatically emit:

- module name, job id, turn/frame key, resource class, target silicon, and
  dependency keys
- queued-at, admitted-at, started-at, first-output-at, completed-at, and
  dropped/deferred-at timestamps
- queue depth, queue wait, execution time, first-output latency, and total
  latency
- coalesced count, stale-drop count, retry count, deferred reason, and silence
  reason
- CPU/RSS deltas where available
- GPU backend, GPU layer count, residency estimate, VRAM/unified-memory deltas,
  and unsupported layers for inference work
- structured success/error state suitable for command callers and replay tests

TDD proves the contract. VDD proves the behavior. The runtime should make both
cheap: each module gets trace spans, logs, counters, timing samples, and replay
hooks by implementing the common trait. A PR that adds a new runtime concern
without this evidence path is adding an unobservable subsystem, even if the
feature appears to work.

#### Standard VDD Record

All agents and platforms should report the same record shape. Do not invent a
new timing table per machine.

```text
scenario:
platform:
hardware:
backend:
git_sha:
command:
model:
gpu_layers:
unsupported_layers:
cold_start_ms:
first_token_ms:
first_response_ms:
all_responses_ms:
responses_expected:
responses_observed:
silence_reasons:
tok_per_sec:
cpu_pct_avg:
cpu_pct_peak:
rss_mb:
gpu_util_pct_avg:
gpu_memory_mb:
queue_wait_ms:
execution_ms:
coalesced_count:
deferred_count:
stale_drop_count:
error_count:
degraded_reason:
log_refs:
next_bottleneck:
```

The runtime should be able to emit this as JSONL from the same trace data used
by tests. Humans can paste the text form into PR comments, but the canonical
machine-readable output should come from the Rust substrate.

#### One-Line Instrumentation API

The substrate should expose tiny helpers so module authors do not hand-roll
timers. The target ergonomics should feel like C/C++ one-line macros while
still producing structured Rust data:

```rust
let _span = vdd_scope!(ctx, "persona.generate", ResourceClass::LocalGeneration);
vdd_mark!(ctx, "first_token");
vdd_counter!(ctx, "tokens", generated_tokens);
vdd_residency!(ctx, backend = "metal", gpu_layers = n_gpu_layers, vram_mb = vram_mb);
vdd_defer!(ctx, "gpu_pressure", retry_after_ms = 250);
vdd_fail!(ctx, "unsupported_qwen_layer", layer = layer_name);
```

Those calls should feed the same `Standard VDD Record` fields automatically.
The common helpers must be available to persona, inference, memory, media,
render, ORM/data, grid, and Docker-adapter code. Iterative optimization should
be a tight loop:

1. run one standard command
2. compare CPU, GPU, memory, power, queue time, first token, tok/s, and
   response count against the prior run
3. make the bottleneck visible
4. repeat until CPU drops, GPU residency rises, memory/power stay bounded, and
   throughput increases

If a performance PR requires custom scripts to discover basic timings, the
substrate is not doing its job.

### 3.2 Tests are contracts

- A required capability is resolved and adopted, or the launch refuses with its cause.
- Appending an event never changes the token prefix of the current epoch.
- A new epoch records its invalidation exactly once.
- Every broken reuse promise reports a defect, cause, replay cost and recovery outcome;
  successful re-prefill never clears the defect. Declared cold transitions remain distinct.
- Cancellation preserves the accepted frontier; incompatible minds/adapters cannot reuse state.
- Resolve, serialize, launch and observe agree on the same configuration identity.
- Late artifacts retain source identity; effects reject changed required dependencies.

Machine numbers belong in receipts on the cards, never in tests.

