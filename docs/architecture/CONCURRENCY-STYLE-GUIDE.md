# Concurrency Style Guide

**Status:** canonical. Required first read before adding any monitor, broker, region, pool, pressure source, or other concurrent concern to the Continuum Rust substrate.

**Companion docs:**
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md) — the runtime contract every Rust module inherits (`ServiceModule`, `ModuleConfig`, `SharedCompute`, `MessageBus`).
- [`BRAIN-REGIONS-SUBSTRATE.md`](BRAIN-REGIONS-SUBSTRATE.md) — the cognitive-cycle trait (`BrainRegion`, `TickOutcome`, `PressureProfile`).
- [`RESOURCE-ARCHITECTURE.md`](RESOURCE-ARCHITECTURE.md) — `PagedResourcePool<K, V>` + `PressureBroker` arbitration.
- [`OBSERVABILITY-AS-SUBSTRATE.md`](OBSERVABILITY-AS-SUBSTRATE.md) — `probe!` / `time_sync!` / `JsonlProbeFileSink` capture pipeline.

This guide is the operational synthesis: how to write a new concurrent concern given that infrastructure. It exists because the amnesiac version of the model keeps reinventing what's already in tree.

---

## The model

> Continuum is an RTOS. Each concern owns its own task, its own tick, its own watch channel. Handlers never block. The substrate is mostly sleeping; work wakes work.

If your design has the hot path computing pressure, reading memory stats, probing disk, picking an LRU victim, or asking a network how it is — your design is wrong. Each of those is somebody else's tick, and the hot path reads a pre-staged snapshot.

This is `[[rtos-brain-no-region-on-hot-path]]` operationalized.

---

## The shape of every concurrent concern

A concern that watches state, runs on a cadence, or coordinates a resource is **always** structured the same way:

```
┌────────────────────────────────────────────────┐
│           OwnerType — singleton Arc            │
│                                                │
│   ┌──────────────┐    ┌────────────────────┐   │
│   │ Atomic gate  │    │ watch::Sender<Snap>│   │
│   │ (lock-free)  │    │  ↓                 │   │
│   └──────────────┘    │ watch::Receiver    │   │
│         ↑              └────────────────────┘   │
│         │ updated by    ↑ borrowed by readers  │
│         │               │ (never block writer) │
│   ┌─────┴───────────────┴────────────────┐    │
│   │  spawn'd tokio task — own interval   │    │
│   │  catch_unwind(AssertUnwindSafe(...)) │    │
│   │   loop:                              │    │
│   │     interval.tick().await            │    │
│   │     - read inputs                    │    │
│   │     - call reporters w/ spawn_blocking│   │
│   │       + 100ms timeout each           │    │
│   │     - publish PressureSnapshot       │    │
│   │     - update atomic gate             │    │
│   │     - quarantine bad reporters       │    │
│   └──────────────────────────────────────┘    │
└────────────────────────────────────────────────┘
```

The canonical implementation is `system_resources/memory_pressure.rs::MemoryPressureMonitor`. Read it before writing a new monitor. If your monitor doesn't have:

- own tokio task with `catch_unwind` wrapping the loop body
- `tokio::sync::watch` channel for snapshot publication
- atomic gate(s) for lock-free hot-path checks
- per-source calls via `spawn_blocking` + `tokio::time::timeout(100ms)`
- quarantine on N consecutive failures
- log_counter so quiet ticks don't spam

…it is missing load-bearing safety. Don't ship it.

---

## Picking the right primitive

```
Is your concern a cognitive subsystem (hippocampus, motor, attention)?
    → BrainRegion (runtime/brain_region.rs). Tick produces TickOutcome.
      Governor schedules it.

Is your concern a service with command surface + pub/sub?
    → ServiceModule (runtime/service_module.rs). Registers with
      ModuleRegistry. Declares command_prefixes + event_subscriptions
      + tick_interval + priority.

Is your concern a shareable resource (KV pages, LoRA adapters, embeddings,
experts, memories)?
    → PagedResourcePool<K, V> (paging/pool.rs). Register Arc'd pool with
      PressureBroker (paging/broker.rs). Implements ResourcePool trait;
      broker reads pressure + fires evict_at_least under tier policy.

Is your concern an exclusive stateful resource (mic capture, LiveKit peer,
Bevy avatar, audio mix)?
    → RAII lifecycle. NOT a pool. See LiveCallTracker for the pattern.

Is your concern a pressure-emitting subsystem (memory, GPU, disk, thermal)?
    → A monitor in system_resources/ (own task, watch channel, atomic gate)
      AND register as a ResourcePool with PressureBroker if it can act on
      its own pressure (evict, demote, refuse admission).
```

Never invent a parallel hierarchy. If your design has a "DiskGuard", a "ResourceWatcher", a "DaemonManager" sitting outside these — you are reinventing one of them. Pick the existing primitive.

---

## State distribution: `watch` is the default

Three channels, three jobs:

| Channel | Job |
|---|---|
| `tokio::sync::watch::Sender<Snapshot>` | One writer publishes the latest state. Many readers borrow lock-free. **Always-current**, no backlog. Default choice for monitor → consumer. |
| `tokio::sync::broadcast::Sender<Event>` | Multi-producer fanout of discrete events. Late subscribers don't see old events. Use for "thing happened at T". |
| `tokio::sync::mpsc::UnboundedSender<Cmd>` | Single-consumer command channel. Use for "here is a piece of work, do it." Reporter registration uses this in `MemoryPressureMonitor::add_reporter`. |

If you find yourself locking shared state through `Arc<Mutex<T>>` for cross-task reads — stop. The reader is blocking the writer's tick. Use `watch` and let consumers `.borrow()` the latest snapshot. Lock-free, contention-free, never starves the producer.

For lock-free per-thread reads of a single value (a gate, a level, a counter): `AtomicBool` / `AtomicU8` / `AtomicU64` with `Ordering::Relaxed`. See `MEMORY_GATE_CLOSED` and `CURRENT_PRESSURE_LEVEL` in `memory_pressure.rs` — both written by the monitor every 2s, read freely from anywhere with zero overhead.

---

## Cadence: pick from the existing ladder

Don't invent new tick intervals. The system already has a coherent set:

| Concern | Interval | Why |
|---|---|---|
| Memory pressure | 2 s | Fast enough to catch a spike before OOM, slow enough to be free. |
| Pressure broker tick | 5 s | Acts on pressure that other monitors are surfacing; doesn't need to sample faster than them. |
| Per-region tick | governed by `SubstrateGovernor` + `CadenceHint` | Region declares preference, governor decides. Never hardcode. |
| Background analyzer / consolidator | 30s–5min | Read engram drift, prune caches, write down what the noteworthy flag picked. |

Lean **slower** when in doubt. The cost of a missed-by-a-second pressure spike is one extra eviction cycle. The cost of polling at 100ms is a permanent CPU floor.

`tokio::time::interval(duration)` is the only cadence primitive you should use. Never `loop { sleep(d).await; ... }` — that drifts under load. `interval` ticks at fixed wall-clock and coalesces missed ticks.

---

## Reporter / source contract

Anything the monitor calls into to gather data is a **reporter**. Reporters are how regions, modules, pools, and subsystems contribute their slice of state to the monitor's snapshot.

The contract (from `MemoryReporter` in `memory_pressure.rs`):

1. `report()` MUST return in under 100 ms. The monitor's `tokio::time::timeout(100ms, ...)` will cancel it.
2. `report()` MUST NOT block on any lock the rest of the system holds during its own work. Reporters are called concurrently with everything else.
3. Reporters run inside `spawn_blocking` so synchronous CPU work or rare blocking I/O is allowed — but you should still be fast.
4. Panics are caught and counted. **Three consecutive failures → quarantined.** The monitor logs `🧠 X quarantined after 3 failures` and never calls it again.
5. `shed_load(level)` is fire-and-forget. The monitor wraps it in `catch_unwind`. It MUST be fast and non-blocking; if it has real work to do, post it to a channel and return.

If your subsystem can answer "how much of resource X am I using right now?" in under 100 ms, expose a reporter. If you can't, you have a deeper design problem — measure asynchronously into an atomic, and report the atomic.

---

## Pressure: surface the lever, never interpret it twice

The `PressureBroker` (`paging/broker.rs`) is the **single decision-maker** for what happens under cross-resource pressure. Resource owners surface their state; the broker decides.

A new resource that can evict, demote, or refuse admission:

1. Implement `ResourcePool` (`paging/pool.rs`). Defaults give you `pressure()`, `stats_snapshot()`. Override `evict_at_least(want_bytes) -> u64`.
2. At boot, `broker.register(Arc::new(your_pool))`. That's it.
3. Broker calls your `pressure()` on its 5 s tick. If global pressure crosses act-threshold, broker fires `evict_at_least` against the worst (High) or all over-budget pools (Critical). It also emits `PressureAlert` to every registered sink.
4. Do **NOT** also interpret memory pressure inside your subsystem. The broker reads `MemoryPressureMonitor`'s snapshot once and decides for everyone. Drift comes from N consumers each rolling their own threshold logic.

For a monitor that watches a *new* resource type (disk, thermal, network), the work is two halves:

- `system_resources/<resource>_pressure.rs` — own task + watch channel + atomic level, matching the `MemoryPressureMonitor` shape exactly.
- A `ResourcePool` registered with the broker that exposes the monitor's view as `usage_bytes` / `capacity_bytes` so the broker can arbitrate across resources uniformly.

Task #88 (disk pressure as substrate concern) is the live example of this two-half pattern.

---

## Observability is not optional

Every load-bearing decision emits a structured event.

- `tracing::info!(target = "module-name", ...)` — module-scoped narrative log. Goes to per-module logger.
- `probe!(class = "boot.status", ...)` / `time_sync!(class, ...)` / `time_async!(class, ...)` — substrate-canonical structured event. Goes to `JsonlProbeFileSink` + `ProbeRouterLayer` for class-based fanout.

**Critical:** `tracing::info!(target = "X.Y", ...)` does **NOT** reach the JSONL probe sink. The sink filters on the `probe_class` FIELD, not the target. If you write `tracing::info!(target: "boot.status", ...)` thinking you're emitting a probe, you are emitting an observability lie. PR #1550 review caught exactly this; don't reintroduce it.

The rule: if a downstream consumer (replay, sentinel, mechanic-grade dashboard) needs to read this event, it goes through `probe!`. If it's prose for an operator tailing a log, it goes through `tracing` / per-module logger.

Sprinkle probes at every meaningful seam. Name the surrounding variables in the probe payload. Wrap timing-critical blocks in `time_sync!` / `time_async!`. See `[[jtag-probes-are-rtos-debugger]]` — probes are how you debug an RTOS, since `info!` lines don't survive across N concurrent tokio tasks. Per `[[OBSERVABILITY-AS-SUBSTRATE]]`, ~half the substrate is structured capture — the differentiator between a complex guess and an intentional brain.

---

## What lives in code, not env vars

**Substrate operational policy is code, not configuration.**

The amnesiac model keeps adding env-tunable thresholds: `CONTINUUM_DISK_MIN_GB`, `CONTINUUM_MEMORY_WARN`, `CONTINUUM_TICK_INTERVAL`. **Stop.** Operators forget env vars between sessions; the code IS the policy. From `[[no-rust-gates-around-cognition]]` and the disk-guard slop intervention: substrate behavior must be predictable from reading the source.

| Policy | Where it lives |
|---|---|
| Pressure thresholds (Normal/Warning/High/Critical breakpoints) | `const PRESSURE_FLOOR = 0.80; const PRESSURE_CEILING = 0.95;` in the monitor file |
| Tick interval | `Duration::from_secs(N)` in the monitor's spawn site, not an env var |
| Quarantine count | `if entry.consecutive_panics >= 3` — literal, not configurable |
| Reporter timeout | `Duration::from_millis(100)` in the loop, not configurable |
| Broker act threshold | `BrokerConfig::act_above: 0.80` — set at construction, not from env |
| Disk pressure floor / ceiling | Will live as `const`s in `system_resources/disk_pressure.rs` when task #88 ships |

Env vars are reserved for **deployment shape** (socket paths, log roots, GPU device IDs) and **debug overrides** that have no production meaning. They are never the answer to "what threshold does this monitor fire at."

**The single-source exception (operator capacity policy).** One narrow class *is* legitimately operator-settable: **resource headroom / deployment-scale policy** — "how much of THIS box do we hand the substrate?" A dedicated deep-learning *foundry* runs its drive and VRAM to 1.0 (it's an appliance); a shared laptop leaves 20%. That's a real per-deployment fact, closer to *deployment shape* than to a firing threshold. It is allowed **only** when it funnels through the ONE typed config file (`config_env.rs`) — the key name, default, clamp, and read-once cache all live in **one place**, and consumers call a typed getter (`config_env::vram_headroom()`, `config_env::disk_headroom()`), never `config_env::read("KEY").unwrap_or(default)` re-derived per module. The sin forbidden-move #2 names is the **SCATTER** (161 sites each rolling their own key + default), not the existence of an override. One concern, one file everyone goes through, read-once, clamped, defaulted → coherent. Anything per-module → slop.

If you find yourself reaching for an env var to make tests pass, you're testing wrong. Inject the threshold through the constructor or `#[cfg(test)]` a const override. See `BrokerConfig` for the pattern: required fields, no `Option`, defaults via `impl Default`, tests construct with explicit values.

---

## The forbidden moves

Each of these is a recurring slop pattern the model reflex-codes under amnesia. Don't.

1. **Synchronous probing on the main thread.** `let used = sysinfo::System::new().refresh_memory()` in `main` or in a request handler. That's the monitor's job, on its own task, on its own interval. The handler reads the watch snapshot.

2. **Env-var-tuned substrate thresholds.** `std::env::var("CONTINUUM_FOO_BAR").unwrap_or("42")` scattered per-module. The code is the policy. Compiled-in `const`s only — **except** the single-source operator capacity policy carved out above (headroom fractions through `config_env.rs`'s typed getters, one file, read-once). The sin is the scatter, not the override.

3. **Sleep-loops where `interval` should be.** `loop { sleep(d).await; do_thing(); }` drifts; `tokio::time::interval(d)` doesn't. Use the latter.

4. **`tracing::info!(target = "class", ...)` as a probe.** It does not reach `JsonlProbeFileSink`. Use the `probe!` macro.

5. **Hot-path pressure interpretation.** "If memory > 90%, refuse." NO. Read the global atomic gate (`is_memory_gate_closed()`), or — better — read the latest `PressureSnapshot` from a `watch::Receiver` you subscribed to at construction. The MonitorTask already decided; you just check the verdict.

6. **Parallel allocators / managers / coordinators.** The system has `ModuleRegistry`, `PressureBroker`, `SubstrateGovernor`, `MemoryPressureMonitor`. If you're naming a new thing `XManager` or `YCoordinator`, you are almost certainly duplicating one of these. Find the right home.

7. **Locks across `await`.** `let g = mutex.lock(); ...; foo().await;` deadlocks and starves. Either drop the guard before the await, or move the state behind a `watch` / `RwLock` (parking_lot, scoped reads) / actor with mpsc.

8. **`unwrap()` / `expect()` on substrate startup.** A monitor that panics on `system.refresh_memory()` returning an unexpected value kills the runtime. Use `catch_unwind` around the task body (see `MemoryPressureMonitor::start`). Quarantine bad reporters, log the failure, keep ticking.

9. **`#[allow(dead_code)]` to silence warnings.** From CLAUDE.md's code-quality discipline: stop and understand. If it's truly dead, delete it. If it's a substrate hook for a not-yet-implemented consumer, mark it `#[cfg(test)]` or wire the consumer.

10. **Custom thread spawning (`std::thread::spawn`).** Use `tokio::spawn`. The runtime owns the executor; your concern is the work. Exception: blocking C FFI that won't yield — those go through `tokio::task::spawn_blocking`, never raw threads.

---

## The acceptance test for a new concern

Before you open the PR, walk this list:

- [ ] Owns its own tokio task. `tokio::spawn` somewhere visible.
- [ ] Loop body wrapped in `AssertUnwindSafe(...).catch_unwind().await` so a panic doesn't kill the runtime.
- [ ] Cadence via `tokio::time::interval`, not `loop { sleep }`.
- [ ] State published via `watch::Sender<Snapshot>`; consumers subscribe via `watch::Receiver`.
- [ ] No `Arc<Mutex<T>>` held across `await`. No lock contention on the hot path.
- [ ] Any external reporter call wrapped in `tokio::task::spawn_blocking` + `tokio::time::timeout(100ms)` + `std::panic::catch_unwind`.
- [ ] Failure mode: quarantine after N consecutive failures, log it, keep running.
- [ ] Thresholds and intervals are `const` in the module, not env vars.
- [ ] Probes at every meaningful seam: `probe!(class = "...", ...)`. Name the variables you'd want at a breakpoint.
- [ ] If it can evict / demote / refuse admission → registers with `PressureBroker` as a `ResourcePool`.
- [ ] If it's a cognitive subsystem → implements `BrainRegion` with declared `PressureProfile`.
- [ ] If it has commands / events → implements `ServiceModule` with declared prefixes and subscriptions.
- [ ] Lock-free atomic gate exposed for hot-path checks (if other code needs to ask "is X over budget right now?" cheaply).
- [ ] Tests construct via explicit config (`BrokerConfig`, `ModuleConfig`), not env, not magic globals.

If any box is unchecked, the substrate is going to teach you which one by failing under load. Save the trip.

---

## When the rule meets the exception

The amnesiac model loves to find an "exceptional case" that justifies the env var, the synchronous probe, the parallel manager. Some real exceptions exist; most don't.

**Real exceptions:**
- One-shot startup probes for *static* facts (CPU vendor, GPU vendor, OS version). Fine to run on main before spawning the runtime. They never need to re-run.
- Synchronous I/O for *configuration* loading (read `~/.continuum/config.env` once). Done before the runtime starts.
- `eprintln!` for fatal pre-boot errors when no logger exists yet. Substrate isn't up.

**Fake exceptions (don't fall for these):**
- "It's just a quick check." — No, it's a precedent. The next dev copies the pattern.
- "Only fires once per boot." — `MemoryPressureMonitor::start` is called once per boot. It still owns a task. So does yours.
- "I'll wire it to the broker later." — No you won't. Wire it now or don't ship it.
- "We need it env-tunable for ops." — Ops doesn't tune substrate policy. Ops tunes deployment shape (sockets, paths). Substrate policy is in code.

If you're not sure which side you're on, the answer is the boring one: implement it as a `ServiceModule` or `BrainRegion`, give it a tick, publish through `watch`. The "I'll do it the right way next time" debt always compounds.

---

## The Video Painter lineage — proven prior art for every rule here

The rules above are not invented; they are the patterns Joel shipped in **cb-mobile-sdk (CBAR / Video Painter)** — a realistic video-based mixed-reality SDK on 2011-era phones with *no competitors*, maintainable by one person. That was possible **because** of this discipline. Studied 2026-08-09 (`/Users/joel/Development/cb-mobile-sdk`, evidence `file:line` below). The lesson underneath all of it:

> **Never take upstream for granted.** Three modes of one courage: **contribute up** when the framework can be made right (Joel contributed edge-detection + optical-flow filters into GPUImage, in contact with its owner); **bypass at the native seam** when the sanctioned path is structurally incapable; **out-optimize the reference at the intrinsics floor** (hand-edited Qualcomm Hexagon/NEON and Apple Accelerate/NEON to push optical flow past OpenCV, on both ISAs). The industry default — accept the upstream abstraction as a fixed ceiling and pile working-around-it code on top — is *literally how a file becomes 4,700 lines*. Bypassing forces the small, correct decomposition, because you cannot marshal cleanly at the wrong layer.

**The bypass, concretely (the courage principle):** Video Painter refused Unity's sanctioned `WebCamTexture`/ARFoundation camera path — which delivers frames as *managed* textures via a per-frame GPU→CPU→GPU copy and gives you exactly one consumer. Instead it held the ARKit `CVPixelBuffer` itself, wrapped its luma/chroma planes as `id<MTLTexture>` with **zero copy** (`CVMetalTextureCacheCreateTextureFromImage`, `CambrianARNative.mm:727-748`), exported them as opaque `IntPtr` handles (`cambrian_GetVideoTextureHandles`, `:899-906`), and injected them into Unity's own render via a `CommandBuffer.Blit` at `CameraEvent.BeforeForwardOpaque` with `Texture2D.CreateExternalTexture` + per-frame `UpdateExternalTexture` — a pointer re-point, not an upload (`CambrianARVideo.cs:42-91`, the whole bypass in **127 lines**). The CPU-copy plane path exists but fires **only** `if (self.delegate.needsFrame)` — to feed the analysis pipeline, never for display (`CambrianARNative.mm:677-701`).

**The pattern language (each rule above, with its CBAR evidence and Continuum mapping):**

1. **One concurrency primitive, subclassed N times.** `QueueThread<T>` (own pthread + `CBCondition::timedWait` + bounded `std::queue` under one mutex, `Threads.h:109-212`) is the *only* thread pattern; `CBAR_VideoThread` and every `CBP_AnalyzerThread` inherit it. → Our `ServiceModule`/`BrainRegion`. Never a bespoke thread per concern.
2. **Backpressure by dropping stale, never blocking the producer.** Bounded queue evicts oldest, `queueSize=1` default (`Threads.h:134-138`); heavy analyzers take every 3rd frame (`CBP_Analyzer.cpp:96-100`). → latest-wins `watch::Sender<Snapshot>`. A slow lane drops ticks; it never stalls ingest.
3. **The frame is a lazy, memoized, ref-counted multi-representation cache.** `CBAR_VideoFrame` computes each view (`_yuvImage`…`_flowImage`, and the *quarter-res* `getOpticalFlowImage` pyramid) once, on demand, behind a mutex, passed as `cv::Ptr` (`CBAR_VideoFrame.cpp:106-264`). → **the persona *turn* is our frame**: one `Arc<Turn>` with `OnceCell` fields (raw prompt, embedded form, RAG context, tool-parse), each derived once, shared across cognition stages, never recomputed per consumer.
4. **Tight-vs-async is a *declared property of the work*, not an ad-hoc call-site choice.** Analyzers declare `needsRealTime()`/`videoOnly()`; the pose+optical-flow tracker is run inline every frame (never queued, never dropped) because everything else joins against it; all else is async (`CBP_FeatureTracker.hpp:33-35`, dispatch `CBP_Analyzer.cpp:88-100`). → a lane declares its latency class (reflexive/deep/sentinel) and the scheduler routes on it. No inline `if` deciding sync-vs-spawn.
5. **Causal join across async stages via a monotonic index + bounded ring.** `frameIndex` + `m_locationCache[idx % CACHE_SIZE]` lets any late stage ask "the camera pose at the frame this result came from," lock-free (`CBP_FeatureTracker.cpp:42-46`). → stamp every tick with a Lamport id; late async results (a tool return, a perception fact) carry the tick id they belong to. This is the mechanism behind "perception facts land *after* the decision."
6. **The copy-avoidance HIERARCHY, and compute-once.** It is not "zero-copy or copy" — it is a ranked ladder, and you take the cheapest rung the work allows (Joel, 2026-08-09): **(a) GPU-resident passthrough** — the data never leaves the device (a `UMat`/OpenCL matrix, a texture handle aliasing the same `IOSurface`); **(b) rasterize** into a target buffer if a representation change is unavoidable; **(c) copy — the absolute last resort**, and when you must, do it **once** and cache the result. cb-mobile: per-frame = `IntPtr` texture handle re-point (rung a); vector/texture passthroughs into `UMat`s live in the imaging layer (rung a); masks/meshes = one bounded `Marshal.Copy` at rung c (`CambrianARMaskIntegrator.cs:141-166`), never per-consumer. → the per-tick path passes *handles* (`Arc` over a tensor, the KV-cache pointer, a paged-genome handle); a representation is derived **once** into a memoized field and shared; only an engram-to-persist or a captured trace is serialized, and exactly once.

   **And a caution that raises the bar, not lowers it (Joel): "ours is way harder."** A video frame is a fixed-size, regular buffer on a metronome. A cognition tick is variable-size, irregular, and its "representations" (embeddings, RAG context, KV-cache, tool-parse) are far larger and costlier to recompute than a `pyrDown`. So the discipline here matters *more* than it did in Video Painter, not less — every accidental recompute or copy of a turn's derived state is the prefill-waste and window-double-pay we already measure (#266, #333). Do it once. Cache. Pass the handle.
7. **Fire-and-forget every mutation; keep only the frame-rate spine ordered.** State changes `std::thread(...).detach()` throughout `CBP_RenderingEngine.cpp`; only the per-frame render+track spine is synchronous. → commands/side-effects dispatch onto tokio tasks; the per-tick cognition spine stays the single ordered path (the Speak *settles* the turn; side-effects run off it).
8. **Anti-switch, discovered polymorphically.** Analyzers are a `vector<shared_ptr<CBP_AnalyzerThread>>` selected by `getAnalyzerOfType<T>()`, never `switch(name)` (`CBP_Analyzer.hpp:65-89`). → the same dynamic-discovery rule the command registry already enforces.
9. **Pimpl keeps public contracts tiny while impls grow.** Every class hides a `struct Impl`; the 960-line engine exposes a small header. → even a large module presents a thin trait/struct; the wire types (`#[derive(TS)]`) and public surface stay small.

**And the honest caveat (this is doctrine, not hagiography):** even Video Painter has two files over a 500-line bar — `ImageProcessing.cpp` (2,097, a static-method algorithm grab-bag) and `CBP_RenderingEngine.cpp` (960). The agent's verdict: the god-file is *the cautionary case, not the model* — it only survived because its header is a flat list and it's pimpl'd. Every other Cambrian-authored file is small (largest hand-written C# is 301; the C-ABI shim is 27 flat trampolines). The 50k–68k-line monsters in that tree are **all** Unity IL2CPP-generated vendor code, never hand-written. **File size is the observable proxy for architectural health** — and Continuum's 3–4.7k-line Rust files are the smell this lineage says to fix.

---

## Reading list (in order, for a new contributor to this surface)

1. `core/continuum-core/src/runtime/service_module.rs` — the base trait
2. `core/continuum-core/src/runtime/registry.rs` — how it gets wired
3. `core/continuum-core/src/runtime/message_bus.rs` — pub/sub primitive
4. `core/continuum-core/src/system_resources/memory_pressure.rs` — the canonical monitor pattern
5. `core/continuum-core/src/paging/pool.rs` — `PagedResourcePool<K, V>` + `ResourcePool` trait
6. `core/continuum-core/src/paging/broker.rs` — `PressureBroker` + arbitration
7. `core/continuum-core/src/runtime/brain_region.rs` — cognitive tick contract
8. `docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md` — the engineering philosophy
9. `docs/architecture/RESOURCE-ARCHITECTURE.md` — the cross-resource picture
10. `docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md` — capture as half the substrate
11. This document — the synthesis

After this, you know enough to add a new concurrent concern without reinventing what's already shipped. The substrate does the hard work; you write the small piece that's genuinely your concern.

---

## Provenance

Written 2026-06-08 after a session where the model — having lost cache — started rebuilding `runtime/disk_guard.rs` as a synchronous main-thread probe with env-tunable thresholds, after the substrate already had `MemoryPressureMonitor` + `PressureBroker` shipped for exactly this purpose. Joel: *"we wrote this before / hope it is in similar locations / only bringing this up because of recent slop / not telling you to delete, just think about long term intentional design."* And: *"yes we need the whole Rtos of cointinuum to be efficient NON blocking threads that are efficient and mostly sleeping like cbar."*

This guide is the disk-guard slop's tombstone. The next session that touches concurrency starts here.
