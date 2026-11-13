//! Boot phase 15: verify the registered set and START every module's initialize. It returns the
//! settle instead of waiting for it, so the socket binds while modules initialize: no module can
//! wedge boot (pattern 2 of docs/architecture/DEGRADE-NEVER-REFUSE.md; the M5 wedge of
//! 2026-11-07 19:11Z, where one module's relock kept the socket unbound until the 300 s boot
//! deadline restarted the node into the same wedge). Phase 18 ([`settle_then_release`]) runs
//! after the bind: tick loops and everything that waits on a settled runtime.

use super::*;
use crate::runtime::{InitPlan, InitRecordStore, InitSettled};
use std::future::Future;
use std::pin::Pin;

pub(super) struct InitializeStarted {
    /// When the bind-side phase rows are timed from (initialize started).
    pub(super) phase_started: std::time::Instant,
    /// Resolves when every module has a first verdict: Ready, Failed or Degraded.
    pub(super) settle: Pin<Box<dyn Future<Output = InitSettled> + Send>>,
}

pub(super) fn verify_and_begin_initialize(
    runtime: &Arc<Runtime>,
    rt_handle: &tokio::runtime::Handle,
    discovery: &crate::airc::AircDiscovery,
    boot_mode: crate::runtime::BootMode,
    phase_started: std::time::Instant,
) -> std::io::Result<InitializeStarted> {
    continuum_base::probe!(
        class = "boot.phase",
        phase = "construct_modules",
        ms = phase_started.elapsed().as_millis() as u64,
        "pre-bind phase"
    );

    // Verify the required-module set for THIS (discovery, mode) pair. A registration fact, so it
    // is checked before the bind: a missing module is a genuine pre-bind refusal (the start_server
    // Err exits non-zero with the repair in the message). A.2 makes the set conditional —
    // `persona_instance_manager` and `persona-rag-inspect` are required only when AIRC is
    // `Healthy` AND the mode hosts personas.
    // A module whose command wiring the registry refused (a duplicate name, a constructor
    // without its descriptor) refuses boot here, before any module initializes or any
    // client is told the catalogue.
    runtime.registry().check_wiring().map_err(std::io::Error::other)?;
    if let Err(e) = runtime.verify_registration(discovery, boot_mode) {
        log_error!("ipc", "server", "{}", e);
        return Err(std::io::Error::other(e));
    }

    // The signal handlers' graceful stop is safe before init settles: the stop skips any module
    // whose initialize has not returned (`ModuleStopOutcome::NeverInitialized`), so a stop during
    // boot saves the modules that are ready and never overwrites state with an unloaded module's.
    crate::runtime::install_signal_shutdown(runtime.clone());

    // The init deadline derives from the slowest healthy init this node recorded on its last boot
    // (`state/module-init.json`, rewritten by this boot); with no record it is the typed backstop.
    let home = crate::paths::continuum_home().map_err(|e| {
        std::io::Error::other(format!("the continuum home must resolve before modules initialize: {e}"))
    })?;
    let plan = InitPlan::from_record(InitRecordStore::under_home(&home));
    let settle = {
        let _rt_guard = rt_handle.enter();
        runtime.begin_initialize(plan)
    };
    Ok(InitializeStarted { phase_started: std::time::Instant::now(), settle: Box::pin(settle) })
}

/// Boot phase 18, spawned after the bind: wait for every module's first verdict, then start the
/// tick loops (a degraded module starts its own when its initialize returns) and release what
/// waits on a settled runtime: the desktop, the operator peer, the dream consolidator and the
/// persona host. The socket already answers; a command for a module still initializing waits
/// for it (bounded by the deadline) and a degraded module's commands are refused by type.
pub(super) fn settle_then_release(
    rt_handle: &tokio::runtime::Handle,
    runtime: Arc<Runtime>,
    settle: Pin<Box<dyn Future<Output = InitSettled> + Send>>,
    executor: Arc<crate::runtime::CommandExecutor>,
    operator_peer_socket: Option<std::path::PathBuf>,
    persona_supervisor_executor_ready_tx: Option<
        tokio::sync::oneshot::Sender<Arc<crate::runtime::CommandExecutor>>,
    >,
) {
    let handle = rt_handle.clone();
    rt_handle.spawn(async move {
        let settled = settle.await;
        continuum_base::probe!(
            class = "boot.phase",
            phase = "initialize",
            ms = settled.ms,
            "every module has a first verdict (initialize now runs beside the bound socket)"
        );
        if let Err(summary) = settled.into_result() {
            log_error!("ipc", "server", "Runtime initialization: {}", summary);
        }
        // Tick loops run as tokio tasks; a handle dropped here detaches, never aborts.
        drop(runtime.start_tick_loops());
        log_info!(
            "ipc",
            "server",
            "Modular runtime settled with {} modules: {:?}",
            runtime.registry().list_modules().len(),
            runtime.registry().readiness().report()
        );
        command_executor::release_executor_dependents(
            &handle,
            &executor,
            operator_peer_socket,
            persona_supervisor_executor_ready_tx,
        );
    });
}
