# Headless Persona Host Loop — #133 Slice 13 Design

**Status:** Design (slice 13 not yet implemented). Captures the wire-up plan + the cleanup model verified during slice-12 review.

**Tracks:** #133 ("LCD-first substrate spawn path"), #121 ("PersonaSpawnerModule"). Reads on top of the merged slices 5–12.

**Adjacent issues:** task #52 (Governor classify_silicon misclassifies Mac Intel as AppleM), task #82 (CBOR Response::Event schema mismatch).

---

## The moment-of-truth

After slices 5–12, the substrate has:

- **Planning** (slice 7): `PersonaSpawnerModule::plan() → Vec<DesiredRole>`
- **Bootstrap** (slice 8): `bootstrap_planned(...) → Vec<MaterializedPersonaPlan>` — turns plan rows into airc identities via `PersonaInstanceManagerModule::bootstrap_one`
- **Adapter materialization** (slice 9): `materialize_adapters(plans, factory) → Vec<HostedPersona>`
- **Service loop** (slice 10): `serve_persona_loop(hosted, conversation, reader, opts)`
- **Production conversation** (slice 11): `AircPersonaConversation::new(runtime)`
- **Spawn helper** (slice 12): `spawn_persona_service(hosted, runtime, opts, rt_handle) → JoinHandle<Result<ServeOutcome, String>>`

What's missing: **nothing in `continuum-core` actually calls this composition at boot.** The existing IPC boot loop at `crate::ipc::start_server` (≈line 1024–1089) calls `bootstrap_one(&intent)` and then *only logs* — it never starts hosting the persona on the grid.

Slice 13 is the rewire that makes the substrate actually host personas headlessly. The demo binary keeps working (slice 11 reshape) but stops being on the critical path. `npm start` with no demo binary running produces talking personas.

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

### After (slice 13)

```rust
let bootstrap_handle = instance_manager.clone();
let registry_for_lookup = instance_manager.registry().clone();
let model_registry = crate::model_registry::global_arc();  // see "open question 1"
let hw_capability = HostCapabilityProbe::detect();         // see "open question 2"
let tier_category = HwTierCategory::from(hw_capability);
let factory: Arc<dyn PersonaAdapterFactory> = Arc::new(LlamaCppPersonaAdapterFactory);
let spawner = PersonaSpawnerModule::new(hw_capability, tier_category);
let plan = spawner.plan();  // Vec<DesiredRole>
let mut hosted_handles: Vec<JoinHandle<Result<ServeOutcome, String>>> = Vec::new();

rt_handle.spawn(async move {
    let mut provider = ResumeOrMintProvider::new(&continuum_root_for_boot, plan.len()).await?;
    for (slot_index, desired) in plan.iter().enumerate() {
        let Some(intent) = provider.next_persona().await? else {
            tracing::warn!(slot=slot_index, "provider exhausted before plan complete");
            break;
        };
        let info = match bootstrap_handle.bootstrap_one(&intent).await {
            Ok(i) => i,
            Err(e) => { tracing::warn!(error=%e, "bootstrap failed for slot {slot_index}"); continue; }
        };
        let Some(runtime) = registry_for_lookup.get(info.persona_id) else {
            tracing::error!("runtime missing from registry post-bootstrap — substrate bug");
            continue;
        };
        let profile = match build_profile(
            info.persona_id, &info.agent_name, desired.role.as_str(),
            &hw_capability_id_str, tier_category, &desired.model_id, &model_registry,
        ) {
            Ok(p) => p,
            Err(e) => { tracing::warn!(error=%e, "profile failed for slot {slot_index}"); continue; }
        };
        let adapter = match factory.build_adapter(&profile).await {
            Ok(a) => a,
            Err(e) => { tracing::warn!(error=%e, "adapter failed for slot {slot_index}"); continue; }
        };
        let hosted = HostedPersona {
            role: desired.role,
            instance: info,
            adapter,
        };
        let handle = spawn_persona_service(
            hosted, runtime, ServeOptions::default(), rt_handle.clone(),
        );
        hosted_handles.push(handle);
        tracing::info!("🌐 hosting slot {slot_index} as {} ({})", desired.role.as_str(), desired.model_id);
    }
    // Hand `hosted_handles` to shutdown hook (see "open question 3")
});
```

---

## Cleanup model (verified during slice-12 review)

Per the slice-12 reviewer's note (1), I traced the cleanup path through `airc_lib`:

1. **`spawn_persona_service` → JoinHandle.abort()**: tokio cancels the task at the next await point. The task owns `hosted`, `runtime: Arc<PersonaAircRuntime>`, and `conversation: AircPersonaConversation`. All three drop in order.
2. **`AircPersonaConversation::drop`**: drops its lazy `Option<EventStream>`.
3. **`EventStream::drop`** (`airc-lib/src/stream.rs:12`): drops `BroadcastStream<Arc<TranscriptEvent>>`, which drops the `tokio::sync::broadcast::Receiver`. tokio's broadcast channel implementation automatically decrements the subscriber count when the receiver drops — **no manual unsubscribe needed**.
4. **Wire-level subscription** (`messaging.rs:186` `ensure_wire_subscriber`): this is the per-room daemon-side subscription. It is **ref-counted across all local subscribers to the same room** — dropping one EventStream does NOT tear down the wire subscription. The wire subscriber tears down when the `Arc<PersonaAircRuntime>` (which holds the `Arc<Airc>`) reaches refcount 0, which happens when:
   - The conversation drops (drops one reference)
   - The supervisor's `Vec<HostedPersona>` drops or the slot gets `.abort()`-ed (drops the other reference held by `runtime: Arc<...>`)

**Conclusion:** dropping the JoinHandle alone is sufficient cleanup. No leak. The slice-12 reviewer's concern was reasonable but the architecture already handles it via Arc semantics + the broadcast channel's subscriber-count drop hook.

This SHOULD go into a doc-comment on `spawn_persona_service` so future authors don't re-derive it.

---

## Open questions for slice 13 implementation

### 1. How does the boot path get an `Arc<Registry>` for `build_profile`?

`build_profile` (slice 5) takes `&Arc<crate::model_registry::Registry>`. The current boot path calls `model_registry::init_global() → &'static Registry` at `ipc/mod.rs:705`. We need either:

- **(A)** Add `model_registry::global_arc() → Arc<Registry>` (clones the inner state once). Tiny change, idiomatic.
- **(B)** Refactor `build_profile` to take `&Registry`. Larger change; touches slice-5 signature + every caller.

**Recommendation:** (A). The global registry is a singleton on a `'static`; wrapping it in an `Arc` once at boot is cheap. The `Arc` carries through every layer that needs it without signature changes.

### 2. Where does `hw_capability: HwCapabilityTier` come from?

The substrate doesn't currently call `HostCapabilityProbe::detect()` at IPC boot. The closest existing primitive is `GpuMemoryManager::detect()` (which IS called for the AIProvider module). We need to either:

- **(A)** Add `HostCapabilityProbe::detect_at_boot()` — runs the hw_probe sequence, returns a `(HwCapabilityTier, HwTierCategory)` pair the spawner needs.
- **(B)** Synthesize from `gpu_manager` we already have: pass through `gpu_manager.gpu_name()` → infer tier.

**Recommendation:** (A). The hw_probe is the substrate's source of truth for tier classification; reusing the GPU-name heuristic is a duplicate-of-(A) trap (the compression principle from CLAUDE.md). One probe at boot, one tier classification.

### 3. Where do `hosted_handles` go for shutdown?

The boot task ends after spawning all handles. Without somewhere to send them, `.abort()` on server-stop is impossible (handles drop → tasks detach → wire subscriptions leak until the runtime Arc drops elsewhere).

**Options:**

- **(A)** A new `PersonaSupervisor` module stored on `runtime` (the IPC `ServiceModuleRuntime`). Owns the handles. On runtime shutdown, calls `.abort()` on each.
- **(B)** Hand the handles to `PersonaAircRuntimeRegistry::remove(persona_id)` — make `remove` await-and-abort the associated task before returning. Tighter coupling; clean call site.
- **(C)** Spawn a watcher task that joins all handles and exits when they all complete; sufficient for the "personas live as long as the server lives" case; doesn't help with selective abort.

**Recommendation:** (A) for slice 13 — the supervisor lives at the same lifetime as `PersonaInstanceManagerModule` and owns "who's running right now." Slice 14+ can integrate (B) once the supervisor exposes its abort/respawn API.

### 4. ResumeOrMintProvider count + role mapping

Today's boot calls `ResumeOrMintProvider::new(&root, 1)` — minimum 1 persona. The plan from `PersonaSpawnerModule::plan_for_tier()` currently returns 2 entries (Helper + Coder both on LCD per slice 7). So provider count becomes `plan.len()`.

The deeper question: **role identity vs persona identity**. ResumeOrMintProvider yields `PersonaIdentityIntent { persona_id, agent_name, source }` — no role field. Slice 13 pairs the Nth intent with the Nth plan entry by position. That's reasonable for the LCD case (every persona at this tier is helper/coder) but doesn't survive once roles diverge (Sentinel runs on different hardware than Helper).

**Slice 14 work:** RoleAwareProvider that yields `(PersonaIdentityIntent, RoleId)` pairs, possibly reading the role from seed.json. Out of scope here.

### 5. Error policy for per-slot failures

Today's loop logs and continues. Per [[no-fallbacks-ever]] this is correct — the substrate refuses to substitute a default persona for a failed slot. But the boot path should at least surface a structured count: "tier planned N, M failed."

**Recommendation:** publish a `BootSummary { planned, hosted, failed: Vec<(slot, role, error)> }` event via the existing event bus. Operators see what happened without scraping logs. Per [[observability-is-half-the-architecture]].

---

## Test plan for slice 13

Existing slice 5–12 tests cover the composition pieces. New tests for slice 13:

1. **Mock-bootstrap integration test**: stub `PersonaInstanceManagerModule` via a trait split (similar to how slice 9 introduced `PersonaAdapterFactory`). Verify the boot loop calls bootstrap N times, materializes adapters N times, spawns N handles. Slice 13 introduces a `PersonaBootstrapper` trait for this — modest scope.
2. **Provider-exhausted test**: ResumeOrMintProvider yields M < N intents. Verify boot logs the deficit and proceeds with M handles, not N. Already exercised in slice 8's `bootstrap_planned_exhausted_provider_errors_with_slot_info` — slice 13's test just wraps that path.
3. **Per-slot adapter-failure test**: stub factory rejects slot 1. Verify slot 0 + slot 2 still spawn; slot 1 logs structured error.

Integration test happens through the IPC server itself (no stub airc daemon available). Slice 13 lands behind a `cfg(test)` guard that lets us inject the stubs at the boot composition point.

---

## What slice 13 does NOT do

- **Adapter pool sharing (#122 shared-base + LoRA paging)**: slice 13 builds one adapter per persona via `LlamaCppPersonaAdapterFactory`. On Intel Mac LCD this means 2 × Qwen2.5-0.5B GGUF loads. With Q4_K_M at ~468 MiB each, fine (~1 GiB). Shared base is #122's territory, not slice 13's.
- **AircRemoteInferenceAdapter integration (#108 slice D/E)**: slice 13 routes every persona through the local adapter. Cross-grid inference is its own thread.
- **Per-persona LoRA selection**: today's profile carries `model_id` but not a LoRA reference. #124 IdentityProjector + #126 first-connection ceremony land that.
- **Role-aware provider**: noted in open question 4.

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
- [[substrate-is-a-good-citizen-on-the-host]] — caller controls scheduling pool
- [[observability-is-half-the-architecture]] — BootSummary as a first-class event
- [[organization-purity-as-we-migrate]] — slice 13 deletes the welcome-log-only path; doesn't keep both
- [[constitutional-design-always-a-next-step]] — every open question has a recommendation
- [[commands-are-dumb-daemons-are-smart]] — `spawn_persona_service` stays trivial; the smart roster reconciliation lives in the supervisor
