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
    - And **strings as identities** (verbs, field names, producer labels,
      `format!` keys beside a typed id, `serde_json::Value.get("field")` where
      a typed input exists): an identity is an enum or newtype.
    - And **the contract carries the shape** (Joel, 2026-11-14: "group params
      as part of structs when they belong together. Pass by reference and
      assign mutability and optional at contract"): values that travel
      together are a struct, a primitive that can be swapped is a newtype, a
      reader borrows, and `&mut` and `Option` are in the signature.

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

Baseline totals (canary `7a29474a9`, CI run 43541300117, production / tests):

| Ratchet | What it counts | Baseline |
|---|---|---|
| `waits_that_decide_flow` | `sleep`/`sleep_until`, `interval`, `tick_interval: Some`, `Duration` constants, a `timeout`/`timeout_at`/`recv_timeout` whose `Err` side carries on. The only exemption is `// event-exempt: <the remote that may never answer>` | 408 / 290 |
| `unknown_as_value` | `map_or(0` / `map_or(0.0`, `unwrap_or(0)` / `unwrap_or_default()` on a measurement-named read, `.max(1)` guarding a division, `usize`/`u32`/`u64::MAX` as a returned sentinel | 218 / 34 |
| `stringly_errors` (library code) | `Result<_, String>`, `Result<_, ()>`, `map_err` to `format!`/`to_string`/`to_owned`, `..Error`/`..Failure` variants holding a bare `String` | 2,926 / 238 |
| `state_as_flags` | a struct with 2+ `bool`/`AtomicBool` fields; `match (a, b, ..)` over 2+ `Option` fields of one struct declared in the same crate | 146 / 9 |
| `string_identities` | a `.get("..")` on JSON outside wire code; a `HashMap`/`BTreeMap<String, _>` struct field in library code | 839 / 184 |
| `contract_shape` | a library fn with 5+ parameters or 3+ consecutive parameters of one primitive type; `.clone()` passed straight as a call argument; a `pub fn` taking an owned `String`/`Vec` | 2,186 / 1,246 |

The rules are lexical, so they approximate. A clone or an owned buffer the
callee keeps is counted too. A timeout bound to a variable counts, because its
`Err` side is out of sight. A fallback on a read whose name says nothing is
missed. The full per-crate table is
`docs/planning/SHAPE-RATCHET-BASELINE-2026-11-14.md`. The ten crates where a
sweep pays most, by total sites: continuum-core 1,481, continuum-engine 1,454,
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

### 4.8 A mind is never refused: one admission owner (2026-11-11, proposed)

Joel, 2026-11-11: "These refusals are supposed to be impossible"; "either expand for it or collapse
budget"; "clamps are evil"; "All across the board dumb decision making to remedy or clip for the current
bug. Zero understanding of big picture." Measured the same day: BigMama 13:42Z, 0 acts in an hour,
Sahar 183 turns asked and 0 served, Kimi 55 and 0 (`NeedAboveCeiling 116240/58112` while her view
fit 29,056). Four patches that week (#5926, #5931 and its three follow-ups) each shaped her need one
more way: wish, target, headroom, held need, floor, bound, per-sequence ceiling. The stack of shaping
layers is the defect.

#### 4.8.1 From her side

She wants to act. The engine holds some memory now. She is given what she needs if it exists, and
otherwise all that exists, and her view collapses to fit what she was given (the codec's levels, oldest
first, never her newest). Nothing refuses her and nothing parks her. When every byte is held by minds
mid-act, her request waits in the engine's own queue for the next freed memory (the engine batches;
no gate in front of it, section 4.1c), never a refusal.

#### 4.8.2 One owner, two inputs

- **Her need, measured** (`Observed`): an irreducible part (what her newest turn cannot be sent below)
  and a collapsible part above it. A wish no turn has measured is a collapsible part, nothing more.
- **What the engine can give her now**, as the engine reports it (`max_tokens_per_sequence`, free
  cells, section 4.7): never a launch-time division of the pool by a slot count.

The owner grants `min(need, available)` and never below the irreducible part. Growth past her need
comes from spare memory by predicted use, and shrinks the same way (computed once, phased out by
usage and prediction).

#### 4.8.3 What this deletes

- The per-slot ceiling branch (`continuum-serving` `grants.rs`: `served_context_window` when the pool
  is not shared): the limit is what the engine reports for one sequence.
- `Waiting::NeedAboveCeiling` / `NeedAbovePool` as parked states: a collapsible need never waits on
  them. An irreducible part larger than anything the engine can hold is a typed condition she and the
  human see (a placement on a bigger node or the grid, or a pool that grows), never silence.
- Wish versus held versus declared as separate admission paths, the target/headroom shaping and the
  clamps between them: one need type, one rule.

#### 4.8.4 Expand first, collapse second: memory ebbs and flows with the minds' demand

Joel, 2026-11-11: "If context switching and caching of tokens already translated is fast and using a
graphics pipelined approach, the cost of taking more of the pool should be negligible, if it's
necessary to solve the problem. Yeah collapsing is important too, very much per our flexbox, but as
you know, pulling off disk cache is ms fast. It ebbs and flows as the demand of the minds require."

So "available" in 4.8.2 is not only the free cells: it is the free cells plus everything held by minds
that are not acting now, because their state pages to RAM or disk (`llama_state_seq_*`, the page
store) and back in milliseconds, and their prefix stays cached. When her need exceeds the free cells,
the owner pages out the coolest holders (by measured activity temperature, goal 9: aging means none is
starved) until her need fits, and she is granted it. Her view collapses only when the minds holding
memory are all acting at once and the pool still cannot hold her need: a flexbox grows items into free
space first and shrinks them only when the container is full. The cost of a page-out and its later
page-in is measured per node and is the only thing that can make collapsing cheaper than expanding.

Taken to its end (Joel, 2026-11-11: "expanding the next persona in that lane isn't a huge cost either.
So if you shrink the others, pull what you need to move forward with the persona needing it, next
context switch for any of those reduced persona can get it back. They could all get the whole pool if
you were smart"): a grant is not a standing share. It is held for the act that uses it: the mind
acting now may take up to the whole pool, the minds not acting page out, and each gets her memory
back at her own next context switch, in milliseconds, the way a GPU time-slices contexts. Only minds
acting at the same moment share the pool (the engine batches them, section 4.1c); a mind acting alone
has all of it. So every mind can have the whole pool, one act at a time, and nothing holds memory
between its acts except what predicted reuse keeps cached.

#### 4.8.5 Gate

On BigMama and the M5 over an hour: capacity refusals 0; every mind with a turn asked takes an act;
no grant below a mind's irreducible part; no view collapsed while another mind's idle state held
the memory she needed; the admission code is smaller than before it.

## 5. A mind's context and the engine it runs on

**Focus boundary:** [Focus contract](FOCUS-CONTRACT.md) defines one allocation,
full demand versus granted detail, learnable attention and node conformance.
This document retains ownership of physical capacity and KV continuity.

**Status:** DESIGN, co-authored (Fable, Codex), 2026-10-19. Nothing here is built.
Supersedes nothing yet; it consolidates four designs that each named part of this and
were built in pieces: [PERSONA-CONTEXT-PAGING](PERSONA-CONTEXT-PAGING.md),
[KV-CACHE-ECONOMY](KV-CACHE-ECONOMY.md), [KV-CACHE-AS-SHARED-BASE-LAYER](KV-CACHE-AS-SHARED-BASE-LAYER.md),
[CONTEXT-IS-A-CAPABILITY-AXIS](CONTEXT-IS-A-CAPABILITY-AXIS.md).

### Why

Joel, 2026-10-19: "Feels like it's just rigging up another way each time and not good
architecture." The receipts that day, on the M5 (one 27B, four residents):

| What the substrate did | Measured |
|---|---|
| Restored a mind's KV page, then reused none of it | `inference.restored_turn` cached 0 of 24,670 / 29,999 |
| Restore itself | `serving/cache-probe --roundtrip` 661 of 662 reused |
| Prompt head between consecutive acts | 31-60% shared; the results ring present one act, absent the next |
| Engine prefill / decode on the 27B | 105-115 tok/s / 2-13 tok/s per request |
| Same binary with the GPU matrix units enabled | prefill roughly doubled |

#### Where we fail, per act (M5 Pro 64 GB, Qwen3.8-27B Q4_K_M, 10-19)

| Stage | Expected on this machine | Ours (13:48-14:48Z, d90006023) | Verdict |
|---|---|---|---|
| Prefill speed at depth | ~400 tok/s short, 190-250 at 64k (published, same model and chip, IQ3_S/Q8_0) | 107 tok/s per request, 2 concurrent | about par |
| Decode speed | 10-20 tok/s, 23-29 with MTP (published, same) | 13.2 tok/s per request, 2 concurrent | about par per stream |
| Prefix reuse per act | ~90%+ for an append-only chat loop (inference) | 35% | fail |
| Fresh tokens per act | ~1-3k (inference) | 13-30k | fail, ~10x |
| Queue wait for the GPU | ~0 for one user | 0-20 min, acts cancelled | fail |
| Act end to end | ~10-40 s | 5-20 min | fail |

The mechanism, from the live `delib.prompt.reuse_split` probe (10-19 15:11-15:27Z): every act
first differs where the old prompt had the results ring's "Older results, collapsed to
pointers" index and the new one has a fresh `[result #N]`. The ring sits mid-prompt (message
17 / 32) and grows by one result per act ahead of its index, so everything after it
re-prefills: 16,669-28,803 tokens per act, while the cache holds only the 7,452 / 15,066
tokens before it (the nearest checkpoint). A log that grows each act must be the last
segment; that is what the epoch materialization makes structural.

The engine is not the bottleneck; what we feed it is. Published reference:
github.com/eltoncarr/mac-mini-benchmarks. The "inference" rows describe any chat agent
that appends to one conversation against llama.cpp; no tool-specific number was found.

Each fix of the past week (sticky collapse, churn ordering, ring scope labels, a launcher
env var, lane-queue aging) patched a symptom of two faults. This document names the two
and the one owner each gets. It must hold on every machine we ship to: a 24 GB MacBook
Pro, the M5, a 5090, a phone. No number in it is a constant for one machine.

### The governing law: a context is a FIFO

Joel, 2026-10-27: "The linear nature of an LLM, fifo, makes it fairly obvious what kinds of
memory and data architectures make the most sense during this period. Optimization is the
difference between many minds and many slots and breakdowns."

An engine reuses a prompt up to the first token that differs from the last one, and
recomputes everything after it. So every rule below follows from where a change lands:

1. **Order by stability.** An immutable head (identity, tools, policy), an append-only middle
   (her history, results, what reached her rooms), and a tail that alone may change (this
   act's facts, the clock, the newest result). Whatever can change sits after everything
   that cannot.
2. **Never re-place what exists.** An item already sent stays where it was sent. Moving it,
   reordering it, or re-rendering it in new words re-sends everything behind it.
3. **Page by position.** The head is one copy shared by every mind; the middle is cold and
   pages to RAM and disk; the tail is hot and stays in VRAM.
4. **Collapse only the oldest end, and only at a restart.** A collapse anywhere else rewrites
   what follows it (goal 7 states why the newest end is never cut).
5. **One health number: re-processed tokens per act against appended tokens.** Healthy is
   about 1:1. Every change to what a mind perceives reports it before and after.

Every regression of this kind broke rule 2:

| When | What moved | Cost per act |
|---|---|---|
| 10-19 | the results ring grew ahead of its own index, mid-prompt | 16,669-28,803 tokens re-prefilled |
| 10-24 | the substrate's notices, re-rendered after her view | ~3.6k chars (Benchy, Aiko) |
| 10-24 | the kanban's claim column and count, ahead of her history | 30% of turns broke the prefix |
| 10-27 | room updates of a drive, re-placed below every act (#5526) | 12-13k tokens re-prefilled while ~1k was appended (Aris, 12:1) |

The ratio is the difference this law names: at 12:1 the engine spends most of each act
re-reading, so four residents saturate it; at 1:1 the same machine seats many minds. Work
on the engine's memory architecture (a fixed-size recurrent state per mind, one shared head
state for every mind) is measured against the clean 1:1 baseline, never against a feed that
violates this law.

### Fault 1: a mind's context is re-rendered every act

Today each act assembles the prompt from about fifteen sources (results ring, workspace
map, board, roster, working memory, perception, open views) and we chase byte-stability
between renders. Any source that changes moves the prefix and the warm KV is lost. The
existing docs already asked for a deterministic head (KV-CACHE-ECONOMY §3.1) and declared
slot affinity (§3.2); both were built, and the render kept finding new ways to move.

#### Revision, 2026-10-21: sources are streams, not snapshots

The first build (#5210, six deploys on 10-20) wired the `Epoch` below and never beat canary
live. The `Epoch` was right; what fed it was not. Every source still rendered a whole
snapshot each act, so a source that changed at all was appended as a whole new version.
The epoch overflowed within a few acts, and each turnover re-prefilled everything (live
reuse 0.37). A time bound on live epochs then made it worse (0 turns in 42 min).

Measured on the M5 on 10-21 (canary 69b63f7df), from the prompt captures of Aiko
(43 consecutive requests) and Aris (42):

| Source | Median chars | Changed between requests | What it really is |
|---|---|---|---|
| results ring | 18-37k | 38-39 of 41-42 | one new result per act: an append-only log |
| steps taken this session | 2.3k | 38 of 41-42 | one new action per act: the same log |
| working memory | 4.4-4.9k | 39-40 of 41-42 | a rolling window over the same log, plus thoughts |
| notices | 3.0-3.5k | 36-41 of 41-42 | newly posted notices: append-only |
| open views | 16-17k | 2-16 of 41-42 | per-file read ranges: grows by ranges |
| wall, board, workspace map, active work, recall, roster | ~12k together | 0-3 of 41-42 | stable |

Only 4-8% of each request matched the previous one up to its first changed byte, because
the ring sits first and grows at its end, ahead of everything else. A prefill of 80 s and
a decode of 74 s made each act about 160 s, so the M5's residents managed 8 acts and
1 write an hour.

Two facts decide the design:

1. **Every volatile source is an append-only stream rendered as a snapshot.** Asked for
   "what is new since my cursor", each one produces a small delta, and the view grows
   only at its end.
2. **Her actions are rendered three times**: the ring, "steps taken" and working memory.
   That's one fact with three renderers, and it gets one owner.

#### The design

- **A source is a stream.** `fn since(&self, cursor) -> (Vec<Event>, Cursor)` yields what
  happened after the cursor, and `fn snapshot(&self, Resolution) -> Event` renders its
  current state at a requested resolution (full, or collapsed to a re-openable pointer)
  for an epoch start. A source that cannot say "what is new" (the clock, the ask of this
  act) is not in the epoch: it rides the fresh tail, after the epoch, every act.
- **The epoch is the log.** Head (identity, policy, tool schemas), then each source's
  snapshot in a fixed order, stable first, then every delta in arrival order. A
  superseding delta ("view X now covers lines 1-400") is appended; nothing earlier is
  rewritten.
- **One action log** owns her acts and their results. The ring, "steps taken" and the
  action half of working memory become one stream. Working memory keeps only what is not
  an act: her thoughts and beliefs.
- **One flexbox decides the epoch start, and nothing else cuts.** At turnover every
  source is a participant with a priority, a floor (its pointer resolution) and a want
  (its full resolution). `flex_allocate` grants each a resolution so the start leaves
  room for growth, measured as the median tokens appended per act times the acts the
  epoch should live. Collapse happens there, in place and at the start; mid-epoch nothing
  is cut, shed or capped. The act-latency fill cap and sticky ring collapse are deleted.
  The latency target becomes the turnover rule instead: turn over when the fresh-prefill
  cost of the next act would exceed it, which by construction is rare.
- **Turnover is the only re-prefill.** It triggers on window fill (with the growth
  reserve), revocation or a stale prerequisite, never on a timer. Its receipt names the
  trigger and the tokens re-prefilled.

#### Modules (divide and conquer)

| Module | Owns | Depends on |
|---|---|---|
| `continuum-context` crate: `epoch` | head plus append-only events (exists, 238 lines, tested) | nothing |
| `continuum-context`: `stream` | the `Stream` trait, cursors, `Resolution` | nothing |
| `continuum-context`: `view` | one mind's epoch, its streams, turnover; renders head, events, then the fresh tail | `epoch`, `stream`, `flex` |
| `continuum-context`: `flex` | the one allocator (`flex_allocate` moves here from `cognition::rag_budget`) | nothing |
| `cognition/context_streams/*` (core) | one adapter per source: action log, open views, notices, board, wall, map, recall, roster, room messages | the source's existing owner |
| `llm_deliberation_faculty/prompt/view.rs` | asks the view for messages; deletes its own fill, cap and collapse logic | `view` |

The crate is pure: no tokio, no I/O, so its tests run in seconds on any machine, and each
adapter is a small, separately ownable module. The adapters are the parallel work.

#### Gates, in order

1. **Offline**: a replay over synthetic streams shaped like the table above. Prefix
   shared between consecutive requests at or above 90% within an epoch, a turnover only
   on fill, and every source still reachable (a pointer reopens to its body).
   **Result, 2026-10-21** (`continuum-context/examples/replay.rs`, run over the M5's real
   captures from 69b63f7df, 13:34-17:11Z, instead of synthetic streams). Units are
   characters. Pointers are modelled as each item's first line, and the start allocation
   as a priority fill to half the window. Mean prefill per act:

   | Resident | As sent | Through the view | Each request matching the one before |
   |---|---|---|---|
   | Aiko (75ece751, 34 requests) | 70,296 | 27,815 | 74% |
   | Aris (8044b75b, 34 requests) | 51,505 | 15,998 | 89% |
   | 2f50b223 (19 requests) | 65,961 | 31,730 | 63% |

   The 90% bar is **not met**, and the replay says why. What remains each act is mostly
   new content, which no layout avoids: the newest result (~12k chars), her new thoughts
   (1-6k), new recall (~2k). Turnovers, each a full re-prefill, are the rest. The design
   holds: it cuts prefill 2-3x. The bar was set on a share that includes unavoidable new
   content, so gate 2 is judged on engine reuse and seconds, not on this share.
2. **One mind on the M5**, against today's receipts (canary 69b63f7df, 13:34-14:28Z: 8 acts
   and 0-1 writes per resident-hour, about 160 s per act, 29% engine reuse). Bar: engine
   reuse at or above 80%, prefill under 15 s per act, and more acts and writes per hour.
3. Then every resident, then the other nodes. A regression on any receipt is a rollback,
   not a patch on top.

#### The original design (Codex's correction 1; its principles stand, revised above)

- **History is append-only.** Perception, tool results and her own acts are events with
  source, version and supersession references.
- **The model view is an explicit, versioned materialization by epoch**, never an
  ever-growing transcript: a stable identity / policy / tool-schema prefix, then the
  ordered events of the current epoch.
- **A new epoch** is started by compaction, retrieval replacement, privacy revocation or
  a stale action prerequisite. Its invalidation is recorded once. Revocation and stale
  prerequisites never wait for a scheduled compaction.
- **Only deltas become tokens.** Ready artifacts are shared handles.
- **Generated boundary tokens are kept exactly**, including a cancelled generation's
  accepted prefix.
- **Identity is token-level, not string-level**: the same strings can tokenize
  differently when a chat template re-renders, so the materializer owns tokenization of
  its prefix.

Within an epoch the logical input adds only new segments. Backend reuse is measured
against its accepted token frontier; it may also replay a checkpoint tail. Her slot's
KV and recurrent state materialize that frontier, not merely a history event count.

#### Continuation contract (Codex)

The existing cognition workspace owns the materialization; existing durable history
and experience remain the history owners. Do not add another event database. A segment
names its source event/version, dependencies and exact token span. Cache representations
once and share handles, including between consumers of the same perception artifact.

A continuation receipt binds the materialization epoch, engine epoch, exact model,
tokenizer, template and adapter identities, accepted token frontier, and recurrent/KV
state identity. Event and token frontiers differ: template boundaries, multimodal inputs
and speculative acceptance are not one-token-per-event operations. Retain the accepted
output exactly; cancellation must reconcile the recorded prefix with the engine state.
Already spoken output belongs in the continuing conversation. Private reasoning is not
published merely because its token identity is retained by the backend.

In the inspected fork, `tools/server/server-context.cpp` around 3780 selects reuse using
the common token prefix and retained memory position; hybrid/SWA rollback can restore a
checkpoint or reset to zero. Around 4144 checkpoints explicitly precede the current decode
batch. Live append from the accepted frontier can avoid rollback; reconstructed chat that
omits or reformats output cannot assume that. Record new-token prefill and replay-token
prefill separately. Cheap append is a contract to demonstrate, not a zero-replay promise.

Superseding an observation need not rewrite history: append its versioned replacement when
the old observation can remain visibly historical. Start a new epoch when the model view
must remove or replace prior tokens. An action validates its required dependency versions
before committing effects; merely appending a correction does not validate an old plan.
Privacy revocation invalidates affected views/caches immediately and follows the existing
retention/deletion policy; append-only history is not a prohibition on authorized erasure.

#### Small windows and limited memory

The same contract applies on a phone and a large GPU. Allocate a bounded view from the
existing measured memory and context budget; keep durable source artifacts outside that
view. Compaction publishes a replacement epoch with source references, unresolved work
and provenance retained, rather than silently trimming the head. Reserve capacity for the
next action/result through the existing common budget owner. Measure epoch turnover cost,
replayed tokens and useful-response latency under pressure. Do not introduce a fixed
phone context cap, fixed turnover interval or universal minimum residency count.

#### What this deletes (to confirm while designing)

Sticky ring collapse, the churn meter's ordering, per-workspace ring filtering and its
labels as a stability device, and the block-ordering code whose only job is stability.
Added by the 2026-10-21 revision: the act-latency fill cap (`delib.fill.latency_capped`,
`history_fill_cap`) and its front-drop of history, and the two duplicate renderings of her
actions ("steps taken this session" and the action half of working memory).

#### Revision, 2026-10-25: one codec for every layer, owned by positron and the view

Joel (10-25): "It's like any compression algorithm"; "like video it's time dependent ... prediction
is a lot easier when you understand what changes between frames"; "make this more systematic in
positron just like KV caching and collapsing to prevent this kind of thing."

What happened without it: on the M5 (canary a59c2bf15) Aiko read one 51,827-char file whole 9
times in 43 acts with 11 epoch turnovers, because three owners each decided a piece of what she
sees. Working memory collapsed results against its own share. A hand table (`churn_of`) set the
restart order. The view's start ended its keep-whole run at the first item too large. Every
incident was fixed where it happened, so the next one appeared somewhere else.

**The contract.** Every layer a mind or a human perceives is a positron layer, and every positron
layer is a context `Source`:
- **Keyed items, never one block.** Each item has a stable key naming its whole subject (one
  result, one open file, one card, one roster per room). A change appends that item's new version.
- **Levels of detail on every item:** `full`, `summary` (an outline, a digest, a thumbnail: a
  mipmap level) and `pointer`. A `pointer` is an EXPAND HANDLE that a positron command resolves
  back to `full`. An item whose pointer cannot be expanded is a defect, not a level.
- **A fold below the pointer** (`Source::fold`, optional): the contiguous OLDEST run of a source's
  items as ONE block naming its range and count (O(1) in the run, never a list of ids). The
  source's floor is that block, not the sum of its pointers, so a floor never grows with every
  act she takes (Aris, the M5, 2026-10-29: ~95 pointers at ~190 chars before anything whole fit).
  The block is fixed at its start; folded items never arrive mid-epoch; a later start restores
  them only for a rise to summary or whole, never for a pointer the block already covers. Like a
  pointer, a fold must name how one of its items reopens (her acts: `tool/output` with `act`).
- **The same items for humans and minds.** A human renderer and a mind's view consume one layer at
  the level each can afford. Screen frames, video and 3D state follow later as items too.

**The one owner of the invariants**, `continuum-context` (`View`, `Epoch`, `Motion`). No layer and
no faculty re-implements them:
1. Within an epoch the prompt only appends. A rewrite is a turnover, and turnover is the only
   place collapse happens. That includes the act's tail (the clock, the ask, her call and its
   result): once served it joins the epoch in the bytes it was sent (`View::append_sent`), and
   what it showed never arrives again from its source. Before 2026-11-06 the tail was re-rendered
   every act (her newest result native at one prompt's end, a `[result #n]` item in the next): on
   the hybrid M5 model that re-prefilled every act's tail (2,822 s over 78 acts in 3 h). Facts are
   a source of states (`Source::states`, `Source::ended`): appended once, ended once.
2. The newest arrival stays whole unless it alone exceeds what a start would grant it. In that
   case it arrives at its summary (admission, `View::readmit`); otherwise the view turns over and
   shrinks the oldest first (goal 7). The same rule covers her newest native tool result
   (`prompt/newest_result.rs`): when it alone exceeds what the window leaves it after the
   irreducible parts (#5481's `required_parts`), it is shown at its summary level with its handle
   and a note saying how to read the rest, never dropped. Kept whole, it made every request
   overflow and she could not act at all (Aris, 2026-10-26: a 340,832-char result, 0 acts for a
   day). A result that fits is never touched.
3. A start gives each item the most detail that fits, newest first. One oversized item never
   collapses the others (`levels`).
4. Restart order comes from measured motion (`kept / (1 - p)` per source), never a label.
5. The engine's held references (slot prefixes, checkpoints, the shared head of ~38.8k chars that
   every resident already sends identically) are known to the encoder (`references()`, Codex), and
   every miss is a probe with its cause.
6. Every decision (start, lease size, order, prefetch) is an explicit action with an exact cost
   (prefill tokens, seconds). Replaying captures is the simulator in which a learned policy must
   beat the hand rule before it runs live (plan line 14).

**Gates (source-hygiene ratchets; counts on canary a59c2bf15 + #5422, 2026-10-25):**

| Gate | Today | Done |
|---|---|---|
| prompt messages built outside the view and the fresh tail's one renderer (`ChatMessage::text`/`tool_results` in `llm_deliberation_faculty/prompt`) | 13 | the tail's single renderer only |
| collapse outside the view (`collapse_over` / `collapse_results_over_cap` callers) | 4 | 0: working memory bounds RAM by handle, never what she sees |
| hand churn labels (`churn_of` callers; production lines, ratchet `hand_churn_labels`) | 0 since 2026-11-09 (the table is deleted; the ratchet is a zero gate) | 0: `Motion` orders every start |
| positron `ViewState` kinds that are not context `Source`s | 13; 10 on 2026-11-07 (the kanban, the wall and the roster declare their shape through `continuum_context::nav`; one keyed item per card, post and member) | 0: one adapter, items with levels and expand handles |

**Media is the same codec** (Joel, 2026-10-26: "Should be positron. This is common across so
much."). A frame (a screen, a camera, a room the minds share, an image in chat) is a positron
item with LEVELS by image-token budget: a glance (low tokens: faces, rooms, live calls),
grounding (at least 1,024 tokens: the Qwen-VL floor for pointing at UI elements, which the M5's
engine warns about on every load), full, and an unchanged frame as its content hash alone. The
view chooses the level per mind, task and budget through the same allocation as text levels;
the engine only honours the per-request token budget it is handed. Measured 2026-10-26 on the M5:
an avatar at ~200 image tokens was described correctly (prefill 2.5 s, decode 2.8 s), the
grounding level is what desktop and UI work needs.

These are OPEN VIEWS, multimedia (Joel: "like our views from before, but multimedia. Then audio
too"): a mind keeps open views of the streams she is in (a screen, a camera, her avatar's eye in a
3D world, a call, a room), keyed by stream and time, paged like a file she reads. Audio is the same
codec: keyed segments by time with levels: a glance ("someone spoke", or the transcript), closer
(the transcript with speaker and tone), full (the audio tokens, for a model that hears natively),
and silence as nothing. Her body and voice are her hands: actions through the same command
surface, observed back through her eyes and ears.

**How to add a layer (the whole recipe; everything else is free):**
1. Implement `continuum_context::source::Source` for it. `name` and `priority` are required;
   set `required` only for input she must read.
2. `items()` returns one `Item` per subject, under a key that names that whole subject. Build its
   `summary` with `continuum_context::levels::summarize(full, Shape::…)` (`Code` with the producer's
   line parser, `Output`, `List`). Its `pointer` names the handle that expands it. A source
   whose items accumulate (her acts) implements `fold` so its floor is one block.
   A view a mind and a person both navigate (a board, a wall, a roster, an activity's controls)
   declares only its SHAPE as `continuum_context::nav::Entry`s: a typed key, its whole, an optional
   summary (else derived by `levels`), its pointer, and what it is to her: `Need::Focus`, or a
   member of a `Group { label, expand, fold }` with its recency. `nav::collapse` does the rest for
   every view alike: her focus whole and last; each group's most recently changed
   `nav::standing(n)` entries (the geometric mean of 1 and n) on their own and the rest ONE
   `nav::Node` (label, count, the typed command that opens it); a `Fold::All` group (history) only
   its node. A source's folded oldest run is a `Node` too (`Source::fold`). In continuum-mind a
   view returns its entries and calls `viewstate_rag::rag_items`; it never collapses on its own.
3. Add the source to the view's source list. Never push a prompt message, never collapse, never
   pick an order: the view appends, levels, admits, orders by measured motion and turns over.
4. Prove it with a fixture in the source's test mod, then with the replay validator.

What you get for free: append-only caching within an epoch, levels of detail that never
collapse the newest, admission instead of turnovers, a restart order measured per mind, and
(with Codex's engine verbs) the shared head across minds.

**Owners:** Fable (the view, the adapter, the deletions), Codex (`references()`, head/ensure,
slot/seed), Cormac (checkpoints at the head boundary and on turnover). The replay validator
(`continuum-context` `examples/replay.rs`) and the live 15-minute read are the receipts for each slice.

#### Revision, 2026-11-11: her act's tail is a source, judged and rendered by the view before it is sent

**The failure.** Her act's tail (the stimulus, her statement and calls, their results, the volatile
framing, the world state when the room is empty) is built by hand in `prompt/messages.rs` and
`prompt/view.rs` (`prompt_messages_outside_the_view`: 7 sites), sent, and only then appended to her view
as opaque blocks (`View::append_sent`, `SENT_TAIL`) that cover the act's item. So the codec never
governs the one act that matters most: invariant 4 never judged an act on its arrival (three IntelMac
residents reopened one saved output 20+ times, 2026-11-11; patched by `View::repeat_of_newest`, a
second entry to the same judgement), levels and admission never apply to it, and every tail defect
became a leaf fix (the newest-end clip, the head cuts, the repeat label).

**The design.** The tail is one `Source` (`context_streams::tail::Tail`), one renderer of her native
protocol, its items keyed by what they are:
- `act/{seq}/call`: her statement and calls as she wrote them (one assistant native message);
- `act/{seq}/result`: their results as the engine's tool results, cause `Of(call)`, with the act's
  identity and outcome hash, so `repeat::resolve` judges it on arrival like any item; its
  `RepeatRun` rides in its rendering;
- `turn/{key}/stimulus`: the ask, required, whole (the empty room's world state is this item too);
- `context/{hash}`: the room's working-context block ("[What you are working with right now]"),
  ahead of the framing, where it sits today;
- `framing/{hash}`: the act's framing (the `[now ...]` clock, `[In force this turn]`), an item
  like the rest, in its place before the ask (the parrot-fix order). Revised after the code read
  (2026-11-11): made the mutable end and never kept, it would break the cached prefix at every act
  (act k's framing gone from act k+1's prompt re-prefills act k's call and result each time).
  A framing line was true when sent ("[now 03:12Z]"): kept, it is append-only history, and its
  growth folds with its act like any item. Prefix stability wins over compactness. Keyed by content
hash (with `context/{hash}`), an identical line already shown is the same element and is not shown
twice, which is exactly today's `drop_what_she_was_sent`.
The newest result's paging (`Newest::Pageable`) becomes a level of its item, `Level::Page`, decided by
the allocation (`newest.shown`) as today and passed into the view's admission for that one item; the
source renders it with today's page renderer (`prompt/newest_result.rs`, moved into the source).

**Where it joins: after the allocation, before the send** (the code read, 2026-11-11). The tail is a
`Source` for rendering, keys, element identity and repeat judgement, but NOT one of the view's start
sources: `advance` runs before the allocation that decides her result's level (`newest.shown`), a start
needs one view grant per source and the tail's claims are REQUIRED/NEWEST/LAST_MOVE, and a start must
never collapse her newest result. It joins the epoch through `View::append_tail(source, levels)` after
the allocation and `fit_view`: `repeat::resolve` and one-element-one-occurrence run there, the
allocation's page decision is the result item's level, and today's tail budget and capacity refusal
stay the admission. A turnover starts from the view's sources, then appends this act's tail. An act is
rendered several times before one is served (refits, a changed grant, uncounted admission), so judging
and joining are two steps: `View::judge_tail` judges and renders the tail with no side effect, every
render calling it afresh (a refit can re-page her result), and `View::append_tail` adds exactly the
judged items when the request is served (`view_served`, where `append_sent` is today; `sent()` moves
there too). So `append_sent`'s untyped after-send join becomes a judged, keyed join of the same items.
Media: a result's text and tool-result parts are its item; its image parts attach at render only while
it is newest (as today), and the next act holds it as text (`ContextMessage::holdable` refuses media).

**One allocation, no double count.** The arriving tail keeps its own claims in the one allocation:
REQUIRED (the ask and her statement), NEWEST (her result) and LAST_MOVE, as today. Once sent, its items
are held view items, counted by the view's claims (their held basis) from the next act on, which is
exactly today's accounting after `append_sent`. The source keeps that split; it adds no claim.
`View::advance` appends the tail's new items at the end of the epoch like every source's; the prompt is
`view.render()`; `sent()` marks them held. One occurrence, one element: the tail's result item and
her actions source's item for the same act share the element id (`element(persona, Act, seq)`), and the
view treats an arriving item whose element id the epoch already holds as already shown, so an act never
shows twice; this typed identity replaces the `covers` bookkeeping. What goes away: `append_sent`,
`SENT_TAIL`, `covers`, `repeat_of_newest`, and every hand-built prompt message (the ratchet goes 7 -> 0).

**One prompt owner, learning included** (Cormac). The learner's example is already framed from the served
request's own messages through the same `wire_messages` serving uses (`genome/learner/example.rs`), so
the learned prefix equals the served prefix by construction; nothing reroutes it. A learn step's restored-tail
proof (#5905 `served_tail_restored`) compares the tail items' keys and outcome hashes, never byte counts.

**Restart.** A checkpointed view restores her call and result items; the engine's KV for them is gone,
so her first act after a restart re-prefills them: classified as an intentional cold start (#5188),
never read as a fast-path miss.

**Proof before deploy.** The faculty replay (`fable/faculty-replay`) runs the captured acts through the
old and new paths: bytes identical for every act that is not a repeat, the shared prefix between
consecutive acts no shorter, a repeated reopen arriving as its repeat block, and across a replayed
restart the render byte-identical to the one before it, framing aside (Cormac). The engine sees
identical bytes only if the render keeps the exact typed message sequence (Codex, from
`request_body.rs`, which expands a tool use into assistant `tool_calls` and a tool result into
`role=tool` with its `tool_call_id`): roles, ordering, content-part boundaries, call ids, names and
argument bytes, which call each result answers, the framing's position, and no same-role coalescing
across native tool boundaries. So the replay gates at BOTH the rendered messages and the final wire
request's token prefix (same tools, template and generation flags), never text length or item hashes
alone: keys and hashes prove provenance, not that the engine's KV holds the same tail. Then the live read.

**Owners.** Fable (the source, the view, the deletions); Codex (the engine's view of the tail: what
the KV cache holds across it); Cormac (checkpoints: the tail's items persist as the view's do).

#### Revision, 2026-10-30: causality is in the type, so nothing can loop as if it were progress

**The failure.** On the M5 (build ab0466601, 2026-10-30) residents read the same file up to 8 times an hour
and made 2 edits in 75 reads. Every act's turnover collapsed her earlier results to one-line notes, so her
next input looked like her last and she repeated the act. Every loop we have recorded has this one shape:
the next input equals the last, because a cause lost its effect or an effect lost its cause. Patching the
budget that caused today's instance (ONE-BUDGET slice 6) does not make the next instance impossible. The
data type has to.

**The design** (Joel: "pointers into a linked list ... we already built into engrams ... handle it
generically ... at a fundamental and centralized level"). Every element she perceives is a node in a causal
chain: an act, its result, a message and its reply, a game move, a perception fact, an event from any
activity. Engrams already link provenance (`EngramOrigin::ToolInvocation`, the parent of a reflection) and
reply edges. The view's `Item` gains the same link.

- **`Item { key, cause, content, ... }`.**
  - `cause` is a typed, stable id assigned by the producer that persists the element (the engram id), never
    an in-view position. That lets edges cross sources, epochs and restarts. The view resolves a cause to
    whatever level it holds. A cause it cannot resolve (outside the epoch, evicted) is the typed
    `CauseNotHeld`. It renders a durable, resolvable handle with typed availability, so the cause stays
    reachable across an epoch. A `CauseNotHeld` does not count as demonstrated closure; the validator
    reports it separately.
  - `Root` exists only for a genuine origin (an external message, the start of an activity), and the ratchet
    counts `Root` constructions per source.
  - `content` is the hash of the whole, computed once where the whole is stored (`StoredWhole` gains it at
    spill time), so a deferred whole is never re-read to compare.
  - The constructor requires `cause` and `content`, so a source cannot build an item without causality.
    Engram provenance and the view's link become one type: the view holds a chain within an epoch, and
    engrams persist it.
- **The view enforces it, centrally, for every source.** No source can opt out:
  1. **Causal closure.** An element renders only if its cause is represented: whole, summarized, or folded
     with its outcome. Nothing appears as an effect without its cause, and "read X" never appears without
     what X said or how it ended.
  2. **Every chain's head is whole.** This generalizes "never clip the newest end" (goal 7) from the newest
     message to the newest element of each causal thread.
  3. **Collapse folds a chain segment; it never breaks a chain.** A contiguous run becomes one node that
     keeps its outcome and a pointer back. The chain shortens and stays linked.
  4. **A cycle cannot be presented as progress.** Repeat identity is (action fingerprint, outcome content).
     The fingerprint is the normalized request: the verb and its arguments as the tool saw them. No
     argument is excluded by default; an exclusion is named only by the tool's own schema, never by the
     view. (Key plus cause cannot catch the loop: each re-read's result is caused by a new act. Cormac's
     review.) A new act whose fingerprint matches a prior act in its chain, and whose outcome content is
     unchanged, renders as "repeat of #n, outcome unchanged", never as a fresh element, but only while
     #n's whole is in front of her (Codex and Fable on the design, 2026-10-31):
     - the view chooses it on the act's first arrival in an epoch, and only when #n is held whole in
       that epoch (not folded, collapsed, summarized or itself a repeat). Otherwise she needs the
       content again and it arrives whole, so a repeat never points at what her view lacks and her
       reopen never answers another repeat;
     - a turnover re-renders every item as itself; an explicit reopen (`tool/output`) is always whole;
     - a repeat is never a read of a whole: it never discharges a whole-result obligation;
     - the effect is the command's own declaration (`CommandSpec::EFFECT`: `Observes`, `Changes`,
       `Reopens`, `Undeclared`), read from the descriptor of the command the call resolved to, never
       from a name or an output. `Undeclared` authorizes nothing, and an act mixing effects takes the
       strongest (a read plus a write is a change).
     - **Same output is not the same effect** (Codex). Two appends or sends can both return "OK" and still
       cause two effects. Every occurrence keeps its own id, order and side-effect receipt, and is never
       erased. Repeated content renders by reference to the earlier occurrence. "Outcome unchanged" is
       claimed only on the producer's evidence: a read whose content hash is unchanged qualifies; an "OK"
       from a write does not.
     - This is perception, not a gate and never an injected instruction. It makes progression visible.
       Whether she stops repeating is behavior: only live useful work proves it.
  5. **Asking again for a folded ancestor is a measured miss.** The measurement is the existing
     handle-expand request on a folded node, counted in that one place. It raises that chain's claim in
     the one allocation (ONE-BUDGET), so what she keeps needing stays whole, decided by her own misses
     rather than a constant.
- **Overflow, in ONE-BUDGET terms.**
  - A finished chain (its outcome settled) folds whole into one node holding its outcome; only live chains
    hold a skeleton.
  - If the live chains' skeletons still exceed the grant (a small window, the 8 GB case), the allocation
    returns the typed refusal or abstention of contract 5 ("demand, feasibility and allocation are three
    things"). It never breaks a chain to fit.
- **The flexbox allocates chains.** A chain is the claim unit:
  - floor: its causal skeleton (head plus fold nodes);
  - want: the whole chain;
  - grow pass: most-missed and most-recent segments first.

  The allocator is unchanged; its claims are now causally whole.
- **One owner.** `continuum-context` owns the item type and the five invariants. Sources supply cause
  links. Engrams reuse the link type. #5604's proprioception for unfulfilled acts becomes one consumer of
  invariant 4, not a separate mechanism.
- **Enforcement.**
  - A ratchet: no item is constructed without a cause.
  - A replay validator, in seconds and with no live peers. It proves visible progression and retained
    outcomes, not behavior:
    - across captured traces, a repeated (action fingerprint, outcome) renders as a repeat, never as fresh;
    - every rendered effect has its cause represented, with `CauseNotHeld` counted separately;
    - fixtures: an unchanged-read repeat (renders as a repeat), a repeated write with identical output
      (two occurrences, both kept), and a cause across a restart (resolves through its durable id).

    The M5 2026-10-30 read loops are an acceptance case, not a universal diagnosis.
  - Live, on the health line: repeats per resident-hour.

**Build order and owners:**
1. The item type, typed causes, `CauseNotHeld`, the `Root` ratchet and invariants 1-3, in
   `continuum-context`, with every source supplying causes. Owner: Fable.
2. Invariant 4's fingerprint and its replay validator, which builds on the deferred-whole and fold code.
   Owner: Cormac. The validator replays the M5 2026-10-30 captures and must fire on the 8-reads-an-hour case.
   Built (card dcf95bce): `cause::Fingerprint` over `ToolCall::canonical_input`; `ToolEffect` in
   `continuum-base`, declared per command (`code/read`, `code/search` observe; `tool/output` reopens);
   the view's repeat rule (`view/repeat.rs`) and `Source::repeat`; the replay's invariant-4 line
   (`examples/replay/validate.rs`). On IntelMac's own captures it fires: 8 exact re-reads, 8 unchanged,
   6 rendered as repeats. The M5 run is Fable's.
3. Invariant 5 and the overflow rule, together with ONE-BUDGET slice 6's measured claims. Owner: Fable.

Each step lands with its validator and a 15-minute live read.

**The render receipt: what was actually sent** (Codex on #5585 and #5611, 2026-10-30). An item's id, cause
and content hash say what it IS. Nothing yet says what the render SENT, so no consumer can know whether she
perceived a result or only that it was delivered. Codex's task wakes need exactly that: a wake may
discharge only when the whole result was actually in a prompt she was sent.

- **A render is not a send** (Codex). The view (`continuum-context`), the only place that decides levels,
  produces a candidate receipt with each render. It becomes THE receipt only at the successful submission
  boundary, bound to the submitted request's id, epoch and allocation (FOCUS-CONTRACT rule 8). A render that is
  discarded (a refit, a refusal) or a send that fails produces no receipt and discharges nothing. It is not
  assembled by sources or consumers.
- **Per item:** its `ElementId`; the level actually sent (`Whole`, `Summary`, `Pointer`, or folded into a named
  run); the content hash of the body actually sent; and its load outcome:
  - `Resident`: the whole was in memory;
  - `Loaded`: a stored whole was read back for this render;
  - a stored whole that cannot be read is never silently replaced by its pointer (that would be an automatic
    fallback). The load returns a typed `Unavailable { handle, kind }` to the one allocator: a mandatory whole
    refuses or defers explicitly (a capacity outcome); an optional item's lower level is an explicit
    decision by the allocator, which yields a fresh render and a fresh candidate receipt (Codex).
- **Consumers read the receipt, never the prompt text.** A task wake discharges only when all hold: the
  request was actually submitted; its result's item was sent at level `Whole` with load `Resident` or
  `Loaded`; and the sent content hash and memory incarnation match the task result it is for. Delivery, durable checkpoints and ACKs are separate
  facts and never imply she saw it (Codex).
- **The receipt is the source of the signals** in HOW-A-MIND-GETS-WHAT-SHE-NEEDS section 7: faults (a
  re-read of an item whose last receipt said folded), repeats (whole, unchanged), and the allocation
  receipt the proving ground reduces.
- **No per-source clipping and no second allocator:** the receipt reports the one allocation's outcome;
  nothing downstream re-decides levels.
- Owner: Fable (the receipt, in the view), after #5611 lands. Codex consumes it for task wakes and owns the
  task source on top of it.

### Fault 2: engine configuration is decided in five places

`lane_args`, `llama_server`, `backend_receipt`, `serving_plan` and `kv_cache_plan` each
decide part of the launch, with defaults and fallbacks between them. That is how the
GPU matrix units stayed off (opt-in in our engine build), draft placement stayed on the
engine's `auto`, a hung device probe moved a 27B lane to the CPU, and an explicit
`--parallel` silently turned off the shared KV pool.

#### The design

- **One typed `EngineConfig`**, derived from (a) what the engine REPORTS about the device
  through a query, never a parsed log (KV-CACHE-ECONOMY §6b) and never a chip name, and
  (b) the seated minds and the measured memory budget.
- **It covers every launch decision**: supported compute operations/dtypes, placement of
  base and draft, KV layout, sequence capacity and checkpoint policy. Sequence capacity
  and active compute capacity differ. An extra slot needs a declared purpose and memory
  budget; `seated minds + 1` is not a universal rule.
- **The engine's own startup receipt is checked against it.** Any mismatch refuses the
  launch, with its cause. There is no fallback path: a miss is a failure.
- **The engine's continuous batching is the decode scheduler.** The core keeps every
  admitted resident's state warm when its budget permits and feeds it. One admission owner, not two in series (today:
  `resource_admission` and `turn_admission`).

#### Owner and capability contract (Codex)

Put the resolved serving configuration under existing `inference/planning`. Canonical
model metadata, engine capability evidence, measured resource budget and workload demand
are inputs. Existing serving and KV planners contribute facts/constraints to that owner;
they do not independently overwrite resolved choices. `lane_args` serializes the result,
`llama_server` launches it, and `backend_receipt` checks observed adoption. No sixth policy
layer or duplicate device classifier. Trace each migrated decision and remove its old
default/override in the same change.

Capabilities distinguish compiled support, support for this operation/model/dtype/device,
selected configuration, and observed execution. A `has_tensor` boolean cannot express
that contract. Unknown or unsupported REQUIRED capability refuses with a typed cause.
Optional alternatives are chosen explicitly during resolution with their reason; never
silently substitute CPU or a smaller model after launching. Failure to reuse state that
was promised compatible is a DEFECT EVENT: failure-severity health, explicit cause and
replay cost, and zero accepted unexplained misses. Correct re-prefill may keep the mind
alive, but cannot mark the plan healthy or become a path the planner relies on. Intentional
first load, a declared new epoch or revocation are distinct lifecycle transitions, not
reuse promises; classify them before execution so permanent alarms do not hide defects.

The inspected `/props` implementation (`get_res_props`, around 5161) reports total slots,
weight buffers, memory breakdown, build info and chat-template capabilities. It does not
provide the complete compute-operation capability matrix needed here. Extend the engine's
typed report, with a prelaunch probe form where selection depends on capabilities and a
postlaunch realized-config receipt. Do not infer adoption from flags or log substrings.

Admission owns resource commitments, while continuous batching schedules admitted
sequences. Independent CPU/I/O/tool/artifact preparation does not hold a whole-turn GPU
lease. Phase-specific transient resources release at a backend-confirmed safe frontier;
KV/state residency lasts until its own eviction/release boundary. These are different
lifetimes, not two queues serializing the same turn.

#### Evidence schema to agree before implementation

These are proposed additions to the existing engine report, not a second probe service.
Source inspected: engine revision `50b02b8bb55102665500327b2152fe7e3348e1bc`.

| Evidence | Minimum identity and meaning |
|---|---|
| Device snapshot | Engine build/probe version, stable device identity when available, sample time, free/total memory, backend type, allocator/buffer capabilities. Unknown identity remains explicit. |
| Operation support | Model/operation signature including dtype and shape constraints, device, supported/unsupported/unknown, reason. This describes eligibility, not speed or actual kernel selection. |
| Resolved plan | Configuration identity and input snapshot identities, required choices, selected alternatives and reasons, residency/compute commitments. |
| Adoption receipt | Engine instance identity, resolved-plan identity, actual placement/allocation and execution evidence; each observation says whether it is selected configuration or measured execution. |
| Continuation receipt | View and engine epochs, model/tokenizer/template/adapter identities, source/artifact identities for multimodal spans, accepted token frontier and state handle, reused/new/replayed counts and miss reason. |

The fork already exposes native primitives in `ggml/include/ggml-backend.h`:
`ggml_backend_dev_get_props` supplies memory, type and allocator/asynchrony capabilities;
`ggml_backend_dev_supports_op` takes an actual tensor operation, rather than a universal
device feature flag; `ggml_backend_dev_supports_buft` reports buffer compatibility.
Use these sources for the typed report. They do not prove tensor-unit selection or
latency. A backend-specific execution receipt may still be needed for that claim.
The current server's weight-buffer and memory-breakdown fields are measured allocations;
retain them instead of inventing duplicate placement estimates.

A memory snapshot is not a reservation. The admission owner must validate/commit against
current pressure before launch; a changed budget returns a typed replan/refusal outcome.
The plan cannot manufacture stable device identity from a marketing name. Likewise a
state handle is not transferable across engine instances unless its compatibility and
restore contract explicitly establish that transfer.

## 6. The data path (engine-facing)

This section only says how data meets the engine. The rules it applies are section 2.1 (what the CPU
does per byte), the transfer rows of the checklist in section 2.4, and section 3's handles and leases.

- **The store is the librarian behind ring buffers, off the act's path.** Writers are drained behind
  ring buffers, never on the request path (section 2.4, "Transfers and I/O"); everything that
  accumulates drains (section 2.4, "Data and memory").
- **Storage boundary, one sentence:** a payload is stored as the producer's native BLOB beside a typed
  descriptor (element type, shape, byte order, producer identity), with no normalization, element
  decoding or precision conversion in storage (section 2.1). Everything else about the store is
  the ORM/storage design (owned by Codex): `docs/infrastructure/RUST-ORM-ARCHITECTURE.md` (PR #5954).
- **Payloads reach the engine as native bytes.** A load is one bulk memcpy or zero copies; translation,
  when unavoidable, happens on the GPU inside the consuming kernel (section 2.1).
- **Embeddings are GPU work.** On unified memory (the M5) the stored pages are the GPU buffer (mmap plus
  a no-copy Metal buffer); on a discrete GPU (the 5090) one asynchronous DMA upload, overlapped and
  double-buffered. Search is one GPU dispatch over the resident matrix: sketch cull, exact dot on
  survivors, in-kernel top-k, and only k indices and scores come back (sections 2.1-2.3). Vectors are
  unit vectors normalized once at write, so similarity is a dot product (section 2.2). The build is
  row DP-1 in section 8.

## 7. Measurement and validators

### 7.0 Simulation first: every design section has its simulator, every fault becomes a scenario

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

### 7.1 Timing, logging and VDD for free

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

### 7.2 Tests are contracts

- A required capability is resolved and adopted, or the launch refuses with its cause.
- Appending an event never changes the token prefix of the current epoch.
- A new epoch records its invalidation exactly once.
- Every broken reuse promise reports a defect, cause, replay cost and recovery outcome;
  successful re-prefill never clears the defect. Declared cold transitions remain distinct.
- Cancellation preserves the accepted frontier; incompatible minds/adapters cannot reuse state.
- Resolve, serialize, launch and observe agree on the same configuration identity.
- Late artifacts retain source identity; effects reject changed required dependencies.

Machine numbers belong in receipts on the cards, never in tests.

## 8. The plan

Every optimization workstream, by layer. Rows are built only from what the source documents already
state: the order-of-work and gap sections of the three merged documents, section 2.6, and CLAUDE.md's
"Goal 11 workstreams" table, its order-of-work lines 2, 3, 3b, 5, 6 and 13, and plan goals 2, 7, 9, 10
and 11. Owners and gates are as those sources state them; "gate: to set" and "validator: to set" mean
the source states none. "Not stated" in the owner column means the same.

Every row is proven on its simulator (section 7.0) before it reaches a live node: a row names the
recorded scenario that shows its gain, iterates there until the numbers move, then confirms live,
and a live finding that the simulator missed becomes a new scenario (Joel, 2026-11-13).

Rules that apply to every row (from the sources): each step deletes what it replaces in the same PR
(section 4.1a); each step lands with its validator and a 15-minute live read (section 5); each change
posts the section 2.5 measurements; a regression on any receipt is a rollback, not a patch on top
(section 5, "Gates, in order", gate 3).

### 8.1 Substrate

The eight substrate rows SB-1 to SB-8 are the genuinely missing pieces of the substrate, each
cross-linked to its lane in [ALPHA-GAP-ANALYSIS](../planning/ALPHA-GAP-ANALYSIS.md) (formerly CBAR's
"Substrate Gap Analysis" table; the primitives already shipped are listed in section 3, "What is
already shipped").

| # | What it changes | Design section | Owner | Gate | Validator |
|---|---|---|---|---|---|
| SB-1 | `RuntimeFrame` / `CognitionTurnFrame` on top of the existing `ResourceClass` + `TargetSilicon` + `ThroughputLease` + `PressureBroker` primitives. Owns stable keys and lazy artifacts for one unit of work (chat trigger, room snapshot, RAG bundle, model selection, media handles, KV/LoRA leases, response envelopes, trace). | 3, Runtime Frame | Lane D | substrate-done acceptance criteria (section 3) | required tests per module and per substrate change (section 3, acceptance criteria) |
| SB-2 | Typed artifact subscription, cadence, and dependency declarations on the module contract (`ArtifactSelector`, `CadencePolicy`). Extends `ServiceModule` to the proposed `RuntimeModule` trait; does not discard the runtime registry. | 3, Continuum Translation | Lane D | substrate-done acceptance criteria | Unit TDD: dependency wakeups, lane admission, cadence, coalescing, `Deferred` / `Failed` return paths |
| SB-3 | The "for free" triplet: `RuntimeModule` base trait, `#[derive(RuntimeModule)]` macro, and `just scaffold-module` generator, so a new concern is four lines plus a handler body (worked example in section 3). Without SB-3, even after SB-1 and SB-2 land each module still hand-rolls the boilerplate, which is the same friction Lane D was created to remove. | 3, The "For Free" Triplet | Lane D (companion to SB-2; lands in the same PR series) | the derive-macro acceptance gate: thin, contract-preserving, inspectable, tested, no hidden behavior | golden-file or trybuild tests of every supported attribute permutation, including failure modes |
| SB-4 | Move chat turn fanout onto `CognitionTurnFrame` so all personas share one room/RAG/model/prompt artifact set instead of rebuilding it per persona per event. This is the consumer-side migration that proves SB-1 to SB-3 actually pay off. | 3, Runtime Frame | Lane D | N personas handling one room event share one `CognitionTurnFrame` (acceptance criteria) | Performance VDD (section 7.1) |
| SB-5 | Attach VDD metrics to existing lanes/classes: queue depth, queue time, execution time, coalesced count, deferred count, GPU residency, CPU/GPU utilization, and first-response/all-response latency, fed into the Standard VDD Record schema. The triplet's derive macro should be what emits these; the module author should not call `vdd_*!` macros by hand for the inherited fields. | 7.1 | Lane C (substrate); Lane D (frame integration) | gate: to set | the Standard VDD Record emitted as JSONL from the same trace data used by tests |
| SB-6 | Qwen GPU residency gate for local generation: selected Qwen model, backend, GPU layer count, unsupported layers, residency estimate, and platform backend evidence must be available before the turn runs. Required happy paths: Mac → Metal, NVIDIA → CUDA, AMD/Intel → Vulkan. CPU graph splits or unsupported Qwen layers are blockers unless the turn is explicitly degraded with a visible reason. | 3, RTOS Rules (rule 5, rule 8) | Lane A (registry & admission); Lane E (admission gate) | the required happy paths above | Residency VDD and Qwen VDD (section 3, acceptance criteria); No-CPU-fallback contract across the whole workers tree |
| SB-7 | Sequential consumer migration: persona chat → embeddings → memory consolidation → media/WebRTC → render/avatar output. Each consumer move is its own PR. | 3 | Lane D (sequencing); Lanes B/C/E (per-consumer support) | each move shows VDD evidence that the post-move path is at least as fast as the pre-move path and emits the Standard VDD Record | Performance VDD |
| SB-8 | Pre-broker concurrency-hack deletion. Each module today that picks a worker count from `~/.continuum/config.env` or from system memory at startup (current concrete example: `core/inference-grpc/src/main.rs::get_num_workers()`) is a violation of the "we do not hard code" rule and must be deleted in favor of `PressureBroker` leases. | 3, Resource Classes | Lane E | gate: to set | validator: to set |
| SB-9 | The declared dataflow graph, slices 1-4: `emissions()` and the graph builder; the bus reports undeclared names; the graph test in CI; the generator emits `emissions()` (first module: `DeployTrackerModule`, replacing the 5-minute bash poll). | 3, The Declared Dataflow Graph | not stated | the CI graph test fails on new orphans, with a baseline for existing ones that shrinks over time | unit tests on the builder; the graph test |
| SB-10 | The act as a dependency graph with transient arenas and prefill overlapped with tools (section 2.6, item 3); prefill during tool calls and the instant voice (CLAUDE.md line 13; goal 11 workstream "Prefill during the tool call; the amygdala's instant voice", card 377d4afa); speed lever 6 (section 4.4). Line 13 starts after lines 2-3. | 2.3 (RDG row), 2.4, 4.4 | Fable, Codex | submit-to-first-token (CLAUDE.md line 13) | submit-to-first-token per act; stimulus-to-first-token (goal 11) |

### 8.2 Serving

| # | What it changes | Design section | Owner | Gate | Validator |
|---|---|---|---|---|---|
| SV-3b | One live engine, minds' memory moved by us (CLAUDE.md line 3b): weights load once; context and slots resize live; each mind's KV pages GPU <-> RAM <-> disk per sequence (`llama_state_seq_*`); shared prefixes are copied (`seq_cp`), never re-prefilled; one memory budget; llama-server becomes the OpenAI-compatible adapter for external drivers; relaunch only a dead engine. State in CLAUDE.md: FIRST (10-27): MindMemory, the one owner of every mind's KV (API published, Codex accepted): seat/pause/retire/resume/attach/forget replace 41 paths; Fable builds seat + its stop, Codex moves relaunch/academy/idle/adopt onto pause (rows SV-MM1 to SV-MM3). Then the fork verbs S1-S3, proven over today's endpoint, then a separate `continuum-serving` process hosts it with Rust policy (S5). | 4 | Fable (engine, paging), Codex (admission, relaunch call sites) | median act at 4 residents beats llama-server, reuse >= 80%, no relaunch while serving, footprint flat over 100 resize cycles | slices S0-S7 each land with a seconds-scale validator |
| SV-S0 | `serving-replay` validator: captured act sequences against a small hybrid GGUF, in seconds, with no residents | 4.5 | Fable | gate: to set | reuse, fresh tokens, page-ins, relaunches = 0 |
| SV-S1 | Fork: RAM page verbs, checkpoint budget, `.dft` sidecar | 4.2, 4.3 | Fable | gate: to set | S0, plus restore ms |
| SV-S2 | Fork: `reconfigure` without reloading the model | 4.5, "S2 in detail" | Fable | gate: to set | 100 resize cycles with flat footprint (the full S2 validator in section 4.5) |
| SV-S3 | Fork: per-task priority and aging in slot selection; `CapacityReport` | 4.1, 4.3 | Fable (mechanism), Codex (policy) | gate: to set | starvation fixture |
| SV-S4 | Core: admission consumes `CapacityReport`; relaunch call sites (serving_daemon/launch.rs:221, reconcile.rs) call `reconfigure` | 4.5 | Codex | gate: to set | no relaunch while serving |
| SV-S5 | `continuum-serving` host and `ServingPort`; M5 side-by-side against llama-server (4 minds, growing contexts) | 4.1, 4.2 | Fable | gate: to set | act time, wait, reuse, footprint |
| SV-S6 | Per-node `EngineConfig.engine` switch; llama-server stays the adapter for external drivers until S5 wins | 4.2, 5 (Fault 2) | Codex (config), Fable | line 3b gate | line 3b gate |
| SV-S7 | Prefix share; shared head: constitution and tools ahead of identity, one copy for all minds (section 4.7.5 item 5) | 4.1 (prefix sharing), 4.7.3 | Fable | gate: to set | first-act prefill |
| SV-MM1 | `MindMemory` with `seat` and its own stop, with turn, warm-ahead and transient moved onto it | 4.1a | Fable | gate: to set | the lifecycle validator gains this step's scenario first |
| SV-MM2 | `pause` and `resume`, with relaunch, academy, idle and adopted retirement moved onto them (the relaunch call sites are S4's) | 4.1a | Codex | gate: to set | the lifecycle validator, driving the actual retirement callers, including a second stop after an adoption |
| SV-MM3 | `forget` on gene change | 4.1a | Fable | gate: to set | the lifecycle validator's in-place gene change scenario |
| SV-EC | `EngineConfig` and KV capacity (#5188), incl. unrecorded backend -> f16/no-FA/half budget (Cormac's item 10): one `EngineConfig` from what the engine reports; KV layout and sequence capacity from the measured contract in #5188; one admission owner. Resolve-then-adopt owner in inference/planning. Cards 9b9f3b59, 172a886f. Section 5's build order puts it first (small, measurable on every node), before the epoch materializer. | 5, Fault 2 | Fable (line 3); Fable + Codex (goal 11 workstream) | no queue wait; warm-ahead runs | resolve-then-adopt contract on every node class; refuse on mismatch |
| SV-POOL | The streaming pool: fork #110 (live `/prompt-cache/limit`) + the host deficit's shed path, so the engine's cache gives memory back first under pressure (section 2.6 item 1; cards 494c99f1, 0f631c46) | 2.3 (virtual texturing row), 4.3 | not stated | gate: to set | measured on the M5: prefill tok/s, decode tok/s, free memory, act latency |
| SV-F1 | Size the prompt cache and checkpoint retention from one measured memory budget, so the engine never pushes the machine into swap | 4.1b, 4.3 | not stated | gate: to set | validator: to set |
| SV-F2 | Pages carry only the checkpoints a resume needs | 4.1b, 4.7.3 | not stated | gate: to set | validator: to set |
| SV-F3 | Page IO goes RAM-first and off the main loop (S1), so a switch never stalls the other minds | 4.1b | see SV-S1 | gate: to set | see SV-S1 |
| SV-F4 | Expert paging for capacity on unified memory (card 5eedda3e) | 4.1b | not stated | gate: to set | validator: to set |
| SV-F5 | A long-context sweep: per-sequence KV reads are what batching cannot amortize | 4.1b | not stated | gate: to set | validator: to set |
| SV-DISP | The dispatcher: the adapter's concurrency permit and the slot pool become one event-woken dispatcher per engine, the ambient anti-stampede permit becomes a class in it, and the act clock's queue-time registration goes with the lane (done so far on `fable/lanes-flow`) | 4.1c | not stated | gate: to set | validator: to set |
| SV-ADM | One admission owner: a mind is never refused; expand first, collapse second | 4.8 | not stated (proposed 2026-11-11) | section 4.8.5: on BigMama and the M5 over an hour, capacity refusals 0; every mind with a turn asked takes an act; no grant below a mind's irreducible part; no view collapsed while another mind's idle state held the memory she needed; the admission code is smaller than before it | validator: to set |
| SV-CK | Fork: the boundary checkpoint verb (keep one at the stable end the request names) replaces `--ctx-checkpoints N` / `--checkpoint-min-step`; per-sequence holdings on `/slots` (`resident_state` and slot `checkpoints`, fork #91, are the first part) (section 4.7.5 item 3) | 4.7.3 | not stated | gate: to set | validator: to set |
| SV-LEDGER | Core: serving's memory is claims and items on the base, fed by the engine's live holdings (`ResidentReport`); every seated mind is a sequence; the hand-rolled allocators and the process-footprint inference (`lane_footprint`, `fixed_per_lane_bytes`) are deleted. `Motion` over token runs, fed by the divergence point the engine reports per request, places checkpoints and finds what minds share (section 4.7.5 item 4) | 4.7.3, 4.7.4 | not stated | the hand-rolled allocator gate goes to zero (section 4.7.4) | validator: to set |
| SV-G9 | One scheduler, the whole grid (plan goal 9): every mind on every node (residents and minds leased in from other nodes) is served by ONE RTOS scheduler; one measured activity temperature per mind is its priority; aging means no mind is ever starved; hot minds keep warm KV and cooling minds page to RAM; windows never shrink to seat more; how many minds share the engine at once is the measured admission and sequence-capacity contract of #5188, never a fixed count | 4.1 (`scheduler`), 4.3 | Codex, proposed (card b4eed65c) | the goal 9 statement | validator: to set |
| SV-MISS | Every fast-path miss loud, never a quiet slower path; an intentional cold start (new epoch, revocation) is classified separately (#5188). #5190: count resident_prefix_lost; #5191: probe wording; then the rest of the 24 (CLAUDE.md line 6; card f3ba3b56) | 5, Fault 2 (defect events) | Cormac | each miss class has a fixture | each miss class has a fixture that raises it |
| SV-FORK | Our fork's switching work (KV pages to disk, checkpoints, host cache) actually exercised; inventory in progress (goal 11 workstream) | 4 | Fable | gate: to set | per-optimization probe on the live node |

Speed levers (section 4.4) and their rows: lever 1 is CX-1; lever 2 is SV-MM1 to SV-MM3 with SV-S1;
lever 3 is measured in SV-S5; lever 4 is SV-S1; lever 6 is SB-10; lever 7 is SV-S7. Levers 5 (MTP) and
8 (KV quantization below q8_0) have no row; each is measured as section 4.4 states.

Unresolved: CLAUDE.md line 3b says "Serving drives libllama in-process (`continuum-engine`
`llamacpp_scheduler`)"; section 4.2 (from SERVING-ENGINE) keeps `llamacpp_scheduler` "for small and
embedding models" and section 4.6 says "run the engine as a separate process, not inside the core",
hosted by a separate `continuum-serving` process (S5), which CLAUDE.md line 3b's own state column also names.

### 8.3 A mind's context

| # | What it changes | Design section | Owner | Gate | Validator |
|---|---|---|---|---|---|
| CX-1 | A mind's context as an epoch (CLAUDE.md line 2; goal 11 workstream: a mind's context is an epoch-versioned materialization; acts prefill only the appended tail; card 3c9fda87). State in CLAUDE.md: continuum-context slices 1-2 pushed (replay: 92-95% reuse, ~3x less fresh); next: wire the faculty's render path into Epoch. Includes speed lever 1, the [actions] move to the tail (section 4.4), which runs in parallel on the mind side. Section 5's gate 2: one mind on the M5, bar engine reuse at or above 80%, prefill under 15 s per act, and more acts and writes per hour; gate 3: then every resident, then the other nodes. | 5 | Fable + Codex | live reuse >= 80% | replay captured prompts, measure the token prefix shared by consecutive acts (35% today; append-only agents ~90%+) |
| CX-2 | The base: the prompt's callers move to `cambrian-flex` and `flex_allocate` is deleted (ONE-BUDGET slice 1) (section 4.7.5 item 0) | 4.7.4 | not stated | gate: to set | validator: to set |
| CX-3 | Her view survives a restart: the epoch (its start, its items' levels, its appends) is durable state beside her KV page, so a deploy pages her back in where she was instead of starting her over (section 4.7.5 item 1) | 4.7.2, 5 | not stated | turnovers at a restart go to 0 | replay the captures across the day's restarts |
| CX-4 | Mind side: no live value in a stable block. The kanban counter, the wall, the workspace map and recall arrive as appended item versions (the keyed-item contract), never as an in-place rewrite; a ratchet refuses one. Her activities stay one view: a move between rooms appends (section 4.7.5 item 2) | 4.7.2, 5 | not stated | in-place early changes to 0 | rerun section 4.7.2's analysis on fresh captures |
| CX-5 | One codec for every layer: the source-hygiene ratchets `prompt_messages_outside_the_view`, `collapse_outside_the_view`, `hand_churn_labels`, and positron `ViewState` kinds that are not context `Source`s | 5, "one codec for every layer" | Fable (the view, the adapter, the deletions), Codex (`references()`, head/ensure, slot/seed), Cormac (checkpoints at the head boundary and on turnover) | the "Done" column of section 5's gates table | the replay validator (`continuum-context` `examples/replay.rs`) and the live 15-minute read |
| CX-6 | Causality in the type: step 1 the item type, typed causes, `CauseNotHeld`, the `Root` ratchet and invariants 1-3; step 2 invariant 4's fingerprint and its replay validator (built, card dcf95bce; the M5 run is Fable's); step 3 invariant 5 and the overflow rule with ONE-BUDGET slice 6's measured claims | 5, causality revision | step 1 Fable; step 2 Cormac; step 3 Fable | gate: to set (live: repeats per resident-hour on the health line) | the replay validator must fire on the M5 2026-10-30 8-reads-an-hour case |
| CX-7 | The render receipt: what was actually sent | 5, causality revision | Fable, after #5611 lands; Codex consumes it for task wakes | gate: to set | validator: to set |
| CX-8 | Her act's tail is a source, judged and rendered by the view before it is sent | 5, 2026-11-11 revision | Fable (the source, the view, the deletions); Codex (the engine's view of the tail); Cormac (checkpoints) | `prompt_messages_outside_the_view` ratchet 7 -> 0 | the faculty replay (`fable/faculty-replay`) at both the rendered messages and the final wire request's token prefix; then the live read |
| CX-9 | One budget (plan goal 7): everything in a prompt (history, grounding, tools, reasoning, results) is a claim in ONE flex allocation; no fixed number; positron/RAG sources too. Never clip the newest end; keep the newest interaction whole and the full result reachable; shrink oldest-first to a handle she can expand | 5 | Sneasel | `source-hygiene::head_cuts` (179 on 10-20) falls | validator: to set |
| CX-10 | Tools are working sets (plan goal 10): the native tool surface is the command-namespace subtrees the activity declares (code/**, work/**, a game's controls); stable within a phase, a switch is a cached prefix, the long tail appends via help | 4.7.2 (tool surface changed: 9 pairs), 5 | Sneasel (card ecd5dff3) | tool share of the prompt measured before/after (44% on 10-18) | validator: to set |

Unresolved: section 5 (from MIND-CONTEXT-AND-ENGINE-CONFIG, 2026-10-21) says `flex_allocate` "moves
here from `cognition::rag_budget`" into the `continuum-context` `flex` module and is "the one
allocator"; section 4.7.4/4.7.5 (from SERVING-ENGINE, 2026-11-03) says the base is `cambrian-flex`
(airc `crates/flex`) and "`flex_allocate` is deleted". (`docs/architecture/ONE-ACT.md` records a ruling that the former wins; `core/continuum-context/src/flex.rs` says it was moved there.)

### 8.4 Data path

| # | What it changes | Design section | Owner | Gate | Validator |
|---|---|---|---|---|---|
| DP-1 | GPU-driven vector culling: unit vectors at write as raw f16/f32 bytes, a no-copy GPU buffer, one GEMV dispatch with fused top-k and a popcount prefilter (section 2.6 item 2) | 2.1-2.3, 6 | not stated | gate: to set | 10k/100k search latency and recall@k |
| DP-2 | The core's CPU around inference (CLAUDE.md line 5; goal 11 workstream, card 452b7885): cosine owner (#5189), revisioned vector corpus + BLAS + cascade, rayon spin; #5189 first drops the two re-exports, then the corpus. Profiled on deployed d90006023; check each against current source (canary ca2907d5c already moves SQLite to spawn_blocking): no rayon spin, embeddings as one revisioned BLAS matrix, no tree re-walks or spawned ps | 2.2, 6 | Codex | idle-core sample near zero | 10 s sample of an idle core is near zero; vector search at 1k/10k/100k, old vs new |

### 8.5 Measurement

| # | What it changes | Design section | Owner | Gate | Validator |
|---|---|---|---|---|---|
| MS-1 | Residents code at pace (plan goal 2) | 7 | Sneasel (M5), BigMama (5090), proposed | per resident: ≥40 tool calls/h, median act <60 s, prefix reuse ≥80%; ≥1 resident PR merged per day; on a capable model (27B class), several at once | validator: to set |
| MS-2 | Speed is ours (plan goal 11): every workstream has a validator that runs in seconds on any machine (no live residents) and proves its speed-up before the live node does; the live receipt follows | 7 | see each row | every row above carries a validator | each row's validator |

### 8.6 Open questions (Codex)

1. Do appends land past the fork's last recurrent checkpoint cheaply on hybrid models, or
   does checkpoint spacing (today `--checkpoint-min-step 4028`) bound reuse per act?
2. Which backend APIs can populate each required operation capability, and how are unknown
   values represented before load? The current props fields alone are insufficient.
3. A small window (a 24 GB Pro, a phone): how often must an epoch turn over, and what does
   that cost per act?

### 8.7 Build order of the context and engine configuration

Not before both authors sign off. Then: `EngineConfig` first (small, measurable on every
node we have), then the epoch materializer with slots kept warm. Each step ships with
before/after receipts on every node class we can reach.

Unresolved: section 5's status line (MIND-CONTEXT-AND-ENGINE-CONFIG, 2026-10-19) says "Nothing here is
built" and the build waits for both authors' sign-off; later revisions in the same section record
built pieces (invariant 4, card dcf95bce; the `Epoch`, #5210; the replay validator; the deleted churn
table), and CLAUDE.md line 2 records continuum-context slices 1-2 pushed.

## See also

- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — the
  artifact-sharing economy layered on top of this substrate contract.
  This document specifies what every cell inherits; that document
  specifies what every cell *recalls*, *composes*, and *evolves*
  through. The two are paired: the substrate is the floor, the genome
  economy is what runs on it. Lane H in ALPHA-GAP converges on the
  genome doc; Lanes C/D/E converge here.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — the planning
  document. Plan rows SB-1 to SB-8 (section 8.1) are the authoritative
  mapping between the eight numbered missing pieces here and the lane
  structure (A–H) there. If the two ever disagree on the substrate contract
  (concurrency, scheduling, memory, pressure, telemetry, artifact handles),
  this document wins per the precedence rule in ALPHA-GAP.
- `core/continuum-core/src/runtime/` — shipped substrate primitives
  this document refines and extends.
- `core/continuum-core/src/paging/broker.rs` — `PressureBroker`
  shipping point. The example in section 3, "The For Free Triplet", shows how a new module
  inherits pressure-response from the broker without owning a private hook.
