# Continuum Engine: a graphics pipeline for many minds

**Status:** the one design for Continuum's engine (the runtime substrate, the serving engine, a
mind's context, the engine-facing data path) and the plan that gets it built. It replaces three
documents that overlapped: `CBAR-SUBSTRATE-ARCHITECTURE.md` (sections 2, 3, 7), `SERVING-ENGINE.md`
(section 4) and `MIND-CONTEXT-AND-ENGINE-CONFIG.md` (section 5). Their text is carried here whole,
once; their order-of-work and gap sections are merged into section 8. ORM and database schema design
is not part of this document; it is `docs/infrastructure/RUST-ORM-ARCHITECTURE.md` (section 6 names the boundary).

**Scope:** fast LLM serving and cache work: multi-persona streaming, KV paging and fast context
switches, prefix reuse, the context codec, admission, and embeddings as GPU work.

## 1. Thesis

Many persistent minds share one live model engine the way a game engine's renderer shares one GPU:
the engine stays up, each mind's state pages between GPU, RAM and disk by predicted need, shared
prefixes are held once, and an act prefills only what is new. Every byte stays native, every reuse is
taken, nothing waits that need not.

### Measured (receipts already recorded in this design; each is repeated in full where it is used)

- **M5, Qwen3.8-27B Q4_K_M, 2026-10-19, d90006023, 13:48-14:48Z:** prefill 107 tok/s and decode 13.2
  tok/s per request with 2 concurrent (about par with published numbers for the chip); prefix reuse
  per act 35%; 13-30k fresh tokens per act; queue wait 0-20 min; act end to end 5-20 min. The ring sat
  mid-prompt and re-prefilled 16,669-28,803 tokens per act (section 5, "Why").
- **M5, 2026-10-19:** a restored KV page reused 0 of 24,670 / 29,999 (`inference.restored_turn`) while
  the restore itself reused 661 of 662 (`serving/cache-probe --roundtrip`); enabling the GPU matrix
  units roughly doubled prefill (section 5, "Why").
- **M5, 2026-10-21, canary 69b63f7df:** 4-8% of each request matched the previous one; about 160 s per
  act; 8 acts and 1 write an hour. The epoch view replayed over the same captures cut mean prefill per
  act 2-3x (Aiko 70,296 to 27,815 chars, Aris 51,505 to 15,998, 2f50b223 65,961 to 31,730) (section 5,
  "Gates, in order").
- **27B hybrid, e7e1077b3:** 27 checkpoints took 11.4 GB of sidecar beside a 2.2 GB state; the save
  took 66-100 s (section 4.0).
- **M5, engine-c 741b0175b, Qwen3.6-35B-A3B Q4_K_M, 2026-10-28:** decode per stream / aggregate 59/59
  at 1 stream, 31/124 at 4 (llama-batched-bench), 31/122 through our server: the server batches like
  stock llama.cpp; MTP wins alone (80 t/s) and loses under concurrency (23/91 at 4) (section 4.1b).
- **M5, 2026-10-27, 16k `tg_3s` samples:** live decode median 20.6 t/s alone, 8.3 with 3 others
  decoding, 4.6-5.6 during any page save or restore (section 4.1b).
- **M5 page IO, 2026-10-28:** isolated 0.7 GB saves in 0.22 s and restores in 0.1 s; live pages carry
  1.88 GB (1.24 GB checkpoint sidecar) and take 3.05 s median to save (section 4.1b).
- **M5, build 7ace74020, 176 acts, 2026-10-28:** minds waited a median 25.7 s plus 24.6 s on two permit
  layers in a 155 s median act while the engine was occupied 99.6% of the time (section 4.1c).
- **M5, 2026-11-03 12:51Z, fork `resident_state`:** target weights 20.69 GB, target KV plus recurrent
  2.14 GB (8,317 B/token over 257,536 tokens), compute graph 1.87 GB, draft context 3.15 GB, projector
  0.90 GB, one checkpoint 94.6 MB (section 4.7.1).
- **M5, 2026-11-03, 1,244 checkpoint resumes and 611 consecutive capture pairs:** 80% resumed within
  8k tokens of the end (median 2,431 re-read); 435 of 611 pairs first differed only in the changing
  tail (section 4.7.2).
- **M5 hybrid model, before 2026-11-06:** re-rendering the act's tail every act re-prefilled 2,822 s
  over 78 acts in 3 h (section 5, "one codec for every layer").
- **M5, 2026-10-26:** an avatar at ~200 image tokens was described correctly with prefill 2.5 s and
  decode 2.8 s (section 5, "Media is the same codec").
- **BigMama, 2026-11-11 13:42Z:** 0 acts in an hour; Sahar 183 turns asked and 0 served, Kimi 55 and 0
  (`NeedAboveCeiling 116240/58112` while her view fit 29,056) (section 4.8).
- **M5, 2026-11-13:** a fixed `--cache-ram` left 60 MB free and prefill at 92 tok/s against ~190
  (section 2.3).

## 2. Non-negotiable: every optimization, exhausted

Joel, 2026-11-13: "use the graphics pipeline approach and study the unreal engine optimizations";
"Carmack level efficiencies, avoiding squares even in the GPU"; "never parse"; "avoid rasterization
and cpu/gpu transfer whenever possible"; "use raw blobs and bytes unless you truly need the
encoding"; "do not waste cpu ever for anything it is unnecessary for". This is the review standard
for every data, storage, retrieval and serving change. A change that cannot say which row it
follows, and show its end-to-end timing and output quality before and after, is not ready.

### 2.1 What the CPU does per byte

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

### 2.2 Carmack rules for the math

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

### 2.3 Unreal Engine systems and their counterparts here

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

### 2.4 The exhaustive checklist (take every one that applies; a skipped one is named with its reason)

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

### 2.5 How a change proves itself

Frame time is act time. Every change here posts, on a live node and on a replay validator:
act latency (median and p90), the stage it targets (prefill tok/s, decode tok/s, search latency,
store op p50/p99), memory (free, compressed, swap, phys_footprint), and output quality (recall@k,
review outcome). A health line's label is not a measurement.

### 2.6 Order of work (2026-11-13)

1. The streaming pool: fork #110 (live `/prompt-cache/limit`) + the host deficit's shed path, so the
   engine's cache gives memory back first under pressure and takes it back when the OS reads normal
   (card 0f631c46; the deficit is judged by the OS's own pressure verdict, never by our counters).
   Measured on the M5: prefill tok/s, decode tok/s, free memory, act latency.
2. GPU-driven vector culling: unit vectors at write as raw f16/f32 bytes, a no-copy GPU buffer, one
   GEMV dispatch with fused top-k and a popcount prefilter. Measured: 10k/100k search latency and
   recall@k.
3. The act as a dependency graph with transient arenas and prefill overlapped with tools.

## 3. The substrate

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

The acceptance test for the runtime pattern is unified in §"Acceptance
Criteria for Substrate-Done" below. The shorter version, restated for the
person about to write a new module:

- New modules are small (a few hundred lines at most). If a persona recipe,
  model adapter, RAG source, media observer, render observer, memory
  consolidator, or grid bridge needs to implement its own transport,
  backpressure, retry loop, logging, queue, metrics, throttle, or lifecycle,
  the substrate is missing a base capability — file the substrate gap, do
  not work around it in the module.
- The correct high-performance path is the *shortest* path. Anti-pattern: a
  PR that grows a module to compensate for missing substrate behavior. The
  reviewer's job in that case is to ask which substrate gap is being papered
  over, then route the work there.

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

## 4. The serving engine

Design pass 2026-10-24 at canary c47aa6b24, fork e7e1077b3 (read-only, file:line evidence); plan line 3b.
Joel, 2026-10-24: "take it on full steam. Make it really optimized and advanced, well organized";
"really optimize and validate by breaking up properly". Every slice lands with its validator (section 8.2).

### 4.0 What the code shows

- **Qwen3.8-27B is a hybrid model.** Its GGUF arch is `qwen35`, which `llm_arch_is_hybrid` lists (llama-arch.cpp:1056). It has 64 blocks, and every 4th one is attention, so 16 attend (gguf_keys.rs:528-538). The KV cache costs 65,536 B/token at f16, about 34.8 KB at q8_0.
  - A partial `seq_rm` cannot roll back the recurrent state. The server therefore restores from checkpoints (server-context.cpp:3812-3912) and creates them at user-turn starts and at n-4 and n-4-ubatch (4003-4016, 4111).
  - Shifting `cache_reuse` is switched off for such contexts (1210-1212).
- **Checkpoints are the hidden memory cost.** e7e1077b3 measured 27 checkpoints taking 11.4 GB of sidecar next to a 2.2 GB state, and the save took 66-100 s. The checkpoints sit in host RAM, which on this machine is the same 64 GB the GPU uses.
- **The KV fits.** Four minds at 59k tokens is 4 × 59k × 34.8 KB, about 8.2 GB. With one unified pool, 4 resident minds fit next to the weights. Today's 2 slots come from the launch geometry and the checkpoint cost, not from a shortage of KV.
- **The in-process scheduler is a weaker copy of the fork.**
  - Every prefill starts at 0 and the KV is freed at the end of each sequence (llamacpp_scheduler.rs:30-32, 691).
  - Prefill fills `n_batch` in seq-id order before the decode tokens of higher seq ids (361-420). That is the starvation inproc_bench measured.
  - It has no MTP, no checkpoints, no jinja templates, no tool parsing, and none of the learning duties the fork's server has gained in its last 40 commits.
- **The fork's server is already a library.** `server-context` is a STATIC target "used by llama-server and CLI" (tools/server/CMakeLists.txt). The CLI drives it in-process through `server_response_reader` (server-context.h:107-115). Today core/llama builds with `LLAMA_BUILD_SERVER=OFF` (build.rs:46) but already links `llama-common` (316).
- **Weights reload today whenever the context changes.** `destroy()` frees the model along with the context (server-context.cpp:962-976), and slots are fixed from `n_parallel` at load time (1252). The core relaunches to grow the window (reconcile.rs:3-13).

### 4.1 Crate and modules

`core/continuum-serving` is the Serving module. Its mechanism lives in the fork as a C ABI shim, `tools/server/server-continuum.{h,cpp}`, which can reach `server_context_impl`. The safe Rust binding goes in `core/llama/src/server.rs`.

| Module | Single owner of | Seam trait |
|---|---|---|
| `lifecycle` | Model loaded once with mmap; context generations | `EngineLifecycle { reconfigure(Geometry) -> PauseReceipt; geometry() }` |
| `seq_table` | Mind (`PageKey`, page_key.rs) → seq id, residency Gpu/Ram/Disk, token frontier, gene set, epoch | `SequenceTable { locate(PageKey) -> Residency; bind/unbind }` |
| `scheduler` | Policy only: temperature as priority, aging, and which minds stay resident. The mechanism stays the fork's `update_slots`: continuous batching plus the time-fair interleave (3263, 3613) | `Admit { admit(ActRequest, Temperature) -> Ticket }` |
| `pager` | `state_seq_get/set_data_ext` to RAM; spill to disk in today's format | `Pager { page_out(seq, Tier); page_in(PageKey) -> PageIn }` |
| `prefix` | Shared heads (see below) | `PrefixShare { share(HeadId, SeqId) }` |
| `checkpoints` | Where, how many and which go to disk (oldest plus newest N, e7e1077b3), within a measured byte budget | `CheckpointPolicy` |
| `genes` | Adapter registry; homogeneity groups (a slot's KV is cleared when its adapters change, 1697-1700) | `GeneSet` |
| `sampling` | Typed parameters into `common_sampler`, grammar and stop; no logic of its own | — |
| `speculative` | MTP configuration and acceptance probes; the logic is `common_speculative` | — |
| `template` | One fidelity owner: `common_chat_templates_apply` / `common_chat_parse` (common/chat.cpp, common/jinja) | `Render` |
| `http` | `server_routes` from `llama-server-impl` in the same process, so external drivers like Hermes share one engine and one budget | — |
| `port` | What minds call: binary frames, no HTTP or JSON | `ServingPort { act(PageKey, ActRequest) -> TokenStream; warm/cool(PageKey, Temperature); capacity() -> CapacityReport }` |
| `probes` | Metrics: reuse, page-in ms, wait, footprint | — |

**Prefix sharing on a hybrid model.**
- With a unified KV, `seq_cp` only rewrites cell metadata (llama-kv-cache.cpp:462-466).
- The recurrent `seq_cp` shares only the source's tail cell, which is the state at the source's end (llama-memory-recurrent.cpp:251-271).
- So a shared head is either a dedicated "head seq" ending exactly at the boundary, or `seq_cp` of the attention cells `[0,p)` plus the head's checkpoint loaded with `PARTIAL_ONLY`.
- Prefer the checkpoint route. A shared tail cell combined with MTP rollback hits `GGML_ABORT("cannot write shared recurrent state with different rollback indices")` (774).

**Page format.** Disk pages stay `llama_state_seq_save_file` output plus the `.ckpt` (0x434B5054) and `.lora` sidecars (2716-2826). `a-<persona>.bin` and `LEDGER_FORMAT 2` (saved_ledger.rs:36-62) then carry over unchanged. One gap: the `.bin` holds only `ctx_tgt` (2809), so the MTP draft KV comes back cold.

### 4.1a `MindMemory`: the `seq_table` and `pager` owner over today's llama-server (2026-10-27)

**Why now.** On 2026-10-27 a mind's KV memory broke four ways in one afternoon on the M5:
- the tool head changed at launch;
- a save was cut by the core's exit;
- a resident was left unsaved in her slot at the stop;
- the boot plan was computed before the ledger was credited.

Each was patched in one lifecycle path (#5538, #5540). The inventory that followed, at canary 107af9167, found 41 paths and 9 decisions made in 2 to 6 places each:
- who may evict a resident;
- when a page is restorable;
- the save-the-evictee procedure, written three times;
- save-all before the engine dies, four times, and missing in idle retirement;
- recording residents lost unsaved;
- quarantine;
- slot ownership;
- scratch policy;
- page-dir ownership.

Joel: "you can't get anything to work unless there is one daemon and centralized location." So `seq_table` and `pager` (4.1) are built FIRST, as one owner speaking HTTP to today's llama-server (step 1 of 4.2). When S1–S5 land, its mechanism changes and its callers do not.

**One owner per engine endpoint: `MindMemory`.** It holds the slot attribution, the saved set, the ledger file, page IO and the quarantine. Nothing else calls `/slots/*` or writes the ledger. It runs as a `ServiceModule` in the canonical shape (CONCURRENCY-STYLE-GUIDE). Its stop is its own `SettleBeforeStop`.

| Request | Replaces | Contract |
|---|---|---|
| `seat(PageKey, Purpose) -> Seat` | `lease` + `pin_slot` + `plan_paging` + the turn's paging closure (turn_admission.rs:344-472); `borrow_slot_for_transient` (:517); `warm_ahead`'s paging; `cache_probe`'s raw `/slots` IO | RAII. She holds a pinned slot with her KV in it (resident, or restored from her page), or the seat says why she starts cold. `Purpose` is Turn, WarmAhead or Transient: a Transient seat may borrow the cheapest resident's slot, which is saved first. |
| `pause(Why) -> PauseReceipt` | `checkpoint_before_replacement`, `checkpoint_owned_restore`, `checkpoint_adopted_before_replacement`, `checkpoint_every_endpoint` (stop), and the missing save in `idle_if_current` | Closes seating and drains the seats, then saves every resident with a confirmed save. A resident whose save fails is recorded as lost, by name. Every engine retirement goes through it: relaunch, academy, idle, adopted retirement, stop. |
| `resume(PauseReceipt, Generation)` / `adopt(Generation)` | `ready()`'s pool swap and ledger carry or load; `adopt_engine_ledger`; `resume_suspension` | Boot, relaunch and adopt load the one ledger by the one rule (fingerprint plus incarnation), and seat nothing. |
| `forget(PageKey, Why)` | `note_page_lost` on a refused restore; nothing today on an in-place gene change | A page whose KV no longer means what it did (a gene change, a refused restore) stops being restorable. |

**Inside it, each decision exactly once:**
- `evict(slot)`: the single save-the-evictee procedure (invalidate, save, confirm).
- `restorable(key)`: the single rule for when a page is offered.
- the ledger written only on its own transitions;
- quarantine owned and cleared by it;
- the scratch policy decided once, in `seat`.

**Deleted when callers move:**
- the paging block of `turn_admission.rs` and `borrow_slot_for_transient`;
- `slots/page_io.rs` (#5540's gate, which becomes `MindMemory`'s own stop);
- the four `checkpoint_*` variants;
- public `take_resident` / `forget_resident` / `plan_paging` / `note_saved` / `note_page_lost`;
- `trim_page_store_after_save`, which has no caller; the page store's trim moves into the owner or is deleted.

**The API (published for the callers' migration; Codex's review of 2bd37de22 is built into the types).**

```rust
/// Which engine process a receipt or seat belongs to: its generation, and for an adopted
/// engine the OS incarnation (pid, start time) verified independently of any launch handle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EngineIdentity { pub generation: Uuid, pub incarnation: Option<(u32, u64)> }

pub enum Purpose { Turn, WarmAhead, Transient }

/// Where a seated mind's KV came from. Every cold seat names why.
pub enum Resumed { Resident, Restored, Cold(ColdCause) }
pub enum ColdCause { FirstVisit, PageRefused, DiscardedUnsaved(LossCause), Quarantined }

/// RAII. The slot is pinned while the seat lives. `completed` commits the mind's attribution
/// (and her tail size, the eviction price); a seat dropped without it detaches the slot.
/// A Transient seat never holds a mind: it borrows the cheapest resident's slot, which is
/// saved (confirmed) before the seat is granted, and is not a driver session (drivers seat
/// as Turn with `PageKey::driver`).
pub struct Seat { /* slot, key, resumed, engine */ }
impl Seat {
    pub fn slot(&self) -> u32;
    pub fn resumed(&self) -> &Resumed;
    pub fn engine(&self) -> EngineIdentity;
    pub fn completed(self, tail_tokens: u64);
}

pub enum PauseWhy { Relaunch, Academy, Idle, AdoptedRetirement, Stop }
pub enum LossCause { SaveFailed(String), DeadlineSpent, EngineExited, PagingUncertain }

/// Not Clone, not serializable: one pause, one consumer. Bound to the engine it paused.
pub struct PauseReceipt { /* engine, why, ... */ }
impl PauseReceipt {
    pub fn engine(&self) -> EngineIdentity;
    pub fn saved(&self) -> &[PageKey];                    // confirmed by the engine's ack
    pub fn lost(&self) -> &[(PageKey, LossCause)];        // named, and recorded for the successor
    pub fn unresolved(&self) -> &[PageKey];               // IO whose outcome is unknown: quarantined
}

pub enum Attach { Launched, Adopted { incarnation: (u32, u64) } }

impl MindMemory {
    /// The only way onto a slot. Refused while paused, or once the deadline passes.
    pub async fn seat(&self, key: Option<PageKey>, purpose: Purpose, deadline: Instant) -> Result<Seat, SeatRefused>;

    /// Close seating, wait for every open seat to close (a seat closes BEFORE its slot is
    /// detached or saved), then save every resident with a confirmed save. Returns only when
    /// no page IO it started is in flight: IO still running at the deadline is listed
    /// `unresolved` and the endpoint is quarantined, so a retirement can never race it.
    pub async fn pause(&self, why: PauseWhy, deadline: Instant) -> Result<PauseReceipt, PauseRefused>;

    /// Reopen the SAME engine (an academy batch done). Refused for a receipt of another
    /// engine, for a quarantined endpoint, and for a Stop pause (an operator stop stays stopped).
    pub fn resume(&self, receipt: PauseReceipt) -> Result<(), ResumeRefused>;

    /// The engine may now be killed. Consumes the receipt; its losses go to the ledger the
    /// successor reads. Refused for a receipt whose engine is not the current one.
    pub fn retire(&self, receipt: PauseReceipt) -> Result<Retired, RetireRefused>;

    /// Boot, relaunch or adopt: bind a new engine and load the one ledger by the one rule
    /// (page-format fingerprint; residents in place only for an Adopted engine whose
    /// incarnation matches the ledger's). Seats nothing.
    pub async fn attach(&self, engine: EngineIdentity, how: Attach) -> Result<Attached, AttachRefused>;

    /// A page whose KV no longer means what it did is never offered again.
    pub fn forget(&self, key: PageKey, why: ForgetWhy);

    /// For the one scheduler (G2, Codex): per mind, residency, frontier and last activity,
    /// residents and drivers alike, as a watched snapshot.
    pub fn snapshot(&self) -> tokio::sync::watch::Receiver<MemorySnapshot>;
}
```

**Validator (S0 for this owner).** `turn_admission/tests/lifecycle.rs` already runs a fake engine whose slots hold `mind:tokens`, through turns, a stop and a boot. It moves to `MindMemory`'s own tests and gains:
- a relaunch;
- an adoption;
- idle retirement;
- an in-place gene change;
- transient borrowing.

The validator drives the actual retirement callers (relaunch, academy, idle, adopted retirement, stop) through `pause` and then `retire` or `resume`, never the checkpoint helper alone, including a second stop after an adoption (Codex).

Each scenario asserts where every mind resumes, in seconds, before any live read.

**Order and owners:** plan rows SV-MM1 to SV-MM3 (section 8.2).

Each step deletes what it replaces in the same PR, and the lifecycle validator gains that step's scenario first.

### 4.1b What concurrent minds cost on the M5: measured 2026-10-28

These findings come from engine-c 741b0175b with Qwen3.6-35B-A3B Q4_K_M, on 64 GB of unified memory. Every number is a receipt, and each one answers "is it llama.cpp, the server, or continuum?"

**Decode across layers** (short prompts, 256-token decodes; t/s per stream / aggregate):

| Streams | llama-batched-bench (no server) | Our server, no draft | Our server, MTP draft n_max 4 |
|---|---|---|---|
| 1 | 59 / 59 | 57 / 57 | 80 / 80 |
| 2 | 45 / 90 | 44 / 89 | 42 / 84 |
| 4 | 31 / 124 | 31 / 122 | 23 / 91 |

- Nothing serializes: the server batches like stock llama.cpp.
- Batching amortizes weight reads, so the aggregate climbs. On this MoE model it reaches only about 2.1× at four streams, because each sequence routes to its own experts.
- The MTP draft spends the same spare capacity that batching does. It wins alone and loses under concurrency.
- Fix: `inference::speculation` drafts only while a turn decodes alone, set per request.

**Live decode by concurrent engine work** (16k `tg_3s` samples, 2026-10-27):

| What else is running | Median t/s |
|---|---|
| Nothing | 20.6 |
| 1 other decoding | 11.2 |
| 2 others decoding | 8.7 |
| 3 others decoding | 8.3 |
| Another slot prefilling | 14.7 |
| Any page save or restore | 4.6–5.6 |

A page save or restore stalls every stream, because SLOT_SAVE and SLOT_RESTORE run on the server's main loop.

**Page IO, isolated against live:**

| | Bytes | Save | Restore |
|---|---|---|---|
| Isolated (99k tokens, SSD) | 0.7 GB | 0.22 s (3.3 GB/s) | 0.1 s |
| Live (92 saves, 95 restores) | 1.88 GB, of which 1.24 GB is checkpoint sidecar | 3.05 s median (0.59 GB/s) | 1.38 s (1.32 GB/s) |

- The engine's IO code is fast.
- Live pages carry about 2.5× the bytes, mostly checkpoints of the hybrid model's recurrent state.
- Live pages also move 5–6× slower. The suspect is memory pressure: 8.4 GB of swap in use, with the engine's 17 GB `--cache-ram` and `--ctx-checkpoints 32` beside the 22 GB model and 4 × 159k KV. This is being sampled.
- The size quote is cheap and is ruled out.

**What follows:** plan rows SV-F1 to SV-F5 (section 8.2).

### 4.1c The engine is fed by queues and signals, never held by permits (2026-10-28)

Joel, 2026-10-28: "Its got to flow or have MANY queues"; "Semaphore really only exists for queue
inserts and signals typically. Otherwise you lock up every thread." The CBAR shape applied to
serving: stages run concurrently, work is submitted, the engine drains what is ready.

**What it replaces, measured on the M5 (176 acts, build 7ace74020).** A resident's model call
passed four permit layers that each counted the same engine slots and were held for the whole
call: cognition's serving lane (one of four reserved for directed calls, lent only to calls under
60 s, which a 71 s median act never is), a prefill throttle (1 to 2 permits for 72% of the time,
guarding a compute-buffer spike one engine never draws), the adapter's concurrency permit, and the
slot pool. Waiting on the first two: a median 25.7 s plus 24.6 s, in a 155 s median act, while the
engine itself was occupied 99.6% of the time and four streams decode 3.4x one stream at 64k. The
locks, not the engine, made minds wait. Each was added after an incident instead of fixing it.

**The rule.**
- A mind *submits* a generation (request plus a reply handle) and keeps flowing: tools,
  perception and the prefill of her appended items run while she waits.
- **One dispatcher per engine**, woken by events only: a submission arrived, a slot finished, a
  page landed. It fills free slots from per-class queues (directed, work, background), oldest first
  within a class with aging, so nothing starves. Priority is queue order, never a reserved slot.
- **Admission is memory, once.** `MindMemory::seat` admits a mind by KV bytes. No per-call permit
  counts slots a second time.
- A semaphore only signals: an item entered a queue, or a slot freed. No permit is held across a
  model call, a tool, or an await on another subsystem.
- The engine's own continuous batching is the concurrency: every seated sequence with tokens to
  process rides the same forward pass.

**Classes, the GCD way** (Joel, 2026-10-28: "no one is ever not getting their task done so a dream
still happens just irregularly and doesn't eat into work"). Each mind is a serial queue (one act at
a time) that targets a quality-of-service class; the engine is the concurrent pool beneath them.

| class | work | rule |
|---|---|---|
| interactive | a directed reply | the next free slot; preempts a lower class at a decode-step boundary |
| work | a resident's act on her card, with her tools | runs whenever ready; never waits behind the classes below |
| utility | ambient thought, perception, gene exams | fills what the two above leave |
| background | dreams, training, consolidation | runs in the gaps; yields at the next step to anything above |

Background is throttled, never starved: a waiting task's effective priority ages upward, so a dream
gets its step even on a busy node, irregularly and in small pieces, and it only ever takes a slot
no higher class wants now (preempted by `page_out`, ~0.1 s, the moment one does). Training is the
same: the engine's idle task, never a hold on serving.

**One owner, one heap.** The dispatcher task owns its ready heap and receives events over a channel
(submitted, slot freed, page landed, priority aged); it pops only when a slot is free. The kernel's
`concurrency::ConcurrentPriorityQueue` forwards each item to an unbounded output channel as soon as
it is signalled, so it does not keep priority order under load; the dispatcher does not build on it.

**Done so far** (`fable/lanes-flow`): the reserved lane is lent whenever nothing directed is
pending; the prefill throttle is deleted; deliberation and dreams take no lane. Left: the adapter's
concurrency permit and the slot pool become the dispatcher above, the ambient anti-stampede permit
becomes a class in it, and the act clock's queue-time registration goes with the lane.

### 4.2 Reuse versus write

| Option | Speed | Effort | Risk |
|---|---|---|---|
| A. Grow `llamacpp_scheduler` into a full engine in Rust | Saves about 1 ms of HTTP per call, nothing that matters against 60 s | Months: rewrite checkpoints, MTP, templates, tool and reasoning parsing, learning custody | Template and hybrid correctness |
| B. Link `server-context` as a library and add mechanism verbs | Identical engine and every fork feature | Weeks: about 1-2k lines of C++ verbs plus the Rust host | C++ exceptions crossing FFI; private impl |
| C. llama-server plus new HTTP endpoints | Same as B | Lowest | No Rust policy; JSON on the hot path |

**Recommendation: B, in two steps.** The new verbs go into the fork's `server-context` first:
- `page_out`/`page_in` to a named RAM page, with disk spill;
- `reconfigure` without reloading the model;
- per-task priority;
- `share_head`.

Step 1 exposes them over today's llama-server HTTP, which is option C, so the gains land before the host changes. Step 2 has a Rust `continuum-serving` binary link `server-context` and `llama-server-impl`. Rust owns the policy modules and serves the binary port, and the same process serves the OpenAI HTTP adapter. That keeps the endpoint working with exactly one copy of the weights. Hosting llama-server next to an engine inside the core would load the weights twice, which 64 GB cannot afford.

`llamacpp_scheduler` stays for small and embedding models. inproc_bench becomes the validator harness, retargeted to `ServingPort`.

### 4.3 Memory model

- **Budget.** After weights load, the budget is the device's measured free memory (`ggml_backend_dev_get_props`) minus the measured compute buffers (`memory_breakdown`, server-context.h meta). It is split into:
  - the KV pool, measured as `get_size_ext` per token;
  - fixed recurrent cost per sequence, measured as `get_size_ext(PARTIAL_ONLY)`; the recurrent size is `n_seq_max` cells (llama-model.cpp:2489);
  - checkpoints, at their measured bytes;
  - the RAM page tier.
- **Sizing comes from measurement, not counts.** `n_ctx` is the KV pool divided by bytes per token. With a unified KV, any seq can grow to `n_ctx` (llama-context.cpp:286), so windows are never fixed per slot and never shrink. Sequence capacity `n_seq_max` is the #5188 contract: the most sequences whose fixed cost fits. Admission seats a mind when her frontier fits; otherwise it pages a victim out.
- **Victims.** Evict the lowest score among unpinned minds. The score is temperature (with aging) weighed against cost: the bytes to save versus tail tokens divided by the measured prefill rate. The page goes to RAM first, which is a memcpy on unified memory. RAM spills to disk on pressure or by temperature, and the disk write runs off the engine thread on bytes already in RAM.
- **"A relaunch is a pause"** means:
  1. stop admitting and drain at a ubatch boundary;
  2. `get_data` every resident seq (target, draft and checkpoints) into RAM;
  3. `llama_free` both contexts, keeping the model and its mmap;
  4. `llama_init_from_model` with the new geometry;
  5. `set_data` everything back and resume.

  A KV-type change is classified as an intentional cold start. A process exit saves to disk. The next process maps the weights warm from the page cache and pages minds in lazily by temperature.

### 4.4 Speed levers, ranked by gain per effort

1. **[actions] moved to the tail (mind side).** The ring sits mid-prompt and forces 16-29k tokens of re-prefill per act (section 5, "Why"). Measure: replay reuse 35% → ≥90%, and fresh tokens per act.
2. **No re-prefill on mind switch.** Keep 4 seqs resident and use RAM paging. Today each switch costs a 66-100 s save, or 70-160 s of re-prefill at about 190 tok/s. Measure: page-in ms, fresh tokens per act, and `PageIn::Restored`/`Resident` share (turn_admission.rs).
3. **Batched decode of N minds.** Measure: aggregate tok/s at 1/2/4 minds against solo.
4. **Host-RAM KV cache.** This is the mechanism behind lever 2 once the pool is over budget. Measure: RAM restore ms against disk restore ms.
5. **MTP.** Measure: acceptance and decode tok/s, especially right after a page-in, because the draft comes back cold.
6. **Prefill during tool calls.** Measure: submit-to-first-token.
7. **Prefix copy.** About 0.5 GB per mind, and it only helps a cold mind's or a fork's first act. Measure: first-act prefill tokens.
8. **KV quantization below q8_0.** Little gain once 4 minds fit. Measure: footprint, plus a quality A/B.

### 4.5 Build order

The slices S0-S7, with their validators and owners, are plan rows SV-S0 to SV-S7 (section 8.2).

Lever 1, the [actions] move, runs in parallel on the mind side under line 2 (Fable + Codex).

#### S2 in detail: `reconfigure`, grow or shrink live (Joel, 2026-10-30: "a truly dynamic llama.cpp that can reduce OR EXPAND slots is crucial"; "make it organized, not some mess")

**Why:** a user starts a game or Photoshop and the engine must give memory back; they quit and it takes it
again (ONE-BUDGET.md, acceptance: the user's own load). The fork today can only do it by relaunch: its sleep
path calls `destroy()` then `load_model()` (server-context.cpp:978-993), which frees and reloads the weights.

**Shape.** Our code in the fork is held to our rules: the logic lives once, in its own module, and the
6,495-line `server-context.cpp` gains only a hook.
- `server-reconfigure.{h,cpp}`: one entry, `reconfigure(const Geometry & target) -> ReconfigureOutcome`.
  - `struct Geometry { int32_t n_ctx; int32_t n_seq; }` in;
  - `ReconfigureOutcome` out: `Applied { Geometry live; }`, or `Refused { std::vector<SlotRefusal> }` where
    `SlotRefusal { int32_t slot; int32_t tokens_held; int32_t tokens_fit; }` names each slot whose state the
    target cannot hold. Typed in the core path; JSON only at the HTTP edge (`POST /reconfigure`).
- `server-slot-page.{h,cpp}`: the one state mover. `SlotPage page_out(slot)` (target, draft and checkpoints via
  `llama_state_seq_get_data`) and `page_in(slot, const SlotPage &)`. The existing `/slots?action=save|restore`
  become callers of it (disk is a `SlotPage` written to a file), so there is one copy of the mover, not two.
- Weights and contexts are separate lifetimes, today fused in three places: `common_init_result` owns the target
  model and context together (`common.cpp:1404`); `common_speculative_init_result` both loads the draft model and
  creates the draft/MTP context (`speculative.cpp`); `load_model` builds weights, contexts, slots and batch in one
  pass (server-context.cpp:1030-1300). The split: `load_weights()` (target model, draft model, mmproj: these
  survive) and `build_contexts(Geometry)` (target and draft contexts, speculative state, slots, batch: these are
  rebuilt). `load_model` = `load_weights` + `build_contexts`; `destroy` = `destroy_contexts` + `free_weights`;
  `reconfigure` = page out, `destroy_contexts`, `build_contexts(target)`, page in. The two common-side owners
  split the same way, so the context is created in exactly one function per kind.
- The slot page reuses what exists: `server_prompt_cache_state` (prompt tokens with their host-memory
  checkpoints, `{main, drft}` state bytes, adapter set) is the page. Its capture (`server_slot::prompt_save`,
  server-context.cpp:276-300) and restore (`server_prompt_cache::load`, server-task.cpp:1895-1930) become
  `page_out` and `page_in` over an exact slot, and the prompt cache calls them. Checkpoints are host memory
  already, so they survive a rebuild untouched.
- The hook: a `SERVER_TASK_TYPE_RECONFIGURE` handled in the task phase, the safe point the fork already argues for
  `SLOT_SAVE` (server-context.cpp:2685): no `llama_decode` in flight, every sequence's KV at a consistent
  inter-batch boundary. The safe point is between BATCHED decode steps, never "between generations": every
  slot is paged out including unfinished generations, the contexts are rebuilt, everything is paged in, and
  each generation continues from its next token with its host-side sampling state. Waiting for generations to
  end would let four busy writers, or one ten-minute generation, hold the person's game hostage (Cormac on
  #5607). Drafting restarts on the rebuilt speculative object. Slot save and restore serialize through the
  same safe point. New work waits in the queue; nothing is
  cancelled. Slot ids stay stable: a smaller `n_seq` removes the highest ids, and a slot among them, busy or
  not, is paged out now with its unfinished generation (contract 2), named in the outcome; it continues when
  residency places it again.

**Contract.**
1. The model and its mmap are never freed; only contexts are (sequence in section 4.3, "a relaunch is a pause").
2. Shrink never truncates. A slot whose state does not fit the target (a smaller `n_ctx`, or an id past a
   smaller `n_seq`) is paged out now, through the same mover (RAM, then disk by temperature), never cut. The
   outcome's `SlotRefusal` names it so the core knows; residency (L4) then chooses by temperature between
   keeping it paged until room returns and shrinking that mind's view at her next epoch start (Cormac on
   #5607). Nothing waits for an epoch to free the person's memory.
3. Grow and shrink are the same path.
4. Each reconfigure reports its cost: pause seconds against tokens held, per backend, which L4 and L6 price.
   Footprint is measured as `phys_footprint` on Metal and device memory on CUDA.
5. The applied geometry is reported (`/props`, the `CapacityReport` of S3), so serving and every mind's
   binding read the live window; the core never infers it.
6. A KV type change is not a reconfigure; it is the intentional cold start (#5188).

**Validator (seconds, small hybrid GGUF, no residents):** four generations in progress across a reconfigure
finish token-identical to an unreconfigured control; and 100 cycles alternating grow and shrink with four
sequences holding state: footprint flat (phys_footprint before and after within the measured noise), weights
never reloaded (load counter), every sequence's state byte-identical across the round trip (its next token
matches an unreconfigured control), a shrink below a held state pages that slot out and names it, and no request
lost (queued requests all complete). **Core (S4, Codex):** the relaunch call sites (`reconcile.rs` re-home,
`launch.rs:221`) call `reconfigure`; a plan below the live geometry now reconfigures too, which is what
gives memory back to the user's app.

### 4.6 Risks

- **Metal and unified memory.** RAM pages, checkpoints, KV and weights all draw on one physical pool, under macOS's GPU wired limit. If the budget is not one pool, the machine swaps and everything stalls. `get_data` has to synchronize the context first. `LLAMA_STATE_SEQ_FLAGS_ON_DEVICE` (llama.h:1024) suits speculative checkpoints only.
- **Hybrid checkpoints.** Checkpoints are about 420 MB each (from e7e1077b3's numbers), so their count must be budgeted. Partial `seq_cp` is wrong for recurrent state. The shared-tail abort at llama-memory-recurrent.cpp:774 is a hazard.
- **Template fidelity.** Keep one renderer, common/chat. Test: captured requests produce identical tokens through llama-server and through the port.
- **LoRA per-seq mixing.** The server requires equal adapters to batch (server-context.cpp:432-437). Each mind's seq keeps her own gene set. Mixing different genes in one ubatch needs per-token adapter kernels, so defer it and measure what group serialization costs.
- **Crash isolation: run the engine as a separate process, not inside the core.**
  - The engine aborts on paths we exercise: line 774, and the failed reload on wake at server-context.cpp:988.
  - The core redeploys several times a day. An engine inside the core would lose all GPU KV and reload the weights on every deploy.
  - The ledger's incarnation and resident adoption (saved_ledger.rs) already let a restarted core take over a live engine with warm KV.
  - The IPC cost is one request per act plus 30-120 token frames a second over a unix socket: microseconds against a 60 s act.
  - MODULE-ARCHITECTURE §9.2 rule 2 asks for one protocol per seam, local or remote.
  - The in-process `ServingPort` implementation exists only for the validator and tests.

### 4.7 Memory is conserved across minds, and the engine says what it holds (2026-11-03)

Joel, 2026-11-03: "One slot grows in context and the other shrinks and same for genome sizes. The
goal here is dynamic allocation"; "think harder about conservation around multiple minds or sharing
what you need across api boundaries so that intelligence can manage"; "you just treat llama.cpp
like any other daemon and interface with it. It's fully under your control." The engine is our
daemon. Nothing about a mind's memory is fixed at launch and guessed: the engine reports what it
holds, the core tells it where her prompt's boundaries are, and the core's ledger decides.

#### 4.7.1 What the engine holds (fork `resident_state`, read live on the M5, 2026-11-03 12:51Z)

Qwen3.6-35B-A3B, `-c 257214 --parallel 2 --ctx-checkpoints 32`, two 13.9k-token prompts in:

| Holder | Bytes | Notes |
|---|---|---|
| Target weights | 20.69 GB | mmapped |
| Target KV + recurrent state | 2.14 GB | 8,317 B/token over 257,536 tokens, reported as one figure. Two holders: attention cells per token (shareable by `seq_cp`) and recurrent state per sequence (never shared); #91 reports them separately |
| Target compute graph | 1.87 GB | one graph for the shared pool |
| Draft (MTP) context | 3.15 GB | 1.34 weights, 0.53 KV, 1.29 compute: unreported before |
| Projector | 0.90 GB | unreported before |
| One checkpoint | 94.6 MB | x 32 per sequence = 3.03 GB per mind at the ceiling |

`memory_breakdown` alone covered the first three. The rest was what the plan had been inferring from
the process footprint as a per-lane fixed term (2.8 to 4.5 GB a minute apart).

#### 4.7.2 Why a mind's prompt re-reads: measured, not assumed

Engine log, 1,244 checkpoint resumes: 80% resumed within 8k tokens of the end (median 2,431 tokens
re-read, her changing tail). The other 20% fell back to her head-end checkpoint (~11k tokens).
The prompt captures of the four residents (`fixtures/prompt-captures`, 611 consecutive pairs) say why:

| Where her next prompt first differs | Pairs |
|---|---|
| Her changing tail (the prompt only appended before it) | 435 |
| Identical | 62 |
| She moved to another room (activity); 52 of 59 keep her first 3 messages | 59 |
| A turnover: her view restarted, and an early block differs | 29 |
| Live state in an early block with no turnover (`room-kanban`'s "557 claimable" counter, `room-wall`, `workspace-map`, `recall`) | 11 |
| Tool surface changed | 9 |
| A turnover with only later messages different | 6 |

Of the 40 turnovers, 26 came on the same core at the same window: her view filled its grant, the
codec's one designed re-read. 12 came across a core restart (the day's deploys and rollbacks): her
view is not carried over a restart, so a deploy starts every mind over. 2 came with a changed window
on the same core (one grown, one shrunk).

Minds share only 6,177 chars of system prompt with each other (it diverges inside it). The
recurring 11,114-token checkpoint is each mind's own head end, not a shared one.

So the blind every-4k checkpoints were covering defects upstream of the engine: a view that does not
survive a restart, live values in blocks meant to be stable, and an unstable tool surface (goal 10).
The fills themselves are by design, and one checkpoint at her stable end serves every act between them.

#### 4.7.3 The contract across the seam

- **Core to engine, per request:** her prompt's boundaries as token positions: the head end, the
  stable end (where her changing tail starts), and each activity's span. The request already
  carries `view_messages` spans for this. The engine keeps a checkpoint exactly at the stable end
  (one per mind per activity), never by a count or a step.
- **Engine to core, live (`/slots`, read on the server thread):** per sequence, the tokens it holds,
  its checkpoints and their bytes, and what is shared with another sequence; on `/props`, every
  fixed holder (4.7.1). A core never infers a holding from the process or the device.
- **The core's ledger, continuous:** weights and the graph are fixed claims; each mind's context is
  her grant, growing with her and accommodated proportionally (Joel: "it might still be given less
  than that, just try to accommodate"; her budget compresses to the grant); an in-flight turn's grant
  never shrinks. Checkpoints and pages are held at their live bytes. Under pressure a cold mind's
  unique state pages to RAM, then disk, by her activity temperature, before any grant shrinks.
- **Conservation:** what is identical across sequences is held once. Today that is the first 6k
  chars; ordering the shared constitution and the stable tool surface ahead of her identity makes
  the shared head the system prompt plus the tools, one KV run and one checkpoint for every mind
  (`seq_cp` of the attention cells plus the head checkpoint, 4.1). Genes follow the same rule: one
  copy per adapter, attached to every mind whose focus selects it.

#### 4.7.4 One solver for prompt and memory (ONE-BUDGET.md)

Joel: "it's always collapsing flexbox style ... everywhere same base classes and interfaces." The base exists:
`cambrian-flex` (airc `crates/flex`, merged as airc#1630), ONE-BUDGET slice 1. Claims are cleared by measured value
per unit against a scarcity price; an item's levels are indivisible increments with their own value; a move between
levels carries its measured `switch_cost`; custody, provenance and generation fencing are in the type. A mind's prompt
and the engine's memory are two tiers of the same solver, and each keeps only its levels and how to move between them.
The engine-memory slice, with caching, collapsing and expansion as one decision valued by prediction, is ONE-BUDGET.md
"Slice 3b". The hand-rolled memory allocators (`window_within`, `lanes_that_fit`, `pool_within`,
`window_allocator::allocate`, the kernel `memory_manager` budgets, `gene_residency::victims`, the persona allocator's
tiers and the rest) converge on it and are deleted; the gate goes to zero.

#### 4.7.5 Order of work

Items 0-5 are plan rows CX-2, CX-3, CX-4, SV-CK, SV-LEDGER and SV-S7 (sections 8.2 and 8.3).

#### 4.7.6 The engine's host memory is one streaming pool, never two heaps (2026-11-16, for review)

Joel, 2026-11-16: "you need to focus on the graphics engine level design". A game engine never lets a resource
class grow until the OS swaps: textures stream inside one pool of a fixed size, a request that would overflow it
evicts the least valuable page first, the pool reports what it holds every frame, and exceeding the budget is a bug a
validator catches, never a cost the frame pays. Section 2.3 already maps this (virtual texturing), and ONE-BUDGET
slice 3b already designs the solver that values every holder. What was never built is the pool itself in the engine,
so the running engine still has two independent heaps.

**What happened** (the M5, 2026-11-16, canary 73d999387 then 33b21b8cf, engine pid 64859 up since 11-15):

| When (Z) | Engine compressed | Swap used | Decode | A resident's act |
|---|---|---|---|---|
| 08:49 | 9.2 GB (RSS 33 GB) | 5.9 GB | 2.6 tok/s, expected 56 | Atlas: 1,508 s, 998 s of it decoding 2,593 tokens |
| 12:56 | 12 GB | 8.7 of 9.2 GB | 1.7 tok/s | median 811 s, p90 1,362 s over the hour |

The residents' own path was healthy at the same time (0 lane-starved, 0 dropped, prefix reuse 72-84%): the loss was
the decode step waiting on decompression and swap. A stop and fresh engine at 13:00:18Z (`continuum stop --engine`,
`continuum start`) freed it; the KV each mind held was the price (4.9 is the stop that keeps it).

**Why: two holders, sized apart, each a pool that never evicts against the other.** The engine relaunched at 13:01Z
runs `--cache-ram 9494` (the prompt cache, 9.5 GB of host RAM) and `--ctx-checkpoints 32 --parallel 5` (up to 160
checkpoints at the ~95 MB each of 4.7.1, ~15 GB). Each is sized by its own derivation (`host_prompt_cache_mib`, the
checkpoint count), neither knows the other, and nothing holds their sum: with the weights (20.7 GB), draft (3.15 GB),
projector (0.90 GB), KV and graph, the engine may claim ~53 GB of a 64 GB machine that also runs the core and the
user's own programs. The core also cannot see what it got: its launch probe says "applied limit and serialized-state
telemetry are not exposed by current engine API" (`serving.prompt_cache.launch`, 13:01:01Z).

**The design: one pool, one budget, eviction by value at the allocation, reported every request.**
1. *One host pool in the fork.* Prompt-cache entries, checkpoints and RAM pages are resource kinds of ONE host pool
   with ONE byte budget. There is no per-holder size and no count: `--cache-ram` and `--ctx-checkpoints` as separate
   limits are deleted (slice 3b already lists their derivations for deletion). RAM pages join the accounting contract
   now as a kind; allocating them stays with the pager that owns paging (4.1a), never a second pager or heap here.
2. *Bytes are reserved before they exist.* A checkpoint save or cache store first reserves its full peak in the pool:
   the serialized size, its temporary serialization buffers, the overlap while a replacement and the entry it
   replaces are both alive, and the allocator capacity it will hold. Reserving evicts the lowest-valued evictable
   residents until the reservation fits; the copy runs only inside a granted reservation, the reservation is rolled
   back on failure, and the old valid entry stays until its replacement is committed. A size check after serializing
   would already have allowed the peak that swaps. This runs on the engine's thread at the allocation, the way a
   texture streamer evicts before it uploads, never by a later sweep. The pool owns reservations and eligibility; the
   engine thread owns native state mutation; disk work, when it comes, goes through the existing asynchronous
   completion owner, never blocking decode.
3. *Pinned is not evictable.* A checkpoint being restored, a cache entry an act is resuming from, and a save in
   flight are pinned. An allocation that cannot fit after legal eviction of every unpinned resident is a typed refusal
   (oversized, or pinned pressure), counted, never an overflow. A shrink below what pinned residents hold cannot apply
   at once: it returns a typed outcome naming the target, the committed, pinned and reserved bytes (applied, pending,
   or refused), and the pending part completes through the existing owner as physical custody ends.
4. *One value unit: expected seconds saved per byte kept.* A checkpoint's value is the prefill time it saves (the
   tokens from its position to its sequence's end, at the measured prefill rate) times the measured chance its
   sequence resumes from it, per byte it holds; a cache entry's is its measured restores times the prefill time each
   saved, per byte. Each value carries its provenance; an unmeasured resident is in an explicit `Unmeasured` state,
   never a guessed number; ties break deterministically (oldest use first, then id). This is the one ordering until
   the core's solver supplies values (slice 3b): no second, independent sizing policy in the meantime.
5. *The budget is the core's, and it moves live.* The authority's device tier grants serving's host pool its bytes
   (the machine's measured memory less every other claim, the user's own programs included, ONE-BUDGET acceptance);
   the core sets it over the engine's control seam and may raise or lower it at any time without a relaunch (S2's
   `reconfigure`, 4.5), each grant carrying a generation.
6. *The pool says what it holds, keyed by instance and generation.* `/props` (and `/slots` per sequence) reports the
   budget as applied with its grant generation and the engine instance, the committed, reserved and pinned bytes by
   kind (cache, checkpoint, page), the count per kind, and the evictions and typed refusals since start. The core
   matches each readback to the engine instance and grant generation it sent (through a concurrent resize or a
   restart) and refuses to treat an unreported or stale limit as applied.
7. *Two invariants, measured separately.* (a) The pool's accounting: committed plus reserved bytes stay at or under
   the grant at every allocation, asserted in the fork. (b) The process: its private, resident, compressed and swapped
   bytes are measured on their own, because logical pool bytes do not bound them (scratch and graph allocations,
   allocator-retained capacity, mapped weights and the OS's compression are outside the pool); a process that grows
   past its fixed holders plus the grant is a defect with a probe (`serving.host_pool.over_budget`), never a cost. The
   simulator (7.0) proves admission and accounting: a day of the four residents' captured requests against a grant a
   quarter of their unbounded need, every allocation within the grant, everything served that fits the grant after
   legal eviction of unpinned residents, and only oversized or pinned-pressure refusals. Decode at par and a flat
   process footprint are proven by native replay and the live read, not by the simulator.

**What this makes of the pieces already in flight.** Placed checkpoints (fork #121) stop making the worthless ones;
the pool makes even a wrong placement unable to swap the machine. The safe stop (4.9, #6042) keeps a mind's KV across a
restart; the pool removes the reason a restart was the only relief. Slice 3b's solver later supplies the values and
the budget; the pool is the primitive it drives, the way a renderer drives a streaming pool it does not reimplement.

**Order and owners** (agreed with Codex on #6081, 2026-11-16). (1) Fable: the fork's pool (reservations, pinning,
eviction by value per byte, typed outcomes, the instance- and generation-keyed report), proven by native tests that
fill a small grant with checkpoints and cache entries under concurrent saves and restores, and assert the accounting
invariant at every allocation, the highest value per byte kept, and only typed refusals. (2) Codex: the core's
authority grant to the pool, applying it live with its generation and reading it back by instance, and the deletion of
both independent sizers (`host_prompt_cache_mib`'s figure and the checkpoint-count derivation). (3) The simulator
scenario above as the accounting gate; then native replay for decode at par, and the live read: an engine up 24 h on
the M5 with its process footprint flat, swap not growing, and decode at par.

**Wire contract (fork half, for Codex's core half; same control seam as `POST /engine/decode-set`, #120).**
- *Identity.* An engine instance is `{pid, started_s}` (the core's `EngineIncarnation`). Every request names the
  instance it means and the grant generation it carries; a request for another instance is refused, never applied.
- *Launch.* `--host-pool-bytes N` is the startup fact: the instance's generation-0 grant, set by the core from the
  authority at launch. There is no other size: `--cache-ram` and `--ctx-checkpoints` as limits are gone. An engine
  launched without it refuses to start (typed), never picks a default.
- *Set.* `POST /engine/host-pool` with `{"instance": {"pid", "started_s"}, "generation": g, "budget_bytes": B}`.
  `B = 0` is legal: every unpinned resident is evicted and pinned bytes drain as they release. Generations rise
  strictly: `g` below the applied generation is refused `stale_generation`; `g` equal to it with the same `B` is an
  idempotent repeat that returns the current state without acting; `g` equal with a different `B` is refused
  `generation_conflict`.
- *Outcome (the reply, and `/props.host_pool` at any time).* `{"instance", "generation", "outcome", "target_bytes",
  "ceiling_bytes", "committed_bytes", "reserved_bytes", "pinned_bytes", "by_kind", "since_start"}` where:
  - `outcome` is `applied` (committed + reserved <= target; ceiling = target), `pending` (pinned or in-flight reserved
    bytes exceed the target; ceiling = the bytes still held above it, falling as custody ends; the same generation
    reports `applied` once reached), or `refused` with `{"kind": "wrong_instance" | "stale_generation" |
    "generation_conflict"}` and the state left exactly as it was;
  - `pinned_bytes` is a subset of `committed_bytes`, never added to it;
  - `by_kind` has `cache`, `checkpoint` and `page`, each `{committed, reserved, pinned, count}`;
  - `since_start` counts `evicted` per kind and `refused` per cause (`oversized`, `pinned_pressure`).
- *Per sequence.* `/slots` gives each slot's checkpoints with their bytes and whether each is pinned, beside the
  positions #121 reports.
- *The core's reading.* The core applies a grant only when the reply (or a later `/props`) names the same instance
  and generation; anything else is not applied. A restart is a new instance, so its generation 0 is the launch grant.

**Receipt since the stopgap restart.** The fresh engine (pid 97364, 13:01Z) decoded at 17.4 and 39.1 tok/s on its first
acts, against 1.7 before; five minutes after launch it already held 7.6 GB compressed as the minds re-prefilled cold.
A restart buys hours, not the fix.

#### 4.7.7 The decode thread only submits; bytes move by DMA (2026-11-16, for review)

Joel, 2026-11-16: "We should target 100 or more tok/sec ... Every wait considered suspect. Like we are building
unreal engine. Consider anything the cpu is doing and whether it can be avoided. Anything contesting"; "That you
have contention means you're not coding correctly. DMA too." The M5 Pro has 18 CPU cores (6 performance, 12
efficiency), a 20-core GPU and 64 GB of unified memory. Contention on it is a defect of the code, never of the
machine.

**Measured** (the live engine pid 97364, `sample` for 8 s at 13:2xZ, three slots processing): the decode thread
(`server_context_impl::update_slots`) was busy 4,304 samples. About 55% was the GPU working (`ggml_metal_synchronize`
inside `llama_decode`); the rest was CPU work in series while the GPU waited: building and splitting the graph and the
attention mask (`set_input_kq_mask_impl`, `ggml_backend_sched_split_graph`, `ggml_gallocr_alloc_graph`) about 20%,
sampling (`common_sampler_sample`: the penalties pass and a partial sort of the 150k-token vocabulary, per token, per
sequence) about 14%, `create_checkpoint` serializing state about 8%, and `seq_rm`, batch setup and log formatting
(`vfprintf` on the hot path). The ggml CPU thread pool spent 3,257 samples spin-polling (`--poll` defaults to 50) for
work it almost never gets, burning the power budget the CPU and GPU share; its only real op was `get_rows`.
~~The GPU idles for roughly 40-45% of the decode loop.~~ **Retracted 2026-11-16** (Codex's review of 9d001e9a4): a CPU
stack sampled in `ggml_metal_synchronize` measures the host waiting at that call, not GPU busy time, and command
buffers execute while `llama_decode` is still encoding. The CPU percentages above are host observations only; GPU
utilization is measured from command-buffer timestamps (4.7.9, `/props.gpu_time`).

**The rule, for every engine-side concern** (each one an `engine_module`, server-module.h, never a hand-built thread):
1. *The decode thread only submits.* It hands the GPU step N; step N+1's inputs (batch, mask, graph) are built while
   step N runs, the way a render thread records the next frame during the current one.
2. *CPU work fans out and stays off the critical path.* Sampling runs per sequence on its own worker module, or as a
   GPU top-k so only k candidates return; tokenizing, templating and JSON never run on the decode thread; logging is
   off the hot path; the CPU pool sleeps instead of spinning (`--poll 0`, threads sized to the CPU work that remains).
3. *Bytes move by DMA, never by CPU `memmove`.* On unified memory the KV already lives where both sides see it. A
   checkpoint or a cache entry is a GPU blit into a pool-owned buffer, enqueued on the command stream and overlapped
   with compute, committed to the host pool (4.7.6) when it completes; where nothing has to move, the pool holds a
   reference-counted handle to the pages instead of a copy. The CPU decides what moves; it never moves it.

**Gate.** The GPU-idle share of the decode loop, sampled the same way, falls from ~45% toward the time the GPU needs
between steps, and decode tok/s per stream rises with it, measured before and after each item on the same node, one
item per change. Each item lands with its own before/after numbers; the per-stream ceiling for this model on this chip
is measured first with `llama-bench` at the residents' depths, so "at par" is a measured number, not an assumption.

#### 4.7.8 What a graphics engine already solved: bgfx, read in its source (2026-11-16)

Joel, 2026-11-16: "Study unreal engine or bgfx ... I am serious about graphics engines. They're the same problems."
bgfx (read-only clone at `~/.continuum/reference/bgfx`, 3aab320) solves the serving engine's problems in a renderer.
What it does, where, and what each becomes here:

| bgfx (file: symbol) | What it does | Here |
|---|---|---|
| `bgfx.cpp: Context::frame`, `swap`, `renderFrame`; `renderSemWait` / `apiSemPost` | Two `Frame`s in flight: the API thread records `m_submit` while the render thread executes `m_render`; `frame()` waits for the render thread to finish the previous frame, swaps the two, releases it. Recording N+1 always overlaps executing N | A decode step is a frame. While the GPU runs step N, workers build step N+1 (batch, mask, graph inputs, sampled tokens); the decode thread swaps and submits. The ~20% of the decode loop spent building inputs (4.7.7) moves off the GPU's critical path |
| `bgfx_p.h: FreeHandle`, `m_freeIndexBuffer.queue`; `bgfx.cpp: freeAllHandles(frame)` | Destroying a resource queues its handle in the frame that submitted it; the real free runs after that frame has rendered. Nothing is freed while the GPU may read it | A step keeps a reference to every resident its commands touch (a checkpoint a restore reads, a buffer a blit writes) and drops them when the step retires (its GPU completion). `host_bytes`' immediate RAII release is correct while copies are on the CPU, and becomes a step-retired release when copies move to GPU blits (4.7.7 rule 3) |
| `bgfx_p.h: m_encoder[]`; `bgfx.cpp: encoderApiWait` | Many threads record into the same frame through their own encoders; `frame()` waits for every encoder to end before the swap | Per-sequence workers (sampling, a stop check, a tool-call parse) record into step N+1 through their own encoder; the swap waits for them. No sequence's CPU work runs on the decode thread |
| `bgfx_p.h: kSortKey*` (64-bit sort key: view, draw type, blend, program, depth) | One key per draw and one sort per frame, so the GPU changes state as rarely as possible | One key per sequence in a step (adapter set, then sequence, then position), one sort per step: sequences sharing an adapter set run together (the decode-set cohort of #120 becomes a sort key, never a scheduler branch) |
| `bgfx_p.h: m_iboffset` / `m_vboffset` reset per frame; `createTransientIndexBuffer` | Transient vertex/index data comes from a per-frame bump allocator reset at the frame's end: no malloc or free per draw | Per-step scratch (token arrays, sampler candidates, JSON fragments, mask rows) from a per-step arena reset when the step retires: no allocator on the hot path |
| `bgfx.cpp: rendererExecCommands(m_cmdPre / m_cmdPost)` | Resource creation and changes are commands queued into the frame and run in order on the render thread at the frame boundary, never locked against the frame in flight | The core's control (a host-pool grant, a decode width, an adapter attach) is a command queued into the next step and applied at the step boundary on the decode thread: no lock held against a running step, and the applied outcome is reported from the same place |
| `bgfx_p.h: HandleAllocT` | Resources are 16-bit handles into fixed tables, never raw pointers | Residents are pool ids; slots, sequences and adapters are typed ids; nothing crosses a thread boundary as a raw pointer |

Unreal, from its documented engine design rather than source here: the Render Dependency Graph (passes declare what
they read and write; the graph orders them, inserts barriers and aliases transient memory), the RHI thread (command
translation off the render thread), async compute queues, and the texture streaming pool (already 4.7.6). The step
graph above is the RDG in miniature: a step declares its reads (KV, checkpoints, adapters) and writes (KV, new
checkpoints), and that declaration is what lets prefill run on its own queue beside decode (the prefill-blocks-decode
stall, 9-12 tok/s per stream while a 134k-token prefill runs).

**Order** (each with its before/after on the M5, one per change): the per-step frame and its step-retired release;
encoders for sampling; the per-step arena; control as step commands (`POST /engine/host-pool` lands this way, so the
core half needs no engine lock); sort keys for the cohort; then prefill on its own queue.

#### 4.7.9 Proving it: every stall is a number the engine reports (2026-11-16)

Joel, 2026-11-16: "Put it in the design document and make sure we measure and prove we aren't having these same
problems." A one-off `sample` is not proof; a stall must be a counter that is always on, read the same way on every
node, so a regression shows the step it happens and every change in 4.7.7-4.7.8 carries its own before/after.

**What the source showed** (fork #121, read 2026-11-16):
- `llama_decode` does not wait for the GPU. It submits (`ggml_backend_sched_graph_compute_async`,
  src/llama-context.cpp) and starts the logits copy back (`ggml_backend_tensor_get_async`); the wait is
  `llama_synchronize`, which the server calls right after decode for any batch with outputs
  (tools/server/server-context.cpp, the `yield_to_queue` lambda) and which `llama_get_logits_ith` would otherwise do.
  So the GPU's idle time per step is exactly: the CPU building and submitting the graph, plus everything between
  the sync and the next submit (sampling, slot bookkeeping, checkpoints, the next batch).
- GPU sampling exists in the fork (`--backend-sampling`, `llama_set_sampler`) and covers every sampler the core
  requests (temperature, the penalties, top-k/p, min-p, dist, logit bias). Only a contiguous PREFIX of a chain runs on
  the GPU (`llama_sampler_chain_backend_init`); grammar and DRY have no GPU path, and a disabled sampler becomes
  `empty`, which does, so the default chain qualifies. It is off for the target model (on for drafts). A first A/B on
  the 0.8B validator with the live engine sharing the GPU was noise (77/55 tok/s CPU vs 66/50 GPU, drifting with the
  shared load): **not adopted**; it is measured in the serving-down window below, never switched on by inference.

**Two instruments, kept apart** (fork `fable/host-pool` 659abf7e1, after Codex's review of 6651dcb65, which had
published a host-timing ratio as `gpu_idle_share`):
- *GPU busy time, measured by the device.* Every graph command buffer records its `GPUStartTime`/`GPUEndTime` into
  the Metal device (overlap counted once), exposed through ggml's backend extension point as the generic proc
  `ggml_backend_gpu_time`, so any backend that can time its own work answers the same call. `/props.gpu_time` gives
  per device `{busy_us, command_buffers, clock_us}`; **utilization = delta busy_us / delta clock_us**. It is the
  engine's utilization only where the engine is the GPU's sole client: another process's work interleaved with ours
  is counted while our buffers are in flight.
- *Host timings per step* (`tools/server/server-step-budget.h`, validator `tests/test-server-step-budget.cpp`):
  `decode_call_us` (llama_decode called to returned), `sync_wait_us` (host blocked until outputs are readable),
  `inter_step_us` (host work between steps while work is pending), and `unsynced_us` for prefill chunks without
  outputs (counted only there). These say where the host spends a step, never what the GPU does. Cumulative relaxed
  atomics in `/props.step_budget`, decode-only and prefill-carrying steps apart; a reader differences two reads, so
  the engine holds no window or threshold. They replace upstream's compile-time `DEBUG_TIMINGS` (deleted).

**First receipt, and what it is not** (qwen35-0.8b, 659abf7e1, M5, two streams of 300 tokens): MTL0 busy 7,188 ms
over 7,600 ms wall (602 command buffers); host per decode step: call 695 us, sync wait 23,592 us, between steps
909 us. This is **not** a utilization result: the live 35B engine shared the GPU, so our buffers were in flight while
its work ran (23.6 ms per step is far above this model's own time). The first real number is the live engine's own
`/props.gpu_time` once this build is deployed there.

**The target is 90% GPU busy while work is pending**, and three things stand between it and today, in measured order:
(1) the engine is starved of work: minds wait at core-side lanes while engine slots are free (4.7.10), so the GPU
decodes one or two streams instead of every mind's; (2) the CPU runs in series between steps (4.7.7: ~20% building
inputs, ~14% sampling, ~8% checkpoint copies on the M5 sample), removed by the frame pipeline (4.7.8); (3) host memory
pressure (swap, compression) stalls everything, removed by the pool (4.7.6).

**Gates, each read from `/props.step_budget` and the core's per-act receipts, before and after every change:**

| Problem | Number | Gate |
|---|---|---|
| Host work between steps (sampling, bookkeeping) | `decode.inter_step_us / steps` | falls with sampling encoders (4.7.8) and, if the window proves it, GPU sampling |
| Host building and encoding a step | `decode.decode_call_us / steps` (overlaps GPU work; read with `gpu_time`) | falls with N+1 recorded during N (4.7.8 frame) |
| Prefill stalling decode | `with_prefill.*` against `decode.*`; per-stream tok/s while a long prefill runs | prefill on its own queue (4.7.8) |
| Host memory and swap | `/props.host_pool`, the node's swap in use | swap flat under load (4.7.6) |
| Minds queued in front of the engine | the core's lane-wait per act ("lane-starved" in the health line) | zero lane wait while engine slots are free (4.7.10) |
| The whole loop | `gpu_time` busy over wall while work is pending; decode tok/s per stream against the `llama-bench` ceiling | **the GPU busy at least 90% of the time work is pending** (Joel, 2026-11-16: "It's insane that it's not 90% utilization"), measured from command-buffer timestamps on every node, and aggregate tok/s rising with the minds served, never flat while minds wait |

**The serving-down window** (one short window, Joel's rule: build or bench with serving down, never beside it):
`llama-bench` for the per-stream ceiling at the residents' depths, then the backend-sampling A/B on the 35B with
nothing else on the GPU. Both results land here with build SHA and time.

#### 4.7.11 By reference, never encoded (2026-11-16)

Joel, 2026-11-16: "Pass raw memory by reference or if caching save as its binary never encode decode ever. It's
useless slop unless you're uploading or something like that. Pass textures by id don't rasterize." Inside the engine
and between the core and the engine, a value moves as the bytes it already is: a handle or id to memory that stays
where it is, or its own binary when it must be kept (a cache file is the bytes, never a re-encoding). Text and JSON
exist only at a boundary to a person or a foreign program (an external OpenAI driver, a debug page).

**Where an act encodes and decodes today** (canary 15664c6ad, read 2026-11-16):

| Hop | Today | Cost measured | By reference |
|---|---|---|---|
| A KV checkpoint | `llama_state_seq_get_data` serializes KV out of device memory into a host blob, restored by parsing it back | host profile: `create_checkpoint` ~8% of the decode thread's samples (a host observation, 4.7.7) plus the host memory of every copy (4.7.6: ~10 GB compressed in the 13:01-13:56 receipt) | a page kept or copied on the device (`seq_cp`, a blit into a pool buffer), named by id; serialization only to persist a page to disk, as its own bytes |
| The prompt, core to engine | OpenAI JSON messages of the whole context (`request_body.rs`, 47 serde_json sites); the engine parses, renders the chat template and re-tokenizes everything to find the prefix it already holds as tokens | qwen35-0.8b tokenizer on this M5, CPU: a 521,065-char 40-message context templates in 10-20 ms and tokenizes to 126,373 tokens in 80 ms per call over HTTP JSON (fork 659abf7e1) | the context codec already knows the appended items (`continuum-context`): send the appended token ids for the mind's epoch, binary, against the slot's known prefix; in process (line 3b), a span by reference |
| Tokens, engine to core | SSE `data:` JSON per chunk, parsed per chunk | not yet measured | token ids as binary frames, detokenized once where text is shown |
| Control and telemetry | JSON `/props`, `/slots`, `POST /engine/host-pool` (this branch added `step_budget` and `gpu_time` there) | not yet measured | one typed binary snapshot the core reads (fixed layout, versioned); JSON kept only as the human/debug view of the same snapshot |

**The checkpoint by id, proven** (fork 7b-series `tests/test-seq-park.cpp`, qwen35-0.8b hybrid, M5 Metal,
2026-11-16). The fork's memory already has the by-id form: `llama_memory_seq_cp` shares attention KV cells by sequence
id, and recurrent state is copy-on-write on the device (`llama_memory_recurrent::find_slot` moves a sharing sequence to
an empty cell with `src` = the shared one; the step graph gathers it through `s_copy`). So a checkpoint is a PARKED
SEQUENCE: park = `seq_cp(live, parked)`; restore = `seq_rm(live)` then `seq_cp(parked, live)`. Measured on a 300-token
sequence: today's serialize 23,894,492 bytes in 1,783 us; parked 0 bytes moved, park 13 us, restore 40 us; the restored
sequence's next-token logits equal a cold run's (argmax equal, max |diff| 0.0044 from batch shape). The costs move from
host memory to the device: one recurrent state cell per parked checkpoint (the recurrent cache is sized by `n_seq_max`
at load, so parked ids are provisioned up front and become the pool's device-side `checkpoint` kind), and attention
cells a parked id still references stay allocated until it is released. The serializing path remains only to persist a
page to disk, as its own bytes.

*The server's form* (fork, same test): the server's checkpoints hold only the partial state (`PARTIAL_ONLY`: recurrent
/ SWA) and trim attention by position, so a checkpoint parks ONLY the recurrent cell: `seq_cp(live, parked, 0, 0)`
(an empty attention range copies no KV cells); restore is `seq_cp(parked, live, 0, 0)` then `seq_rm(live, N, -1)`,
the server's own order. Measured: today's partial serialize 20,201,932 bytes in 1,190 us per checkpoint on the 0.8B;
parked 0 bytes, no attention cells held (`seq_pos_max(parked) = -1`), park 3 us, restore 29 us, logits equal a cold
run. Honest cost: on unified memory a parked cell occupies the same bytes as the copy, in device memory; what goes is
the copy, the duplicate host buffer and the compressor churning on it. The count of parked cells is the pool's budget,
provisioned at load (`n_seq_max` = slots + parked cells). Integration: each slot's checkpoint list names parked ids
(a free list owned by the pool), evicting one is `seq_rm(parked, -1, -1)`, and the draft context's partial state and
the speculative stash follow the same form once measured.

*Live sizes, measured* (the M5's engine log `~/.continuum/logs/llama-server-58057.log`, 2026-11-16, Qwen3.6-35B-A3B,
`--parallel 5 --ctx-checkpoints 32 --cache-ram 9953 --kv-unified -c 717385`): one checkpoint is 80.573 MiB at 9,005
tokens and 141.330 MiB at 39,812 tokens, about 62 MiB fixed plus ~2 KiB per token. Five slots of 32 checkpoints can
therefore hold on the order of 16 GiB of host copies, outside `--cache-ram`'s bound: the measured source of the
swap and compression in 4.7.6's receipt. Parked, the fixed part is one recurrent cell on the device and no copy; the
per-token part must be identified before the count is set (if it is the draft's or a sliding window's KV, parking
shares those cells by id and it costs nothing; if it is per-token recurrent history, it is the cell's own size). The
number of parked cells is then a budget the pool grants from measured value per byte, never a fixed 32 per slot. Measured
next (fork `test-seq-park`, `TEST_SEQ_PARK_N`, qwen35-0.8b): the TARGET's partial state is constant, 20,201,932 bytes
at 300, 1,500 and 3,000 tokens, so it is the recurrent cell alone and parks for one fixed cell. The live checkpoints'
per-token growth therefore comes from the other parts a server checkpoint copies (`data_dft`, the draft context's
state, and `data_spec`); identifying which, and whether it is KV that parking shares by id, is the next measurement
before the count is set.

**Order**, by measured cost: checkpoints as parked sequences first (memory and copies, and the pool's largest kind);
then the prompt as appended token ids; then binary token frames; then the control snapshot. Each lands with its
before/after (host memory, act latency, the step budget).

**The binary seam, core to engine** (proposed 2026-11-16, for Codex's review before either side is built). It
replaces OpenAI JSON on the act path; the OpenAI HTTP surface stays only for external drivers (line 16).
- *Transport.* The engine's Unix socket (already present beside each engine, e.g. `~/.continuum/logs/llama-server-58057.sock`),
  length-prefixed frames, little-endian, fixed headers. In process (line 3b) the same frames become spans by reference.
- *Tokens are the unit, prepared once.* The context codec (`continuum-context`) already keys every item and only
  appends within an epoch; the core renders each item through the model's chat template once and tokenizes it once
  (one `tokenize` frame per NEW item), caching the ids under the item's key. The engine never re-templates or
  re-tokenizes a context. The vocabulary's pieces are fetched once per engine instance (`vocab` frame: every piece's
  bytes by id), so the core detokenizes by table lookup, never by a round trip.
- *Act request* (`kind = act`): `magic u32, version u16, kind u16, slot i32, page u128 (the mind's typed page id),
  epoch u64, prefix_n u32, prefix_hash u64 (xxh64 of the prefix ids the core believes the slot holds), sampling
  {temp f32, top_k i32, top_p f32, min_p f32, repeat_penalty f32, repeat_last_n i32, frequency_penalty f32,
  presence_penalty f32, seed u32, n_predict i32, think_budget i32}, n_append u32, n_stop u32`, then `i32[n_append]`
  appended ids, then each stop sequence as `u32 len, i32[len]`. The engine checks `prefix_n`/`prefix_hash` against its
  own token list for the slot: a match decodes only the appended tail; a mismatch is a typed refusal
  (`prefix_mismatch {engine_n u32, engine_hash u64}`) and the core resends from the longest prefix it can prove, never
  the whole context by default.
- *Response frames:* `tokens {slot i32, n u32, i32[n]}` as they are sampled; `done {slot, stop u8 (eos, stop_seq,
  length, cancelled), prompt_n u32, cached_n u32, prompt_us u64, predicted_n u32, predicted_us u64}`; `refusal {slot,
  code u16, payload}` with typed codes (prefix_mismatch, slot_busy, context_full, engine_stopping). Tool calls and
  reasoning are parsed by the core from ids it already holds, not re-parsed from text by the engine.
- *Control and telemetry* use the same framing (`host_pool` grant and report, `step_budget`, `gpu_time`): fixed
  structs, versioned; JSON `/props` remains the human view of the same values.
- *Gate.* Per act: bytes on the wire (whole-context JSON today vs appended ids), engine CPU before the first decode
  (template + tokenize today, measured 10-20 ms + 80 ms at 126k tokens on the 0.8B's tokenizer), and act latency;
  the validator replays captured acts through both seams and requires identical token ids and outputs.

**No promises, no futures** (Joel, 2026-11-16: "Promises and other idiocy. I wish you never saw JavaScript"). Work
off the decode thread is a job system: a job writes its result into storage its owner already owns, completion is a
monotonic ticket counter waited on with an atomic wait, cancellation is the job called once as cancelled, and queues
are fixed rings sized by their owner (fork `server-module.h`, ae8b41925's successor). A per-job heap-allocated shared
state with a mutex and a blocking `get()` is the JavaScript reflex this rule forbids, in C++ and in Rust alike.

**Ratchet** (Joel: "I wish I'd prohibited serde in some areas"): `serde_json` sites on the act path
(`continuum-engine/src/ai/openai_adapter/`, `continuum-engine/src/inference/request_body.rs`, `stream`/`lane_send`,
`continuum-context`) are counted by source-hygiene and may only fall; a new one fails the build. A crate that is
clean of it gets a compile-time ban (no `serde_json` dependency) so the reflex cannot return.

#### 4.7.10 No semaphore where nothing is scarce (2026-11-16)

Joel, 2026-11-16: "there's semaphores and other nonsense where they make no sense." The engine batches continuously
across its slots and assigns them itself; a core-side permit that bounds the same thing only queues minds in front of
an idle engine. The health line shows exactly that: residents "lane-starved" while the GPU has room.

**One act's path, traced on current canary** (15664c6ad, read-only worktree, 2026-11-16; an earlier trace from a
checkout 509 commits behind was retracted after Codex's review; the central claim below re-read by hand at
`turn_admission.rs:543`). Paths under `core/continuum-engine/src/` unless marked `cognition/`. The prefill-fit permit
and the ambient turn permit no longer exist (#5582).

| # | Gate (file:line) | Bounds | Held while waiting | Verdict |
|---|---|---|---|---|
| 1 | measured hold, `cognition/.../admission.rs:57` -> `inference/measured_hold.rs:239` | background work in a hold | nothing | real |
| 2 | lane: non-directed budget (Notify loops, `lanes.rs:462-525`) then the physical FIFO semaphore (`lanes.rs:240`) | engine `--parallel`; non-directed = lanes - 1 | nothing | priority policy is real; the COUNT duplicates 5c and the engine |
| 5a | one act per mind, `turn_admission.rs:376-391`, `slots/mind_gate.rs:72` | a mind's page | nothing (lane dropped at :385, only if the first poll is Pending) | real |
| 5b | lane re-taken, `turn_admission.rs:398` | as 2 | the mind gate and its custody read | churn: rejoins at the BACK of the queue |
| 5c | decode permit, `turn_admission.rs:409-421`, `slots/endpoint_concurrency.rs:142` | the engine's slot count | the mind (lane dropped :413), then back to 5b | **duplicate of 2 and the engine** (same number, two sources) |
| 5d | endpoint readiness + RwLock read, `turn_admission.rs:428`, `engine_generation.rs:110,154` | engine transitions | lane, decode permit, mind | real (drain before a transition) |
| 5e | slot lease, `turn_admission.rs:483-520`, `kv_slot_pool.rs:184` | citizen slots | mind only (`drop(admission)` :516) | real |
| 5g | per-slot permit, `turn_admission.rs:543` `pool.acquire_slot(slot).await` | 1 per slot | **lane, decode permit, endpoint read, pin, mind** | real (the core's record of which page a slot holds; the evicted page saved before overwrite), but must wait holding only the pin |
| 5h | paging ledger, `kv_slot_pool.rs:346` -> `saved_ledger.rs:145` | — | everything | **sync `sync_all` of file and directory on an async worker** |
| 5i | KV save/restore, `turn_admission.rs:598-649`, `paging_lifetime.rs:35` | the page | everything | real; `sleep(bound)` checkpoints with `/slots` probes |
| 6 | LoRA catalog, `stream.rs:365` | catalog miss | everything | HTTP on the act path |
| 7, 8 | residency guard, `stream.rs:491`, `lane_send.rs:230-261` (twice) + `/input_tokens` quote | model identity | everything | real once; **three guards collapse to one** |
| 9 | POST, `lane_send.rs:329` | the request | everything | a `/slots` probe beside every POST |
| 10 | 503 / refused, `lane_send.rs:401-451` | a relaunch | **everything, including the endpoint read guard** | watch with a 120 s deadline; holding the read guard can stall the relaunch drain that needs the write guard (to be checked against relaunch ordering) |

**What it proves.** (a) The lane and the decode permit are held across every wait from 5d to 10 (readiness, the
per-slot permit, paging, the LoRA probe, three residency guards, the POST, a relaunch wait) and across an unresolved
page's custody (`turn_admission.rs:162-166`): none of those needs a lane, and every one blocks a mind that could decode.
(b) Two gates count the engine's slots (2's physical semaphore and 5c), and dropping/re-taking between them costs the
waiter its queue position (5b). (c) A sync fsync (5h) runs on a runtime worker while all of it is held.
(`config.env` is not read per act on current canary; `current_serving()` is a watch clone.)

**The plan** (Codex takes the inference cuts, one measured change each: lane wait per act, engine slots busy, tok/s):
1. Take the lane and decode permit LAST: readiness, the per-slot permit, paging and the residency check run holding
   only the pin, the slot permit and the endpoint guard; the lane is held only while the engine generates.
2. One count of engine slots: 5c goes, 2 keeps the priority policy (directed reserve, work before ambient) with the
   count read from the engine.
3. One residency guard instead of three; the `/slots` probe beside every POST and the LoRA probe leave the act path.
4. The paging ledger's durable write moves to its owner task (off the async worker, never under held permits).
5. A relaunch wait holds no endpoint read guard (or listens for condemnation), so a drain never waits on it.
6. Revisit 2's lanes - 1 reserve with measured directed demand, never a fixed reserve that halves a 2-slot engine.

