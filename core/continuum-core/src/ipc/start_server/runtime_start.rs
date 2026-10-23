//! Boot phase 14: initialize every module, load state, install the shutdown
//! handlers, start the tick loops and verify the required-module set.

use super::*;

pub(super) struct RuntimeStarted {
    /// When the bind-side phase rows are timed from (post-initialize).
    pub(super) post_init_started: std::time::Instant,
    /// Held by the boot thread for the life of the server.
    pub(super) tick_handles: Vec<tokio::task::JoinHandle<()>>,
}

pub(super) fn initialize_and_start_runtime(
    runtime: &Arc<Runtime>,
    rt_handle: &tokio::runtime::Handle,
    discovery: &crate::airc::AircDiscovery,
    boot_mode: crate::runtime::BootMode,
    phase_started: std::time::Instant,
) -> std::io::Result<RuntimeStarted> {
    // Initialize modules (runs async init in sync context)
    continuum_base::probe!(
        class = "boot.phase",
        phase = "construct_modules",
        ms = phase_started.elapsed().as_millis() as u64,
        "pre-bind phase"
    );
    let phase_started = std::time::Instant::now();
    rt_handle.block_on(async {
        if let Err(e) = runtime.initialize().await {
            log_error!("ipc", "server", "Runtime initialization failed: {}", e);
        }
        // The CBAR lifecycle pair (2026-09-02), coded ONCE at the one trait +
        // this one broadcast site: every module loads its state after init,
        // and the SIGNAL handlers run the parallel save-and-join broadcast
        // before exit — a stop finally saves, a boot finally loads, both
        // receipted per node (boot.load_state / shutdown.step).
        continuum_base::probe!(
            class = "boot.phase",
            phase = "initialize",
            ms = phase_started.elapsed().as_millis() as u64,
            "pre-bind phase"
        );
        let load_started = std::time::Instant::now();
        runtime.load_all_state().await;
        continuum_base::probe!(
            class = "boot.phase",
            phase = "load_state",
            ms = load_started.elapsed().as_millis() as u64,
            "pre-bind phase"
        );
    });
    let phase_started = std::time::Instant::now();
    crate::runtime::install_signal_shutdown(runtime.clone());

    // Start periodic tick loops for modules that declare a tick_interval.
    // Replaces TypeScript's per-persona setIntervals (task polling, self-task gen, training checks).
    // Tick loops run as tokio tasks — they're lightweight and don't block the IPC thread.
    let _tick_handles = rt_handle.block_on(async { runtime.start_tick_loops() });

    // Verify the required-module set for THIS (discovery, mode) pair.
    // A.2 makes the set conditional — `persona_instance_manager` and
    // `persona-rag-inspect` are only required when AIRC is `Healthy`
    // AND mode requires persona hosting. Slice A's flat list put
    // those in the unconditional set, which broke `--mode=inference-only`
    // (R1#1 BLOCK). Conditional dispatch eliminates the contradiction
    // structurally.
    if let Err(e) = runtime.verify_registration(discovery, boot_mode) {
        log_error!("ipc", "server", "{}", e);
        return Err(std::io::Error::other(e));
    }

    log_info!(
        "ipc",
        "server",
        "Modular runtime ready with {} modules: {:?}",
        runtime.registry().list_modules().len(),
        runtime.registry().list_modules()
    );
    Ok(RuntimeStarted {
        post_init_started: phase_started,
        tick_handles: _tick_handles,
    })
}
