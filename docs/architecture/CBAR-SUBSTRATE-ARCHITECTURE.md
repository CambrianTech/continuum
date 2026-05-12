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

Continuum should implement the same pattern in Rust:

```rust
pub trait RuntimeModule: Send + Sync {
    fn name(&self) -> &'static str;
    fn lane(&self) -> ResourceClass;
    fn target(&self) -> TargetSilicon;
    fn subscriptions(&self) -> &[ArtifactSelector];
    fn cadence(&self) -> CadencePolicy;
    fn handle(&self, frame: Arc<RuntimeFrame>, ctx: ModuleContext) -> ModuleResult;
}
```

`subscriptions` and dependency wakeups are deliberate Continuum upgrades beyond
CBAR, not a direct port. CBAR analyzers declare routing flags such as
`needsColorFrames`, `needsRealTime`, and `videoOnly`; then they pull artifacts
opportunistically from `CBAR_VideoFrame`. Continuum needs a richer contract
because N personas, RAG builders, model planners, memory jobs, and bridge
observers may all be waiting on different artifacts from the same turn. The
runtime must know those dependencies so it can wake only the useful work,
coalesce duplicates, and report deferrals.

The substrate provides:

- bounded per-lane queues
- dependency wakeups
- realtime versus delayed lanes
- newest-state coalescing
- resource admission
- GPU/model residency leases
- per-module logs and metrics
- flush/abort/shutdown
- trace events
- silence/deferred reasons
- automatic TDD/VDD evidence capture hooks
- fail-hard command errors
- ts-rs exported contracts

The module author provides:

- what artifacts it needs
- what resource lane it uses
- how often it should run
- the small piece of actual work

That is the "for free" architecture.

## Extension Bar

A new concern should normally be a few hundred lines, not a new subsystem. If a
persona recipe, model adapter, RAG source, media observer, render observer,
memory consolidator, or grid bridge needs to implement its own transport,
backpressure, retry loop, logging, queue, metrics, throttle, or lifecycle, the
substrate is missing a base capability.

The acceptance test for the runtime pattern is:

- New modules are small and focused.
- Communication is inherited from the runtime bus.
- Backpressure is inherited from the lane and pressure broker.
- Timing and performance metrics are automatic.
- Failure and deferred-state reporting are automatic.
- Resource leases and handles are standard.
- Cross-module consistency is enforced by common traits and generated types.
- No module grows into a monolith to compensate for missing substrate behavior.

This is the practical reason for the CBAR model. The architecture should make
the correct high-performance path the shortest path for every new class/module.

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
  `workers/continuum-core/src/paging/broker.rs`.
- `ServiceModule`, `ModuleRegistry`, `MessageBus`, `SharedCompute`, metrics,
  and logging under `workers/continuum-core/src/runtime/`.
- `ChannelQueue` and related persona queue consolidation primitives under the
  persona runtime.

The genuinely missing pieces are:

1. Define `RuntimeFrame` / `CognitionTurnFrame` on top of the existing
   `ResourceClass` + `TargetSilicon` + `ThroughputLease` + `PressureBroker`
   primitives.
2. Add formal artifact subscription, cadence, and dependency declarations to
   the module/job contracts. This can extend `ServiceModule` and existing
   planner jobs; it does not require discarding the runtime registry.
3. Move chat turn fanout onto `CognitionTurnFrame` so all personas share one
   room/RAG/model/prompt artifact set.
4. Attach VDD metrics to existing lanes/classes: queue depth, queue time,
   execution time, coalesced count, deferred count, GPU residency, CPU/GPU
   utilization, and first-response/all-response latency.
5. Add a Qwen GPU residency gate for local generation: selected Qwen model,
   backend, GPU layer count, unsupported layers, residency estimate, and
   platform backend evidence must be available before the turn runs. The
   required happy paths are Mac -> Metal, NVIDIA -> CUDA, and AMD/Intel ->
   Vulkan. CPU graph splits or unsupported Qwen layers are blockers unless the
   turn is explicitly degraded with a visible reason.
6. Migrate one expensive consumer at a time: persona chat, then embeddings,
   then memory consolidation, then media/WebRTC, then render/avatar output.

## Test Contract

CBAR-like runtime work is not accepted by browser smoke alone.

Required tests:

- Unit TDD for dependency wakeups, lane admission, cadence, and coalescing.
- Resource VDD for bounded queues, memory leases, and no monotonic growth.
- Performance VDD for first response, all responses, tok/s, and queue time.
- Residency VDD proving Metal/CUDA/Vulkan/local GPU path when required.
- Qwen VDD proving Qwen 3.5 text/code and Qwen2-VL vision use the expected
  local GPU backend, report layer residency, and fail loud on unsupported
  layers instead of silently running CPU-shaped inference.
- Accuracy VDD for replayed persona/RAG/tool output.

The alpha gate is not "it boots." The gate is that the runtime behaves like an
engine: predictable, concurrent, observable, fast, and small to extend.
