//! Boot phase 1: prepare the socket path and load the model registry
//! BEFORE any ServiceModule is constructed.

use super::*;

/// Returns the instant the pre-bind `construct_modules` phase is timed from.
pub(super) fn prepare_socket_and_load_model_registry(
    socket_path: &str,
    boot_mode: crate::runtime::BootMode,
) -> std::io::Result<std::time::Instant> {
    prepare_unix_socket_path(socket_path)?;
    continuum_base::probe!(class = "boot.step", step = "socket_path_prepared", "the socket path is prepared; the model registry scan is next");

    log_info!(
        "ipc",
        "server",
        "Starting IPC server on {} (mode={})",
        socket_path,
        boot_mode.label()
    );

    // (Removed the startup API-key boot check. The local llama-server
    // gateway has no credential — its catalog entry is `auth: None` — so there is
    // no key to resolve or self-heal. Gateway readiness is now expressed by the
    // serving daemon's snapshot, checked at adapter registration, not a key probe.)

    // Load the model_registry BEFORE any ServiceModule is constructed.
    // Several adapters (AnthropicAdapter, LlamaCppAdapter, …) read from
    // `model_registry::global()` in their constructors — if init hasn't
    // happened yet those panic at module registration time. Failure here
    // is fatal: the registry is the single source of truth for model ids
    // and a missing config is a boot-order / packaging bug, not a runtime
    // condition we can recover from.
    let registry_started = std::time::Instant::now();
    let registry_init = crate::model_registry::init_global();
    continuum_base::probe!(
        class = "boot.registry_init",
        outcome = if registry_init.is_ok() { "ok" } else { "error" },
        ms = registry_started.elapsed().as_millis() as u64,
        "model registry scan (pre-bind; header reads + size walk)"
    );
    // BOOT PHASES, each with a name: the additive model "pre-bind ≈ registry +
    // module init" was refuted by measured time-to-answer on two hosts
    // (IntelMac, 2026-09-04) — the third thing had no probe. Now every
    // pre-bind step reports its wall time: construction of the 59 modules,
    // initialize, load_state, bind.
    let phase_started = std::time::Instant::now();
    match registry_init {
        Ok(reg) => {
            log_info!(
                "ipc",
                "server",
                "model_registry loaded: {} models across {} providers",
                reg.models().count(),
                reg.providers().count()
            );
            // A degraded model is REPORTED, never silent (#63). Boot survives a bad
            // artifact now, so the only way an operator learns their model is unusable is
            // if we say it — once per model, with the parser's own words, at the seam that
            // decided it. Silence here would trade a loud crash for a quiet lie.
            // NOT log_warn! — this line fires BEFORE any ServiceModule exists (see the
            // comment above: the registry loads first on purpose), and every file-logging
            // path is dead at that instant. Measured, three silent drops deep:
            //   1. `log_*` needs LOGGER, whose only production initializer is ffi/mod.rs
            //      (the legacy Node-embedding entry) — never called on the native server.
            //   2. `clog_*` needs GLOBAL_LOG_SENDER, set by
            //      LoggerModule::new() — which has not been constructed yet here.
            //   3. even once set, `queue_log` try_sends into a bounded channel and drops
            //      silently when full.
            // I wrote this warning with log_warn!, could not find it anywhere, and only
            // then learned the whole chain was dark. A degradation nobody can see is
            // exactly the bug this fix exists to kill, so it uses the sink that needs no
            // initialization and demonstrably reaches the log file at boot.
            for (id, why) in reg.unhydratable() {
                tracing::warn!(
                    model_id = %id,
                    reason = %why,
                    probe_class = "registry.model.unhydratable",
                    "model is UNAVAILABLE — its artifact did not hydrate. The core is up \
                     and every other model is usable; this one cannot be served or planned \
                     against until the artifact or the reader is fixed."
                );
            }
        }
        Err(e) => panic!("failed to load model_registry: {e}"),
    }
    Ok(phase_started)
}
