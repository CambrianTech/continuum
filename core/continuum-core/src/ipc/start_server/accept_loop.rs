//! Boot phase 20: the blocking accept loop over the primary IPC endpoint (a Unix socket, or the
//! Windows TCP loopback listener; `continuum_host::core_ipc` holds the difference).

use super::*;

/// Accept connections until the endpoint closes, each served on its own thread. Every caller is
/// local (a Unix socket, or loopback TCP on Windows), so callers are stamped `None`:
/// owner-by-locality.
pub(super) fn serve_connections(
    state: Arc<ServerState>,
    listener: continuum_host::core_ipc::Listener,
) -> std::io::Result<()> {
    loop {
        match listener.accept() {
            Ok(stream) => {
                let state = state.clone();
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, state, None) {
                        log_error!("ipc", "server", "Client error: {}", e);
                    }
                });
            }
            Err(e) => {
                log_error!("ipc", "server", "Connection error: {}", e);
            }
        }
    }
}
