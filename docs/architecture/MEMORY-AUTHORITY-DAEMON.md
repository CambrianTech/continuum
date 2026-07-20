# Memory Authority Daemon — one location, continuously, as a concurrent daemon

**Status:** design + build plan (2026-07-20). Precedence-winning on "who decides how much VRAM/context each lane gets, and when."

**Required companions (read first):**
- [`CONCURRENCY-STYLE-GUIDE.md`](CONCURRENCY-STYLE-GUIDE.md) — the RTOS shape every concurrent concern MUST take. This doc is an application of it.
- [`CBAR-SUBSTRATE-ARCHITECTURE.md`](CBAR-SUBSTRATE-ARCHITECTURE.md), [`GENOME-FOUNDRY-SENTINEL.md`](GENOME-FOUNDRY-SENTINEL.md), [`INFERENCE-LANES-REALISTIC.md`](INFERENCE-LANES-REALISTIC.md).
- Memory: `[[memory-system-is-fully-dynamic-nothing-static]]`.

---

## The two non-negotiables (Joel, restated as invariants)

1. **Memory is a live signal, never an "at launch" decision.** Available memory — our lanes/KV/adapters/eval/train AND the world outside (a game the user just quit or launched, another app) — changes every second. The served window, lane count, and KV are a **continuous function of live available memory**, tracked up AND down, forever. There is no spawn-time constant, no "YaRN at launch," no "grow a deep lane on demand" as a discrete one-shot.

2. **Memory is managed from ONE singular location.** One authority owns ALL memory accounting AND ALL memory policy. It is a **concurrent daemon**: its own task, its own tick, its own `watch` channel. On each tick it turns live-available into an allocation for every consumer and **publishes** it. Consumers do NOT compute their own footprint or sample memory — they `.borrow()` the pre-staged allocation snapshot and reconcile to it.

The style guide says it directly: *"If your design has the hot path computing pressure, reading memory stats, probing disk, picking an LRU victim… your design is wrong. Each of those is somebody else's tick, and the hot path reads a pre-staged snapshot."*

---

## The violation we are removing

Today the serving/eval hot paths COMPUTE the memory decision inline:

- `serving_daemon::host_budget()` reads `system.snapshot().memory.available_bytes` + the governed VRAM ceiling **on the serving tick**, then `plan_serving()` runs the window/lane fixpoint **on the serving tick**.
- `serving_daemon::reconcile_to_plan()` owns the resize policy inline (the `2×-starved` relaunch hysteresis + `sticky_served_window`).
- `cognition/eval::plan_eval_lane_ctx()` is a **second, duplicate** budget calc that reads raw GPU free and runs `plan_serving` again for the eval lane.

Three consumer-side deciders, each sampling memory on its own hot path. That is the banned shape, and it is why "the window is decided at launch" keeps recurring: the decision isn't owned by a daemon that watches the live signal — it's recomputed ad hoc by whoever is spawning a lane.

Accounting is also still split (the `[[memory-system-is-fully-dynamic-nothing-static]]` audit): the `resources/` `ResourceDaemon` board (the intended authority), `gpu/memory_manager.rs` (a second VRAM ledger), the `PressureBroker`, and a separate `governor/` module. Converging accounting is the sibling track; THIS doc converges the **decision**.

---

## Target architecture

We add **zero new tokio tasks and zero new modules.** We are an RTOS: the daemon shape repeats everywhere, and CBAR hands it to you for free — the **base trait + derive macro + scaffold generator** triplet (`ServiceModule`/`RuntimeModule`, `#[derive(RuntimeModule)]`, `just scaffold-module`). A module *declares* `tick_interval` + prefixes + subscriptions and the substrate spawns its task, drives its tick, wires its `watch` pub/sub, defers it under pressure, and gives it a VDD span + replay fixture. **You never hand-roll `tokio::spawn` + `interval` + `watch::channel` + `catch_unwind` — that's the hard way.**

And here we don't even need to scaffold a new module: the two daemons already exist and are **already ticked by the substrate**:
- `ResourceDaemon` (`resources/daemon.rs`) already runs a tick and already publishes `watch::Sender<LeaseBoard>` (`subscribe()`), and serving already registers with it (`add_consumer`, `TierDownPolicy`). It is THE memory authority. We give it the one job it's missing — **compute + publish the allocation on the tick it already has** — and make consumers subscribe.
- `serving_daemon` is already a `ServiceModule` with its own declared tick. It becomes a subscriber that reconciles to the published allocation on the tick it already has.

We do NOT invent a parallel hierarchy (style guide: no `AllocationManager`/`MemoryGovernor` outside the existing authority).

```
          live memory signal (MemoryPressureMonitor watch snapshot,
          GpuCapacitySource scan) — changes constantly
                        │
                        ▼  (borrowed on the authority's EXISTING tick — never on a consumer's)
   ┌─────────────────────────────────────────────────────────────┐
   │  ResourceDaemon — THE memory authority                      │
   │  (already a substrate-run daemon; we add to its tick)       │
   │                                                             │
   │   on the tick it already has:                              │
   │     available = ceiling − Σ(measured consumers)  ← board    │
   │     alloc     = AllocationPolicy(available, demand, catalog)│ ← plan_serving math, injected as policy
   │     publish watch::Sender<LeaseBoard>        (accounting)   │
   │     publish watch::Sender<Allocation>        (decision) ◄── NEW field, same tick, same watch idiom
   └─────────────────────────────────────────────────────────────┘
             │ borrow (lock-free)              │ borrow
             ▼                                 ▼
   serving_daemon (existing ServiceModule) eval / training
   reconcile lane to alloc.serving         read alloc for a lane of footprint F
   (NO host_budget, NO plan_serving)        (NO plan_eval_lane_ctx)
```

If a genuinely new concern is ever needed, it is born via `just scaffold-module` + `#[derive(RuntimeModule)]` — declared, not hand-spawned.

**Key shapes:**

- `Allocation` (published snapshot): the authority's current decision. For serving: `{ model_id, lanes, per_lane_window }` — exactly the `ServingPlan` the fixpoint produces, but **owned and published by the authority**, recomputed every authority tick from the live signal. Grows when a game quits (memory monitor watch updates → next authority tick recomputes → republishes → serving borrows the bigger window and reconciles). Shrinks the same way.
- `AllocationPolicy` (injected fn, swappable): the sizing math (`plan_serving` wrapped to close over the live catalog + demand + suppress/pin). Registered by serving at boot, the same way `TierDownPolicy` already is. The **authority owns the tick and the publish**; the **policy is pluggable** (and eventually AI-driven — `[[grid-agreements-swappable-policy-deterministic-rails]]`, Joel: "strategy and policy, even AI control, in one place"). Consumers never call it; the authority does, on its tick.
- Consumers are **subscribers**: `serving_daemon` holds a `watch::Receiver<Allocation>` and its tick reconciles the llama-server to the published serving allocation. `eval` asks the authority for a lane allocation of footprint F from the same published live board (replacing `plan_eval_lane_ctx`), and `acquire_guarded`s the lease (G1) as it already does.

**Cadence:** the authority ticks on the existing ladder (the `ResourceDaemon`'s current interval). Memory pressure is surfaced at 2 s by `MemoryPressureMonitor`; the authority does not sample faster than that. Lean slower, per the guide. A `watch` snapshot is always-current, so serving reconciling on its own 5 s tick reads the freshest allocation with no coupling.

---

## Build slices (VDD-gated; each compiles + tests + deploys before the next)

**Slice 1 — the authority publishes the allocation; serving subscribes (behavior-preserving).**
- `ResourceDaemon` gains `watch::Sender<Allocation>` + `allocation()` accessor, and an injected `AllocationPolicy` (registered by serving, closing over catalog/demand/suppress/pin). On its tick, after publishing the board, it invokes the policy with the live board `available` and publishes the resulting `Allocation`.
- `serving_daemon::reconcile_to_plan` reads `resource_daemon.allocation()` instead of calling `host_budget()` + `plan_serving()`. **Same numbers** (the policy IS `plan_serving`), just computed on the authority's tick and borrowed — not on serving's tick. Delete `ServingDaemonModule::host_budget`/`plan_next` as consumer-side computers; their bodies become the injected policy.
- Gate: live personas keep serving; the published `Allocation` matches what serving used to compute; window still grows/shrinks with memory.

**Slice 2 — eval reads its allocation from the authority; delete `plan_eval_lane_ctx`.**
- `eval` asks the authority for a one-shot lane allocation of footprint F against the live board (the same `available` the serving policy sees), then `acquire_guarded`s (G1 unchanged). Delete the duplicate `plan_eval_lane_ctx` budget calc.
- Gate: webdev/humaneval eval lanes still size to the model's full window when VRAM affords; no duplicate memory read.

**Slice 3 — the resize policy (2×-starved / sticky) moves into the authority.**
- The "when to relaunch to grow/shrink" hysteresis is part of the allocation the authority publishes (it decides the target window with dwell/hysteresis), so serving's tick is a pure "reconcile to published target." Delete the inline `2×`/`sticky_served_window` from serving.
- Gate: no thrash (`[[never-thrash-sticky-hysteresis-on-every-lane]]`); a game-quit grows the lane within one authority+serving tick.

**Slice 4 — converge the accounting planes.**
- Fold `gpu/memory_manager.rs` + the separate `governor/` module into the `resources/` authority so `available` is one number from one ledger (the `[[memory-system-is-fully-dynamic-nothing-static]]` audit's endgame). Then delete the remaining static fractions: `budget = available` directly.

---

## Forbidden moves (the reflexes to catch)

- **A consumer sampling memory on its own tick.** `system.snapshot().memory` / `gpu::monitor::detect()` inside `serving_daemon`/`eval`/anywhere that isn't the authority's tick. The consumer borrows the published allocation; it never reads memory.
- **A shared `compute_budget()` two callers pull from.** Deduplicating the wrong shape is still the wrong shape. The decision is OWNED by the authority's tick and PUBLISHED; it is not a function consumers invoke.
- **A new "AllocationManager" / "MemoryGovernor" sitting outside `ResourceDaemon`.** The authority already exists and already ticks + publishes; extend it. (Style guide: never invent a parallel hierarchy.)
- **Hand-rolling the daemon (`tokio::spawn` + `interval` + `watch::channel` + `catch_unwind` by hand).** We are an RTOS; the daemon shape is FREE via the triplet — `ServiceModule`/`RuntimeModule` + `#[derive(RuntimeModule)]` + `just scaffold-module`. Reuse the daemon that already exists (`ResourceDaemon`), or scaffold a declared module. Never spawn a raw task on a whim.
- **A launch-time or one-shot memory lever** (`-c` decided once, YaRN "at launch", "grow her a deep lane" as a discrete action). Everything is the continuous republish loop.
- **`Arc<Mutex<T>>` for cross-task allocation reads.** `watch` + `.borrow()`, lock-free; never block the authority's tick.
