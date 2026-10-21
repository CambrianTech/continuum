//! Boot phase 4: the serving daemon and the modules that watch or feed it.

use super::*;

pub(super) fn register_serving_daemon(
    runtime: &Arc<Runtime>,
    gpu_manager: &Arc<GpuMemoryManager>,
    system_monitor: &Arc<SystemResourceMonitor>,
    resource_daemon: &Arc<crate::resources::ResourceDaemon>,
    model_catalog: &Arc<crate::model_registry::live::ModelCatalog>,
) -> std::io::Result<Arc<crate::modules::serving_daemon::ServingDaemonModule>> {
    // ServingDaemonModule — the ever-present control loop that decides (and
    // re-decides every tick) how this host serves persona inference: which base
    // model, how many continuous-batching lanes, how many models warm. Budget
    // comes from the LIVE free-memory monitor (organic ebb/flow — drops when a
    // game/build/renderer grabs memory) capped at physical VRAM. It is ONE
    // consumer of the holistic resource budget the PressureBroker arbitrates
    // across inference + TTS/STT + classifier CNNs + renderers; next refinement
    // is negotiating an arbitrated share from the broker rather than reading
    // raw free memory. Registered after the monitor so it can read it.
    // Timed: `boot.construct` bills the whole preceding span to `serving`, so this is the
    // ONLY way to tell how much of that number is actually this constructor (see the span
    // note above). If this reads small while the `serving` row stays huge, the cost is a
    // neighbour's and the row was never about the serving daemon.
    let t_serving = std::time::Instant::now();
    // The pin store is built HERE, once, under the continuum home — the daemon and
    // its pin/unpin commands are handed the handle and never resolve a path of their
    // own (a global resolution let a unit test pin the real machine, 2026-09-16).
    let pin_store = crate::modules::serving_pin_store::ServingPinStore::under_home(
        &crate::paths::continuum_home().map_err(|e| {
            std::io::Error::other(format!("serving pin store needs the continuum home: {e}"))
        })?,
    );
    let serving_daemon = Arc::new(crate::modules::serving_daemon::ServingDaemonModule::new(
        gpu_manager.clone(),
        system_monitor.clone(),
        resource_daemon.clone(),
        model_catalog.clone(),
        pin_store,
    ));
    // Training admission reads governed memory under this gate (aae8af55).
    crate::modules::serving_daemon::LifecycleGate::set_global(serving_daemon.lifecycle_gate());
    crate::probe!(
        class = "boot.stretch",
        stretch = "serving_span",
        step = "serving_daemon_new",
        ms = t_serving.elapsed().as_millis() as u64,
        "pre-bind stretch step"
    );
    // DECLARE OURSELVES TO THE AUTHORITY BEFORE THE PLANNER CAN TICK (#438). The authority
    // budgets serving via `budget_for_replacing` — available PLUS serving's own residency —
    // and that add-back is keyed on serving being a registered consumer. Attaching the planner
    // first left a window where a plan could run with serving unregistered, so its OWN resident
    // model counted as an external squeeze: `usable_gb = 0` on a 53 GiB board. See
    // `declare_to_memory_authority`. These two lines are ORDER-DEPENDENT; do not swap them.
    serving_daemon.declare_to_memory_authority();
    // The lane plan is a MEMORY decision, so it runs on the ONE memory authority's tick,
    // not serving's own (MEMORY-AUTHORITY-DAEMON slice 1b). Serving's tick only reconciles
    // the llama-server to the authority-published plan.
    serving_daemon.register_planner_on_authority_tick();
    runtime.register(serving_daemon.clone());

    // A node with no serveable coder model pulls the ladder FLOOR (the 1.5B, #4154)
    // instead of stranding on unfit on-disk weights — the missing wire that made deleting
    // the 0.5B strand a weak tier (the Intel box, 2026-09-17). Watches serving's snapshot;
    // fetches off its own tick, honoring the host's rate-limit headers.
    runtime.register(Arc::new(
        crate::modules::floor_fetch::FloorFetchModule::new(
            serving_daemon.subscribe_serving(),
            system_monitor.clone(),
        ),
    ));

    // #79: expose the one per-machine resource authority's accounting board as a typed
    // read command (`resources/board`). The daemon owns its background poll + watch
    // snapshot; this thin module wraps the same `Arc<ResourceDaemon>` so the measured
    // per-consumer attributions + drift are readable by an operator, a persona, or a
    // grid peer — the reporting half of "accurate footprint() drift-reporting."
    runtime.register(Arc::new(
        crate::modules::resources_module::ResourcesModule::new(resource_daemon.clone()),
    ));
    Ok(serving_daemon)
}
