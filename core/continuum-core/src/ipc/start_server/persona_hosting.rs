//! Boot phase 11b: plan the resident roster from the serving daemon, build
//! the PersonaSpawnSupervisor, and spawn the hosting task that waits for the
//! wired executor.

use super::*;

/// Returns the sender the boot thread fires once the wired executor exists.
pub(super) fn start_persona_host(
    runtime: &Arc<Runtime>,
    serving_daemon: &Arc<crate::modules::serving_daemon::ServingDaemonModule>,
    instance_manager: &Arc<crate::modules::persona_instance_manager::PersonaInstanceManagerModule>,
    airc_interceptor_cell: &Arc<tokio::sync::OnceCell<Arc<airc_lib::Airc>>>,
    rt_handle: &tokio::runtime::Handle,
    stretch_mark: &mut impl FnMut(&'static str, &'static str),
) -> tokio::sync::oneshot::Sender<Arc<crate::runtime::CommandExecutor>> {
    // The Grid's first heartbeat at server boot: resume any
    // existing citizens from disk + ensure at least one is
    // present. ResumeOrMintProvider scans
    // `<continuum_root>/personas/*/seed.json`; for each parsed
    // seed it yields a ResumedFromDisk intent (airc-lib will load
    // the existing keypair from identity.key when bootstrap runs
    // — same persona, same peer_id, across restarts). If no
    // citizens are on disk, it floor-mints one fresh per the
    // `min_personas = 1` policy below.
    //
    // Fired as an async task off the IPC bootstrap thread so the
    // server-ready signal isn't blocked on daemon round-trips.
    // Failure of any single bootstrap is non-fatal — log + move
    // on; the operator can re-fire via the
    // `persona/instances/bootstrap` command once the underlying
    // issue (disk full, daemon down, corrupted seed) is resolved.
    // ── Slice 13: substrate-managed persona hosting ──────────────
    //
    // Composes slices 7-12 into the production boot path:
    //
    //   PersonaSpawnerModule::plan_for_tier (slice 7)
    //     → bootstrap_planned (slice 8): mints/resumes airc identities
    //     → materialize_adapters (slice 9): builds inference adapters
    //     → spawn_persona_service (slice 12): runs the serve_persona_loop
    //     → PersonaAircRuntimeRegistry::attach_service_loop (slice 13 Q3):
    //       parks the JoinHandle in the slot alongside the runtime
    //
    // BEFORE slice 13: this boot loop called `bootstrap_one` per
    // persona and logged a welcome — the persona was reachable via
    // `airc peers` but never responded. Mute citizens.
    //
    // AFTER slice 13 (this code): each planned persona gets her
    // serve_persona_loop running on rt_handle, with the JoinHandle
    // owned by the registry slot for orderly shutdown.
    //
    // Per the design doc HEADLESS-PERSONA-HOST-LOOP.md (PR #1510):
    //   - P2 (in effect): plan_for_tier returns single Helper until
    //     slice 14 lands role-in-seed.json.
    //   - Q1 (applied): bootstrap_planned takes &Registry.
    //   - Q3 (applied): single registry keyspace owns runtime +
    //     service loop.
    //   - P1 (deferred): tokio::signal::ctrl_c wiring lands in a
    //     follow-up alongside the Runtime::shutdown caller.
    //     Slot-level shutdown_slot is available via the registry
    //     and exercised by the persona/instances/* IPC commands.
    //   - Q2 (deferred): detect_host_capability needs a production
    //     GpuMonitor constructor that doesn't exist yet (see TODO
    //     below). For now slice 13 uses CpuOnly + Compat, which
    //     produces the LCD Qwen2.5-0.5B Helper for all tiers. When
    //     the GpuMonitor construction lands, swap the hardcoded
    //     tier for `detect_host_capability(&gpu_monitor, &system_info)`.
    //   - P3 (deferred): ResourceBroker.acquire admission for each
    //     adapter spawn is its own slice. Current LCD case (1
    //     persona × ~500 MiB GGUF) is well within all supported
    //     tiers' headroom; broker admission becomes load-bearing
    //     when multi-persona returns in slice 14.
    // Slice 13.5: the persona spawn pipeline lives in
    // `PersonaSpawnSupervisor` now. IPC boot just constructs
    // it and calls `spawn_all` — every previously-inline
    // composition step (bootstrap_planned, materialize_adapters,
    // spawn_persona_service, attach_service_loop, orderly
    // drain, BootSummary) is encapsulated in the supervisor and
    // unit-testable in isolation.
    //
    // TODO #52: replace `CpuOnly + Compat` with
    // `detect_host_capability(&gpu_monitor, &system_info)` once
    // a production GpuMonitor constructor exists.
    // The serving daemon is the single hardware authority: it detects the
    // tier (drives n_gpu_layers — retires the hardcoded CpuOnly+Compat of
    // TODO #52) AND decides the model + lanes from an honest budget +
    // on-disk footprints with GPU-residency. The spawner obeys the plan —
    // no hardcoded tier/model/lanes. (Supersedes the #1645 tier-clamp fix.)
    // Glass-box: surface the artifact cache state at boot so the real provisioning
    // picture (what weights/avatars are present, how much disk) is visible without
    // a debugger ([[never-blind-feedback-driven-iteration]]). First live use of the
    // provisioning system on the running core. `eprintln!` (not the category logger)
    // so a boot summary is UNCONDITIONALLY visible in the server log, regardless of
    // which log categories the operator has enabled.
    eprintln!(
        "📦 artifact cache at boot: {}",
        crate::provisioning::Provisioner::default().cache_report()
    );
    let (hw_cap, tier_cat, tier_id) = serving_daemon.detected_tier();
    // Persona floor — how many citizens to host (read early: it is ALSO the
    // serving plan's lane DEMAND, so it must be set before the first
    // compute_plan; see below for the full doc).
    let persona_floor = crate::config_env::read("CONTINUUM_PERSONA_FLOOR")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1);
    // Lanes serve DEMAND: the floor is how many minds need a concurrent
    // lane. Without this, the planner maximized lane count and split the KV
    // budget across slots nobody asked for — 2 personas served through 4
    // slots at a quarter-window each (the 2026-07-10 starvation).
    serving_daemon.set_lane_demand(persona_floor as u32);
    // The plan is the single grouped source of truth (model + lanes +
    // host-fit served window). Pass it by reference to the spawner per
    // [[pass-the-model-struct-no-param-hell]] — no destructured loose
    // params, no constant clamps re-derived downstream.
    let serving_plan = serving_daemon.compute_plan();
    match &serving_plan {
        Some(p) if p.fits_on_gpu => {
            tracing::info!(
                base_model = %p.base_model.model_id,
                lanes = p.lanes,
                served_context_window = p.served_context_window,
                resident = p.resident_models,
                tier_id,
                "serving daemon drives persona spawn (model + lanes + served window from ServingPlan)"
            );
        }
        other => {
            tracing::warn!(
                plan = ?other,
                "no GPU-fitting serving plan — spawner falls back to the tier default model, 1 lane, floor window"
            );
        }
    }
    // Persona floor — how many citizens to host. Single source: this one
    // config value drives BOTH the spawner's plan-slot count (via
    // with_population) AND the identity provider's mint floor below, so
    // plan-slots and minted identities stay 1:1. Default 1. Raise it to host
    // a collaborating POPULATION — the two-solver cooperation loop needs ≥2
    // distinct citizens in a room so their coordinated turns become training
    // signal (rooms → recorder → dataset → genome). Config-owned
    // ([[config-env-single-owner]]); once minted, citizens persist + resume
    // even if the floor is later lowered.
    // #430: the resident roster is RECIPE DATA — the default experience's
    // `citizens`, embedded floor + disk overlay. A malformed overlay file
    // refuses loudly and the shipped floor still stands (same #432 arm as
    // the positron projection); the author's actionable error surfaces at
    // `activity/spawn`, which validates the same directory.
    // The recipe's citizens, ONCE: the allocator daemon takes them whole (role +
    // declared requirement, #4271); the spawner takes their roles.
    let resident_citizens = {
        use crate::experience::source::RecipeExperienceSource;
        let overlay_dir = RecipeExperienceSource::overlay_dir(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
        );
        match RecipeExperienceSource::resident_citizens(&overlay_dir) {
            Ok(roles) => roles,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    dir = %overlay_dir.display(),
                    "recipe overlay REFUSED while reading the resident \
                     roster — hosting the EMBEDDED default recipe's \
                     citizens until the named file is fixed or removed \
                     (#430)"
                );
                RecipeExperienceSource::resident_citizens_embedded()
            }
        }
    };
    // THE GRID ALLOCATOR DAEMON (card 10bba591): the one live caller of the pure
    // allocator. Reads this node's published plan and the gossip ledger's offers,
    // seats the roster across the grid, publishes on change — the placement
    // switch moves minds to strictly better seats between turns, the reconciler
    // draws this node's open seats, the health line reports the moves. Same
    // recipe roles the spawner hosts; the plan watch the reconciler parks on.
    // The allocator seats by EVERY activity's citizens (card ccb316a7): the project
    // declares what a coder peer needs; the default roster alone made every mind a
    // helper any model could hold.
    let grid_citizens = {
        use crate::experience::source::RecipeExperienceSource;
        let overlay_dir = RecipeExperienceSource::overlay_dir(
            &crate::modules::persona_instance_manager::resolve_continuum_root(),
        );
        RecipeExperienceSource::grid_citizens(&overlay_dir)
            .unwrap_or_else(|_| RecipeExperienceSource::grid_citizens_embedded())
        // unwrap_or_else: a refused overlay was already reported by the resident read above; the embedded activities stand
    };
    runtime.register(Arc::new(
        crate::modules::grid_allocator::GridAllocatorModule::new(
            serving_daemon.subscribe(),
            grid_citizens,
        ),
    ));
    let resident_roles: Vec<crate::persona::role_template::RoleId> =
        resident_citizens.into_iter().map(|c| c.role).collect();
    stretch_mark("resident_roles_and_overlay", "supervisor_construct");
    stretch_mark(
        "supervisor_construct",
        "resume_task_spawn_and_rest_of_block",
    );
    let supervisor = crate::persona::host::PersonaSpawnSupervisor::new(
        crate::persona::spawner_module::PersonaSpawnerModule::new(hw_cap, tier_cat)
            .with_citizens(resident_roles)
            .with_serving(serving_plan.as_ref())
            .with_population(persona_floor)
            // The boot draw's bound before any live plan: the last SETTLED lanes
            // this host served (card 7c38ff6f) — handed in once, never re-read.
            .with_remembered_lanes(
                // One record, one law (#4255): a collapsed geometry seeds the roster bound no
                // more than it seeds the boot plan.
                crate::modules::served_window_store::load_geometry()
                    .and_then(|g| g.cache_geometry().map(|(_, l)| l)),
            ),
        instance_manager.clone(),
        // Persona reasoning binds to whatever the serving daemon has live,
        // read off its published ServingSnapshot (not a probe of our own).
        // The OpenAI-compatible `/v1` transport gives native function-calling
        // (the persona's HANDS actually fire) for free, vs the in-process
        // llama.cpp adapter which silently dropped tools. Joel 2026-06-21.
        // Wrapped so a persona whose durable override names a `remote_peer` gets
        // an `AircRemoteInferenceAdapter` over the grid instead of a local lane
        // (card 1d2f65e7, slice 2 — slice 1 #3799 made the override durable but
        // inert). A persona with no override, or a LOCAL one, is delegated
        // straight through to the factory below, unchanged.
        //
        // The SAME cell the AircInterceptor gets: it is filled asynchronously by
        // the attach task spawned above, typically well after this line runs. The
        // factory reads it at build_adapter time, never captures it here — an
        // `Option` snapshotted at construction would record "airc absent" forever
        // and refuse every remote persona for the process lifetime.
        std::sync::Arc::new(
            crate::persona::remote_lane_factory::RemoteLaneAdapterFactory::new(
                std::sync::Arc::new(crate::persona::supervisor::ServedModelPersonaAdapterFactory),
                crate::modules::persona_instance_manager::resolve_continuum_root(),
                airc_interceptor_cell.clone(),
            ),
        ),
        tier_id,
        crate::model_registry::global(),
        rt_handle.clone(),
    );
    let continuum_root_for_boot =
        crate::modules::persona_instance_manager::resolve_continuum_root();
    // Round-2 verifier finding on PR #1568: `spawn_all` calls
    // `bootstrap_one` which (since task #222) lazily fetches the
    // process-global CommandExecutor via `executor()`. If the
    // spawned task reached that call BEFORE the IPC thread runs
    // `init_executor_with_interceptors` below, production would
    // panic. The earlier shape only "worked" because
    // `ResumeOrMintProvider::new` does disk I/O that's slower
    // than the IPC thread reaching init_executor — a race, not
    // an ordering guarantee.
    //
    // Structural fix: gate the spawned task on a oneshot that
    // the IPC thread sends AFTER `init_executor_with_interceptors`.
    // The spawn task literally cannot reach `executor()` before
    // the global is initialized — the ordering is enforced by
    // the channel, not by relative I/O latency.
    let (executor_ready_tx, executor_ready_rx) =
        tokio::sync::oneshot::channel::<Arc<crate::runtime::CommandExecutor>>();
    // The serving daemon's published plan is the readiness signal the
    // persona host reacts to (event-driven spawn — see the reconcile loop
    // below). Subscribe BEFORE the spawn so no plan edge is missed while
    // the task waits on the executor-ready oneshot.
    let serving_plan_rx = serving_daemon.subscribe();
    rt_handle.spawn(super::persona_host_reconciler::run_persona_host(
        executor_ready_rx,
        continuum_root_for_boot,
        persona_floor,
        supervisor,
        serving_plan_rx,
    ));
    executor_ready_tx
}
