# CBAR Substrate Architecture

**Status**: architecture reference for Continuum's Rust runtime.

**Authoritative precedent**:
`/Users/joelteply/Development/cambrian/cb-mobile-sdk/cpp/cbar`

CBAR matters because of its engineering philosophy, not because Continuum
should copy every class literally. It is a small-code, high-throughput,
RTOS-style runtime where each concern gets threading, cadence, shared frame
artifacts, logging, lifecycle, and performance behavior almost for free.
Continuum needs that same shape for persona cognition, inference, memory,
WebRTC, Bevy/rendering, ORM/data, and grid work.

## Core Philosophy

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

## What CBAR Actually Does

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

## Continuum Translation

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
// is a Lane D deliverable; see "Substrate Gap Analysis" for assignment.
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

## The "For Free" Triplet

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

### Worked Example: A New Engram Analyzer

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

## Extension Bar

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

## Timing, Logging, And VDD For Free

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

### Standard VDD Record

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

### One-Line Instrumentation API

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

## Runtime Frame

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

## Resource Classes And Targets

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

## Handles, Leases, And No Bulk Copies

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

## RTOS Rules

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

## Domain Mapping

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

## Substrate Gap Analysis

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

The genuinely missing pieces, each cross-linked to its lane in
[ALPHA-GAP-ANALYSIS](../planning/ALPHA-GAP-ANALYSIS.md):

| # | Missing piece                                                                                                                                                                                                                                                                                                                                                                                            | Owning lane                                            |
|---|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------|
| 1 | `RuntimeFrame` / `CognitionTurnFrame` on top of the existing `ResourceClass` + `TargetSilicon` + `ThroughputLease` + `PressureBroker` primitives. Owns stable keys and lazy artifacts for one unit of work (chat trigger, room snapshot, RAG bundle, model selection, media handles, KV/LoRA leases, response envelopes, trace).                                                                          | Lane D                                                 |
| 2 | Typed artifact subscription, cadence, and dependency declarations on the module contract (`ArtifactSelector`, `CadencePolicy`). Extends `ServiceModule` to the proposed `RuntimeModule` trait shown above; does not discard the runtime registry.                                                                                                                                                        | Lane D                                                 |
| 3 | The "for free" triplet — `RuntimeModule` base trait, `#[derive(RuntimeModule)]` macro, and `just scaffold-module` generator — so a new concern is four lines plus a handler body (worked example in the previous section). Without (3), even after (1) and (2) land each module still hand-rolls the boilerplate, which is the same friction Lane D was created to remove.                               | Lane D (companion to #2; lands in the same PR series)  |
| 4 | Move chat turn fanout onto `CognitionTurnFrame` so all personas share one room/RAG/model/prompt artifact set instead of rebuilding it per persona per event. This is the consumer-side migration that proves (1)–(3) actually pay off.                                                                                                                                                                  | Lane D                                                 |
| 5 | Attach VDD metrics to existing lanes/classes: queue depth, queue time, execution time, coalesced count, deferred count, GPU residency, CPU/GPU utilization, and first-response/all-response latency, fed into the Standard VDD Record schema in this doc. The triplet's derive macro should be what emits these — the module author should not call `vdd_*!` macros by hand for the inherited fields. | Lane C (substrate); Lane D (frame integration)         |
| 6 | Qwen GPU residency gate for local generation: selected Qwen model, backend, GPU layer count, unsupported layers, residency estimate, and platform backend evidence must be available before the turn runs. Required happy paths: Mac → Metal, NVIDIA → CUDA, AMD/Intel → Vulkan. CPU graph splits or unsupported Qwen layers are blockers unless the turn is explicitly degraded with a visible reason. | Lane A (registry & admission); Lane E (admission gate) |
| 7 | Sequential consumer migration: persona chat → embeddings → memory consolidation → media/WebRTC → render/avatar output. Each consumer move is its own PR and must show VDD evidence that the post-move path is at least as fast as the pre-move path and emits the Standard VDD Record.                                                                                                                  | Lane D (sequencing); Lanes B/C/E (per-consumer support)|
| 8 | Pre-broker concurrency-hack deletion. Each module today that picks a worker count from `~/.continuum/config.env` or from system memory at startup (current concrete example: `core/inference-grpc/src/main.rs::get_num_workers()`) is a violation of the "we do not hard code" rule and must be deleted in favor of `PressureBroker` leases.                                                       | Lane E                                                 |

## Acceptance Criteria For Substrate-Done

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

## See Also

- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) — the
  artifact-sharing economy layered on top of this substrate contract.
  This document specifies what every cell inherits; that document
  specifies what every cell *recalls*, *composes*, and *evolves*
  through. The two are paired: the substrate is the floor, the genome
  economy is what runs on it. Lane H in ALPHA-GAP converges on the
  genome doc; Lanes C/D/E converge here.
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — the planning
  document. The Substrate Gap Analysis table above is the authoritative
  mapping between the eight numbered missing pieces here and the lane
  structure (A–H) there. If the two ever disagree on the substrate contract
  (concurrency, scheduling, memory, pressure, telemetry, artifact handles),
  this document wins per the precedence rule in ALPHA-GAP.
- `core/continuum-core/src/runtime/` — shipped substrate primitives
  this document refines and extends.
- `core/continuum-core/src/paging/broker.rs` — `PressureBroker`
  shipping point. The example in §"For Free Triplet" shows how a new module
  inherits pressure-response from the broker without owning a private hook.
