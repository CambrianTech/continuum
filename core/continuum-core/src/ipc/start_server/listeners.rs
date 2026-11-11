//! Boot phase 16: bind the primary IPC endpoint and announce IPC readiness.

use super::*;

/// Bind the primary IPC endpoint through `continuum_host::core_ipc`: the Unix-domain socket at
/// `socket_path` (world read-write, #1008), or on Windows the TCP loopback listener at
/// CONTINUUM_CORE_BIND (default 127.0.0.1) and the core's TCP port. The OS difference lives in that
/// capability; this is the same bind on every OS.
pub(super) fn bind_ipc_listener(
    socket_path: &str,
    phase_started: std::time::Instant,
) -> std::io::Result<continuum_host::core_ipc::Listener> {
    // Everything between initialize's START and the bind call (initialize no longer has to end
    // first: it settles beside the bound socket). The row is named for what it measures
    // (IntelMac's review of #3714), and the bind itself gets its own instant. Before this was one
    // bind, the Windows path had no row at all for the stretch where BigMama's boot spent 108
    // unattributed seconds (2026-09-05).
    continuum_base::probe!(
        class = "boot.phase",
        phase = "init_started_to_bind",
        ms = phase_started.elapsed().as_millis() as u64,
        "pre-bind phase"
    );
    let bind_host = std::env::var("CONTINUUM_CORE_BIND").unwrap_or_else(|_| "127.0.0.1".to_string()); // unwrap_or_else: unset = loopback only, the endpoint's documented default
    let tcp_addr = format!("{bind_host}:{}", endpoint_paths::core_tcp_port());
    let endpoint = continuum_host::core_ipc::Endpoint { socket_path: std::path::Path::new(socket_path), tcp_addr: &tcp_addr };
    let bind_started = std::time::Instant::now();
    let listener = continuum_host::core_ipc::Listener::bind(&endpoint)?;
    continuum_base::probe!(
        class = "boot.phase",
        phase = "bind",
        ms = bind_started.elapsed().as_millis() as u64,
        "the bind call itself: the endpoint exists after this"
    );
    log_info!("ipc", "server", "IPC endpoint bound: {}", listener.describe());
    Ok(listener)
}

pub(super) fn announce_ipc_ready(socket_path: &str) {
    // Socket is bound + world-rw chmod'd. Fire the ready watch so any
    // consumer (main.rs awaiting boot completion, future subsystems
    // wanting to know when IPC is up) observes the false→true edge.
    // The watch supersedes the previous oneshot parameter — any number
    // of late or eager subscribers can attach via `subscribe_ready()`.
    let _ = crate::runtime::ipc_ready::ipc_ready_tx().send(true);
    continuum_base::probe!(
        class = "ready.observed",
        module = "ipc-server",
        socket = socket_path
    );
}
