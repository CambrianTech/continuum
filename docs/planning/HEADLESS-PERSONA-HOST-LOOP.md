# Headless Persona Host Loop — #133 Slice 13 Design

**Status:** Design (slice 13 not yet implemented). Revised 2026-06-02 in response to PR #1510 review.

**Tracks:** #133 ("LCD-first substrate spawn path"), #121 ("PersonaSpawnerModule"). Reads on top of the merged slices 5–12.

**Adjacent issues:** task #52 (Governor classify_silicon misclassifies Mac Intel as AppleM), task #82 (CBOR Response::Event schema mismatch), task #88 (Disk pressure as substrate concern).

---

## The moment-of-truth

After slices 5–12, the substrate has:

- **Planning** (slice 7): `PersonaSpawnerModule::plan() → Vec<DesiredRole>`
- **Bootstrap-and-plan** (slice 8): `bootstrap_planned(...) → Vec<MaterializedPersonaPlan>` — turns plan rows into airc identities via `PersonaInstanceManagerModule::bootstrap_one`, plus the per-row inference profile.
- **Adapter materialization** (slice 9): `materialize_adapters(plans, factory) → Vec<Result<HostedPersona, SupervisorError>>` with structured per-slot error variants.
- **Service loop** (slice 10): `serve_persona_loop(hosted, conversation, reader, opts)`
- **Production conversation** (slice 11): `AircPersonaConversation::new(runtime)`
- **Spawn helper** (slice 12): `spawn_persona_service(hosted, runtime, opts, rt_handle) → JoinHandle<Result<ServeOutcome, String>>`

What's missing: **nothing in `continuum-core` actually composes this at boot.** The existing IPC boot loop at `crate::ipc::start_server` (≈line 1024–1089) calls `bootstrap_one(&intent)` and then *only logs* — it never starts hosting the persona on the grid.

Slice 13 is the rewire that makes the substrate actually host personas headlessly. The demo binary keeps working (slice 11 reshape) but stops being on the critical path.

---

## Slice 13 scope vs deferred follow-ups (REVISED per PR #1510 re-review)

The original draft listed P1/P3/Q5/Q7 as "hard prerequisites that MUST land in slice 13." PR #1510 re-review #2 caught the divergence: the slice 13 implementation (#1511, now merged) explicitly defers all four with TODOs cited in the boot-composition preamble. This section accurately reflects what shipped vs what's deferred — and why the substrate is whole without them.

### Shipped in slice 13 (PR #1511, merged 2026-06-02)

- **Q1**: `bootstrap_planned` / `derive_spawn_plan` / `build_profile` take `&Registry` instead of `&Arc<Registry>`.
- **Q3**: `PersonaAircRuntimeRegistry` extended to `PersonaSlot { runtime, service_loop }`. `attach_service_loop`, `is_service_loop_finished`, `shutdown_slot` methods added. One keyspace owns both.
- **P2**: `plan_for_tier` returns single Helper. `debug_assert!(plan.len() <= 1)` at the producer. `slice_14_restores_helper_plus_coder_for_compat` test pinned `#[ignore]` until slice 14.
- **Boot composition**: `crate::ipc::start_server` boot loop replaced by `bootstrap_planned → materialize_adapters → spawn_persona_service → attach_service_loop`. Old welcome-log-only path deleted per [[organization-purity-as-we-migrate]].
- **Q2 (partial)**: boot uses `HwCapabilityTier::CpuOnly + HwTierCategory::Compat` hardcoded. `detect_host_capability(&gpu_monitor, &system_info)` is a 3-line replacement once a production `GpuMonitor` constructor exists (no production callsite builds one today; only tests do). TODO in `ipc/mod.rs` cites task #52.

### Deferred follow-ups (slice 13.5+)

- **P1**: `tokio::signal::ctrl_c()` → `Runtime::shutdown` is NOT wired in slice 13. Per-slot shutdown is available via `PersonaAircRuntimeRegistry::shutdown_slot` and exercised by `persona/instances/*` IPC commands. Server-level signal handler is its own sub-slice. **Consequence today:** server stops via process kill; tokio runtime drop reaps daemon-attach tasks. Per the cleanup-model section below this is sufficient against the pinned airc rev — `.abort()` (or drop) on the `EventStream`'s `DaemonAttachGuard` aborts the per-channel attach handles. No leak.
- **P3**: `ResourceBroker.acquire` admission before `factory.build_adapter` is NOT in slice 13. Current LCD case is 1 persona × ~500 MiB GGUF, well within all supported tiers. Becomes load-bearing when multi-persona returns in slice 14 (where #122 shared-base + LoRA paging will need broker admission for the LoRA cache).
- **Q5**: structured `BootSummary` event publishing is NOT in slice 13. The boot composition logs `hosted_count` / `failed_count` / per-slot `tracing::warn!` for now. Operator observability via log scraping. `MessageBus::publish("persona:boot:summary", ...)` is a slice-13.5 follow-up when a subscriber (alerter, dashboard) wants it.
- **Q7**: per-persona `is_service_loop_finished` poller is NOT in slice 13. The registry exposes `is_service_loop_finished` so a supervisor poller can land later; for now operators observe via `persona/instances/list` (the registry surfaces the same data through the IPC command).

### Why this divergence is acceptable

Single-persona LCD is in-budget for all tiers. The supervisor's `shutdown_slot` (the orderly path that registry-removes + JoinHandle-aborts) is already exposed for the IPC commands. The cleanup-model section below shows that `.abort()` ALONE (the path tokio runtime drop takes) is sufficient against the pinned airc rev. P1/P3/Q5/Q7 are observability + back-pressure refinements, not invariants the slice-13 substrate violates.

The original "Hard prerequisites" framing predates that verification. The implementation deferred them because the substrate doesn't break without them. The Q2 deferral (no production `GpuMonitor` constructor) is the only one tied to a genuine missing primitive; tasks #52 + slice 13.5 cover it.

### P2. Single-persona-per-plan invariant (until slice 14)

**Status:** the position-pairing of `provider.next_persona()` outputs to plan rows is **broken from boot 2 onward**.

Trace:
1. Boot 1: provider mints two fresh identities in plan order. `mint_fresh_intent()` (`resume_or_mint_provider.rs:134`) creates `Uuid::new_v4()` + derives `agent_name` from `agent_name_from_identity(uuid)`. Names are random per UUID.
2. Plan order `[Helper, Coder]`, persona "Maya" (random) → slot 0 / Helper, "Bart" (random) → slot 1 / Coder. Both write `seed.json` to `personas/<name>/`.
3. Boot 2: `scan_personas_dir` (`resume_or_mint_provider.rs:200`) calls `dir_entries.sort()` — alphabetical. Disk yields `[Bart, Maya]`. Position-pair: Bart=Helper (was Coder), Maya=Coder (was Helper). **Role identity flipped.**

This is a pre-existing latent bug in `bootstrap_planned` (slice 8) but slice 13 is the first place where (persona_id, role) becomes load-bearing for cognition.

**Slice 13 mitigation:** constrain `plan_for_tier` output to ONE row max via a `debug_assert!(plan.len() <= 1)` at the boot composition. `PersonaSpawnerModule::plan_for_tier` currently returns `[Helper, Coder]` for Compat (slice 7) — the Coder entry gets temporarily commented out behind a `// TODO #133 slice 14: re-enable when RoleAwareProvider lands` marker, and we ship the substrate hosting one Helper persona at all tiers.

**Slice 14 owns:** writing `role: RoleId` into `seed.json` on mint, reading it on resume, refusing to boot if a seed is missing the role field. THEN `plan_for_tier` returns `[Helper, Coder]` again.

This is a real regression in coverage vs the demo binary today (which hosts one persona but its role isn't substrate-typed). Slice 13 still ships the supervisor path; the multi-persona case waits one slice.

---

## Boot flow before / after

### Before (current `ipc/mod.rs:1024–1089`)

```rust
let bootstrap_handle = instance_manager.clone();
rt_handle.spawn(async move {
    let mut provider = ResumeOrMintProvider::new(&continuum_root_for_boot, 1).await?;
    loop {
        let intent = provider.next_persona().await?.unwrap();
        match bootstrap_handle.bootstrap_one(&intent).await {
            Ok(info) => tracing::info!(/* welcome log */),  // ← persona online but mute
            Err(e) => tracing::warn!(/* boot failure */),
        }
    }
});
```

The runtime is registered in `PersonaAircRuntimeRegistry`. Nothing pulls events from her airc subscribe stream. She's reachable via `airc peers` but never responds.

### After (slice 13 — composes existing primitives)

The "after" code uses `bootstrap_planned` (slice 8) + `materialize_adapters` (slice 9) directly, not an open-coded re-implementation. Net-new code is the loop over `Vec<HostedPersona>` calling `spawn_persona_service` with broker admission and slot-result accumulation:

```rust
// Pre-boot composition (one-time, before the boot task)
let host_capability = detect_host_capability(&gpu_monitor, &system_info);  // see Q2
let tier_id = host_capability.tier_id();                                    // String
let tier_category = HwTierCategory::from(host_capability);
let model_registry: &'static Registry = model_registry::global();           // see Q1
let factory: Arc<dyn PersonaAdapterFactory> = Arc::new(LlamaCppPersonaAdapterFactory);
let spawner = PersonaSpawnerModule::new(host_capability, tier_category);

// Boot task (replaces the existing 1024–1089 loop)
let bootstrap_handle = instance_manager.clone();
let registry_for_lookup = instance_manager.registry().clone();
let supervisor_for_handles = persona_supervisor.clone();  // see Q3
let broker_for_admission = resource_broker.clone();        // see P3

rt_handle.spawn(async move {
    let mut provider = ResumeOrMintProvider::new(&continuum_root_for_boot, 1).await?;

    // Slices 8 + 9 already do the heavy lifting — compose them.
    let plans = bootstrap_planned(
        &spawner, &bootstrap_handle, &mut provider, &tier_id, &model_registry_arc,
    ).await?;  // returns Vec<MaterializedPersonaPlan>, structured per-slot errors threaded through

    let hosted_results = materialize_adapters(plans, &*factory).await;
    //                                              ↑ per #122 needs broker admission here too;
    //                                                slice 13 adds the wrapper

    let mut hosted_count: usize = 0;
    let mut failed_count: usize = 0;
    for (slot_idx, result) in hosted_results.into_iter().enumerate() {
        match result {
            Ok(hosted) => {
                // `hosted` is the PersonaContext — already carries
                // the airc runtime as `hosted.runtime` per the
                // `&ctx` doctrine. No separate lookup.
                let persona_id = hosted.identity.persona_id;
                let handle = spawn_persona_service(
                    hosted, ServeOptions::default(), rt_handle.clone(),
                );
                if let Err((returned_handle, reason)) = registry_for_lookup
                    .attach_service_loop(persona_id, handle)
                    .await
                {
                    returned_handle.abort();
                    let _ = returned_handle.await;
                    tracing::error!(slot=slot_idx, persona_id=%persona_id, reason, "attach failed; handle drained");
                    failed_count += 1;
                    continue;
                }
                hosted_count += 1;
            }
            Err(err) => {
                tracing::warn!(slot=slot_idx, error=?err, "slot materialization failed");
                failed_count += 1;
            }
        }
    }
    tracing::info!(hosted=hosted_count, failed=failed_count, "🌐 Substrate boot composition complete (slice 13)");
});
```

Net-new code in slice 13: ~25 lines of composition. Everything else is composing existing primitives — `bootstrap_planned` (slice 8), `materialize_adapters` (slice 9), `spawn_persona_service` (slice 12), `attach_service_loop` (slice 13 Q3). `BootSummary` event publishing is the **Q5 deferred follow-up** noted in the scope section above — for now slice 13 emits `tracing::info!` lines with the same counters; a structured `MessageBus::publish("persona:boot:summary", ...)` lands when a subscriber wants it.

---

## Cleanup model (REVISED per PR #1510 re-review against pinned `f6ed190`)

PR #1510 re-review #1 caught that the prior revision named the wrong cleanup mechanism. The actual mechanism against the airc rev pinned in `src/workers/Cargo.toml:44-48` (`f6ed190`) is `DaemonAttachGuard`, not the older `ensure_wire_subscriber` / `inner.subscribers` map.

### What actually fires the cleanup

Production subscribe (`airc-lib/src/messaging.rs:204 subscribe()`) is daemon-attached. It returns an `EventStream` whose internal `EventStreamInner::Daemon` variant holds a `DaemonAttachGuard` (`airc-lib/src/stream.rs:25-68`). The guard owns `Vec<JoinHandle<()>>` — the per-channel attach tasks spawned by `daemon_subscribe`.

`DaemonAttachGuard::drop` (`stream.rs:62-68`):
```rust
impl Drop for DaemonAttachGuard {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}
```

When the `EventStream` drops, the guard drops, the per-channel attach tasks abort. IPC connections close at the next poll. **No leak.**

### The cleanup chain on `JoinHandle.abort()`

1. **`abort()` on the spawned service-loop `JoinHandle`**: tokio cancels the task at the next await point.
2. **Task drops**: `hosted` (PersonaContext) and `conversation` (AircPersonaConversation) drop.
3. **`AircPersonaConversation::drop`**: drops the held `EventStream`.
4. **`EventStream::drop` → `DaemonAttachGuard::drop`**: aborts each per-channel attach `JoinHandle`. Per-channel attach tasks tear down, IPC handles close.

`.abort()` ALONE is sufficient.

### What `shutdown_slot` adds (and why)

`PersonaAircRuntimeRegistry::shutdown_slot(persona_id)` does the strictly-stronger sequence:
1. Take the JoinHandle out of the slot.
2. `abort()` it AND `await` the JoinHandle (drains cleanly — the abort path's cancellation Error is discarded; we did the shutdown intentionally).
3. Remove the slot from the registry — drops `Arc<PersonaSlot>`, drops `Arc<PersonaAircRuntime>`, drops `Arc<Airc>` (once the last reference releases).

The `await` step ensures the task has fully cancelled before the function returns — useful for orderly server shutdown where the next step depends on this persona being gone. The registry-remove step releases the in-substrate Arc references (so a follow-up `registry.get(persona_id)` correctly returns `None`).

But the daemon-side IPC cleanup happens via the `DaemonAttachGuard` drop chain inside step 2 (the abort path) — the registry-remove is for in-substrate state hygiene, not for daemon-side teardown. Either path (just abort OR shutdown_slot) cleans up the daemon side. `shutdown_slot` adds the registry-remove + drain ordering on top.

### Practical implication

- **Tokio runtime drop on process exit** (slice 13's actual shutdown path until P1 lands): all task `JoinHandle`s drop, all daemon-attach tasks abort via the guard chain. Daemon sees the IPC connection close at its end of the socket. Clean enough for a server exit.
- **Per-slot shutdown via `persona/instances/*` IPC commands**: uses `shutdown_slot`; orderly + drained + registry cleared. Operator-driven and load-bearing once slice 14 ships multi-persona.
- **`P1` (slice 13.5)**: wires `tokio::signal::ctrl_c` → walk the registry → call `shutdown_slot` on each persona → then `runtime.shutdown()`. The orderly path for graceful Ctrl-C.

### Architectural constraint (now corrected)

The original "registry-remove is on the cleanup path" framing was wrong for the daemon-side teardown — it's on the in-substrate-state cleanup path. The daemon-side teardown is the `DaemonAttachGuard` drop chain. Both paths matter at different layers; conflating them was the doc's error.

`PersonaAircRuntimeRegistry` is still the single keyspace owning per-persona lifetime info (Q3's resolution stands) — but it's not the sole authority on daemon-side resource release. That authority is the `DaemonAttachGuard` chain inside airc-lib. The substrate just has to let `EventStream` drop happen (which it does, on any abort or task drop).

---

## Open questions for slice 13 implementation

### Q1. How does the boot path get a `Registry` for `bootstrap_planned`?

`bootstrap_planned` (slice 8) takes `&Arc<crate::model_registry::Registry>`. The current singleton is `OnceLock<Registry>` (`model_registry/singleton.rs:23`) returning `&'static Registry`.

**Option A — Add `model_registry::global_arc() → Arc<Registry>`.** Requires changing the singleton storage from `OnceLock<Registry>` to `OnceLock<Arc<Registry>>` AND updating every existing caller of `global()` (the `&'static Registry` lifetime changes). **Not "tiny."**

**Option B — Refactor `bootstrap_planned`'s signature to take `&Registry`.** One-callsite change inside slice 8. Touches `derive_spawn_plan` + `build_profile` (slice 6 + 5) too — they internally hold the `Arc` for cloning across roster entries.

**Revised recommendation (corrected from PR #1510 review):** **(B) is smaller** if we accept passing `&Registry` and using `Arc::new()` once internally in `build_profile` (cost: one Arc allocation per persona at boot). Option A's cost analysis was inverted in the first revision of this doc — singleton-storage migration is not "tiny."

### Q2. Where does `host_capability: HwCapabilityTier` come from?

The existing `cognition/host_capability_probe.rs:87 fn detect_host_capability(gpu_monitor, system_info) -> HwCapabilityTier` already does this work. No new struct needed.

**Recommendation:** call `detect_host_capability(&gpu_monitor, &system_info)` from the IPC boot path. ~3 lines.

**Impact of task #52 (Mac Intel misclassification):** slice 13 inherits the misclassification — on Intel Mac with AMD discrete, `detect_host_capability` may classify wrong. Practical effect: the persona is hosted as the wrong tier's `RoleId::Helper` model. Substrate keeps running; persona-side performance is sub-optimal. Acceptable for slice 13; task #52 is the right place to fix the classifier, not slice 13.

### Q3. Where do `hosted_handles` go for shutdown? Does `PersonaSupervisor` duplicate `PersonaAircRuntimeRegistry`?

`PersonaAircRuntimeRegistry` (`airc_runtime_registry.rs:50`) already holds `persona_id → Arc<PersonaAircRuntime>` — "who's currently online." The proposed PersonaSupervisor holds `persona_id → JoinHandle<...>` — "who's currently being served." **Same keyspace.** Per the compression principle this is a smell.

**Revised recommendation (per PR #1510 review):** **extend `PersonaAircRuntimeRegistry`** rather than introducing a parallel module. The `Arc<PersonaAircRuntime>` registration becomes an `Arc<HostedPersonaRuntime>` that owns both the airc runtime AND the optional service-loop `JoinHandle`:

```rust
pub struct HostedPersonaRuntime {
    pub airc_runtime: Arc<PersonaAircRuntime>,
    pub service_loop: Mutex<Option<JoinHandle<Result<ServeOutcome, String>>>>,
}
```

Registry's existing `remove(persona_id)` method becomes the natural shutdown path: aborts the JoinHandle, awaits drain, drops the Arc. The cleanup chain in the previous section flows through one place. No duplicate keyspace.

This is a slice-13 modification to `airc_runtime_registry.rs` — small, additive, doesn't break existing callers (they're all reading the inner `Arc<PersonaAircRuntime>`).

### Q4. ResumeOrMintProvider count + role mapping → (see P2)

Per P2, slice 13 ships with `plan.len() <= 1`. Slice 14 lands `RoleAwareProvider` + role-in-seed.json. Doc resolves to single-role until then.

### Q5. Error policy for per-slot failures

Per [[no-fallbacks-ever]]: per-slot failures stay failed. Per [[observability-is-half-the-architecture]]: the boot path publishes a `BootSummary { planned, hosted, failed: Vec<(slot, role, error)> }` to the existing event bus.

**Venue (corrected from PR #1510 review):** publish to `MessageBus::publish(BusEvent { name: "persona:boot:summary", payload })`. No declared subscribers in slice 13 — the event is for operator scraping via `events/recent` IPC. If a future module wants automated alerting (e.g., "more than 50% slots failed → page someone"), it subscribes then. This is a "log it now, react to it later" event per the substrate's standard observability pattern.

### Q6. Hot-reload semantics for seed.json (NEW — was Finding #10)

**Out of scope for slice 13.** The boot loop runs once and exits. New seed.json files appearing at runtime do NOT trigger re-scan. Operator can fire the existing `persona/instances/bootstrap` IPC command (`ipc/mod.rs:947`) to bring up a new persona ad-hoc. A filesystem-watcher path is reasonable future work but is its own design problem (race conditions on partial writes, etc.).

### Q7. Wire-subscription failure mid-boot (NEW — was Finding #11)

If `AircPersonaConversation::next_message`'s first call fails (daemon dies between bootstrap and service-loop start), `serve_persona_loop` returns `Err`, the `JoinHandle` resolves with `Err(...)`. Slice 13's design: the supervisor periodically inspects `JoinHandle::is_finished()` for each registered persona, and if a handle resolved with Err, logs + emits a `persona:host:failed` event + removes from the registry. This is a slice-13 polling task (every 5s say) rather than a JoinHandle-watch (which would need a separate listener task per persona). Polling is enough — the failure mode is rare and operator-investigated.

---

## Test plan for slice 13

1. **`PersonaBootstrapper` trait split** — for stubbing `bootstrap_one` without an airc daemon. Modest scope; lets the boot composition test exercise the (registry-lookup, profile-build, adapter-build, spawn) chain on stubs.
2. **Stub airc daemon for integration**: slice 12 already established the pattern (`StubConversation` in `service_loop.rs` tests). Slice 13's integration test wires a stub `PersonaInstanceManagerModule` + stub adapter factory + stub conversation through the boot composition function. Verifies BootSummary contents on happy + failure paths.
3. **`Runtime::shutdown` test**: a small test that constructs a boot composition + invokes `runtime.shutdown().await` + asserts all JoinHandles are torn down + all wire subscribers gone (via stub airc).
4. **Per-slot failure isolation**: stub adapter factory rejects slot 0; verify slots 1+ still spawn; BootSummary captures the slot-0 rejection.
5. **`scan_personas_dir` boot-order test (REGRESSION)**: write the alphabetical-sort hazard into a fixture test that boots, restarts, asserts roles haven't flipped. Goes red until slice 14 lands role-in-seed.json. Pinned as `#[ignore = "tracks task #133 slice 14"]` until then.

Integration test happens via the IPC server itself — no real airc daemon, no real GGUF; the stub layer is the substrate's testability story.

---

## What slice 13 does NOT do

- **Multi-persona-per-plan** — see P2. Single Helper per tier until slice 14.
- **Shared-base / LoRA paging (#122)** — slice 13 builds one adapter per persona via `LlamaCppPersonaAdapterFactory`. Two adapters at LCD is fine (~1 GiB). Higher-tier multi-persona waits for #122.
- **Cross-grid inference (#108)** — every persona routes through local adapter.
- **Per-persona LoRA selection (#124, #126)** — today's profile carries `model_id` only.
- **Hot-reload of seed.json** — see Q6.
- **Filesystem-watcher persona introduction** — see Q6.
- **Automated alerting on BootSummary failures** — see Q5; out of scope until a subscriber wants it.

---

## Slice-13 status (PR #1511 merged 2026-06-02)

**Shipped:**
- [x] Q1 — `bootstrap_planned` signature takes `&Registry`
- [x] Q3 — `PersonaAircRuntimeRegistry` extended to `PersonaSlot { runtime, service_loop }`; `attach_service_loop`, `is_service_loop_finished`, `shutdown_slot` methods
- [x] P2 — `plan_for_tier` returns single Helper, `debug_assert!`, ignored slice-14 regression test
- [x] Boot loop at `ipc/mod.rs` REPLACED by the composition above (per [[organization-purity-as-we-migrate]])
- [x] Q2 (partial) — hardcoded `CpuOnly + Compat` with TODO citing task #52 + missing production `GpuMonitor` constructor

**Plus integration polish in PR #1511:**
- [x] Room-name discovery (`discover_default_room_name`) — fixes the join-by-uuid-as-string hazard that landed Paige in the wrong channel
- [x] LCD model (`continuum-ai/qwen2.5-0.5b-instruct-GGUF`) added to `model_registry::catalog::models()`
- [x] `PersonaContext` rename (was `HostedPersona`) + `RagInspectionRequest::for_persona(&profile)` single derivation site (the `&ctx` doctrine — see [[context-is-the-client-airc-token-is-identity]])

**Deferred to slice 13.5+ (see Scope section above for reasoning):**
- [ ] P1 — `tokio::signal::ctrl_c()` → `runtime.shutdown()` wired
- [ ] P3 — `ResourceBroker.acquire(...)` admission before factory.build_adapter
- [ ] Q5 — structured `BootSummary` `MessageBus::publish`
- [ ] Q7 — supervisor poller checking `is_service_loop_finished` every 5s
- [ ] Q2 completion — `detect_host_capability` once a production `GpuMonitor` constructor lands (task #52-adjacent)

**Integration validation (2026-06-02):**
- Substrate-hosted Paige replied in Joel's `continuum` room: "Hello Joel, thank you for testing the substrate-managed host loop. I'm here to assist with any questions or concerns you have." Full trace: airc msg → boot composition → AircPersonaConversation subscribe → RagInspectionRequest::for_persona(&ctx.profile) → inspect_persona_rag_with_inference → ctx.runtime.say. Five layers proved end-to-end on Intel Mac CPU-only.

---

## Reference docs (canonical substrate)

- [docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md](../architecture/CBAR-SUBSTRATE-ARCHITECTURE.md) — runtime contract every module inherits
- [docs/architecture/GENOME-FOUNDRY-SENTINEL.md](../architecture/GENOME-FOUNDRY-SENTINEL.md) — tiered artifact sharing
- [docs/architecture/AI-COMMAND-NAMESPACE.md](../architecture/AI-COMMAND-NAMESPACE.md) — every AI/ML thing under `ai/*`
- [docs/architecture/INFERENCE-LANES-REALISTIC.md](../architecture/INFERENCE-LANES-REALISTIC.md) — realistic floor: ONE base, N persona lanes
- [docs/planning/ALPHA-GAP-ANALYSIS.md](ALPHA-GAP-ANALYSIS.md) — lane-shaped roadmap
- [docs/planning/AI-LANE-OPEN-QUESTIONS.md](AI-LANE-OPEN-QUESTIONS.md) — explicit punch list

---

## Memories worth refreshing on slice 13 implementation

- [[no-fallbacks-ever]] — per-slot errors stay errored
- [[no-if-statements-use-llms-for-cognition]] — boot path filters substrate signals only; LLM judges everything else
- [[no-stdio-piping-for-process-ipc]] — substrate talks only via typed sockets
- [[substrate-is-a-good-citizen-on-the-host]] — caller controls scheduling pool AND consults ResourceBroker before adapter spawn (P3)
- [[observability-is-half-the-architecture]] — BootSummary as a first-class event
- [[organization-purity-as-we-migrate]] — slice 13 deletes the welcome-log-only path; doesn't keep both. Also: don't reinvent `bootstrap_planned` / `materialize_adapters` — compose them.
- [[constitutional-design-always-a-next-step]] — every open question has a recommendation
- [[commands-are-dumb-daemons-are-smart]] — `spawn_persona_service` stays trivial; the smart roster reconciliation lives in the supervisor
- [[every-error-is-an-opportunity-to-battle-harden]] — the position-pairing hazard (P2) gets a regression test that goes red until slice 14 lands
