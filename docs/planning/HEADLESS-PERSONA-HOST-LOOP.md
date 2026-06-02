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

## Hard prerequisites (must land BEFORE or AS PART OF slice 13)

These are not optional. Without them the slice-13 wire-up is dead code.

### P1. Server shutdown signal wired to `Runtime::shutdown`

**Status:** missing. `Runtime::shutdown` exists at `runtime/runtime.rs:354`, but `grep -rn 'shutdown()' src/{ipc,bin}/` returns ZERO callers. The server has no `tokio::signal::ctrl_c()` → graceful shutdown today.

**Consequence if skipped:** the supervisor's `.abort()` hook on each persona's `JoinHandle` becomes dead code — handles drop on process exit, daemon-attach tasks get reaped by tokio runtime drop instead of the orderly path. Wire subscribers leak until process death.

**Required work in slice 13:** wire `tokio::signal::ctrl_c()` (or platform equivalent) in `ipc::start_server`'s main task to call `runtime.shutdown().await`. PersonaSupervisor's `shutdown()` impl then walks its `JoinHandle` collection.

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

### P3. ResourceBroker admission for adapter spawns

**Status:** `modules/resource_broker.rs` exists. Slice 13's adapter materialization loop currently has no admission check.

**Consequence if skipped:** even the LCD case loads ~500 MiB Q4_K_M GGUF per persona. Two personas = ~1 GiB. M5UmaProMax tier could be 5+. Substrate must consult the broker before each `factory.build_adapter()` call. Per [[substrate-is-a-good-citizen-on-the-host]].

**Required work in slice 13:** add `broker.acquire(adapter_memory_budget).await?` before each `materialize_adapters` factory call. Per-slot rejection → mark slot as `RejectedByBroker` in `BootSummary`. The lease handle threads into `HostedPersona` so it releases when the conversation drops.

This is materially new code, not pure composition. If P3 is too heavy for slice 13, the alternative is to gate slice 13 on landing P3 as a slice 12.5 first.

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

    let mut summary = BootSummary::default();
    for (slot_idx, result) in hosted_results.into_iter().enumerate() {
        match result {
            Ok(hosted) => {
                let Some(runtime) = registry_for_lookup.get(hosted.instance.persona_id) else {
                    summary.failed.push((slot_idx, hosted.role, "runtime missing post-bootstrap".into()));
                    continue;
                };
                let handle = spawn_persona_service(
                    hosted.clone(), runtime, ServeOptions::default(), rt_handle.clone(),
                );
                supervisor_for_handles.register(hosted.instance.persona_id, handle);
                summary.hosted += 1;
            }
            Err(err) => {
                let (slot, role) = err.slot_and_role();
                summary.failed.push((slot, role, err.to_string()));
            }
        }
    }
    bus.publish(BusEvent::new("persona:boot:summary", summary.into()));  // see Q5
});
```

Net-new code in slice 13: ~25 lines of composition + the `BootSummary` struct + the `PersonaSupervisor::register` call surface. Everything else is composing existing primitives.

---

## Cleanup model (corrected from PR #1510 review)

The slice-12 review noted this section was wrong on the specifics. Corrected:

The actual cleanup chain on `JoinHandle.abort()`:

1. **`abort()` on the JoinHandle**: tokio cancels the task at the next await point. Task drops, owning `hosted` + `conversation`.
2. **`AircPersonaConversation::drop`**: drops the lazy `Option<EventStream>`.
3. **`EventStream::drop`** (`airc-lib/src/stream.rs`): drops the `BroadcastStream<Arc<TranscriptEvent>>` (in-process broadcast receiver). Tokio's broadcast Receiver `drop` decrements the in-process subscriber count.
4. **Wire subscriber** (`airc-lib/src/transport.rs:65 ensure_wire_subscriber`): **NOT dropped by EventStream alone.** The wire subscriber lives in `Arc<Airc>.inner.subscribers` (a `HashMap<PathBuf, WireSubscriber>`). It's idempotent on `subscribe()` — multiple local broadcast subscribers reuse the same wire subscriber. It tears down only via:
   - Explicit `teardown_wire(&wire)` call (not the path we use), OR
   - `Arc<Airc>` reaches refcount 0 (drops `inner.subscribers`).

**Practical implication for the supervisor:** **`.abort()` alone is INSUFFICIENT to release the daemon-side wire subscription.** The supervisor's shutdown path MUST also:

```rust
join_handle.abort();
join_handle.await;  // drain the abort
let runtime = registry.remove(persona_id);  // drops Arc<PersonaAircRuntime> → drops Arc<Airc>
// Arc<Airc> drop → inner.subscribers drop → wire subscriber tasks drop
```

If the supervisor's `.abort()` runs but the registry still holds the `Arc<PersonaAircRuntime>`, the wire subscriber stays alive and the daemon keeps sending events into an in-process broadcast channel with no readers — events accumulate in the broadcast buffer until the channel's overflow policy kicks in.

**This is a real architectural constraint** that the slice-13 PersonaSupervisor design has to honor. It's also Finding #4 (next section) — `PersonaAircRuntimeRegistry` is on the cleanup path, so the supervisor can't be orthogonal to it.

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

## Slice-13 implementation checklist (extracted from above for the PR)

- [ ] P1 — `tokio::signal::ctrl_c()` → `runtime.shutdown()` wired in `ipc::start_server`
- [ ] P2 — `plan_for_tier` Compat tier returns `[Helper]` only; Coder entry commented with `// TODO #133 slice 14`; `debug_assert!(plan.len() <= 1)` at the boot composition
- [ ] P3 — `ResourceBroker.acquire(...)` called before each `factory.build_adapter`; lease tied to `HostedPersona` lifetime
- [ ] Q1 — `bootstrap_planned` signature takes `&Registry`, internal `Arc::new` once per call
- [ ] Q2 — boot path calls `detect_host_capability(&gpu_monitor, &system_info)` directly
- [ ] Q3 — `PersonaAircRuntimeRegistry` extended to hold `HostedPersonaRuntime { airc_runtime, service_loop }`; `remove(persona_id)` becomes the shutdown path
- [ ] Q5 — `BootSummary` struct + `MessageBus::publish("persona:boot:summary", ...)` at boot composition end
- [ ] Q7 — supervisor poller task checking `JoinHandle::is_finished()` per registered persona, every 5s
- [ ] Boot loop at `ipc/mod.rs:1024-1089` REPLACED by the composition above (delete the welcome-log-only path per [[organization-purity-as-we-migrate]] — don't keep both)
- [ ] Tests 1–5 from the test plan section land alongside

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
