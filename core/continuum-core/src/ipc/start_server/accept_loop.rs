//! Boot phase 20: the blocking accept loop (Unix socket, or the Windows TCP
//! loopback listener that is the primary IPC there).

use super::*;

#[cfg(unix)]
pub(super) fn serve_unix_connections(
    state: Arc<ServerState>,
    listener: UnixListener,
) -> std::io::Result<()> {
    // Accept connections (event-driven - sleeps until connection).
    //
    // Unix: block on the Unix-domain socket accept loop. Unix socket = LOCAL
    // caller (the operator on the box) → None = owner-by-locality.
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();

                // Spawn thread for concurrent handling.
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
    Ok(())
}

// Windows: no Unix-domain sockets. Bind a TCP loopback listener as the
// PRIMARY IPC and block on its accept loop. Loopback == the local operator,
// so callers are stamped `None` (owner-by-locality), mirroring the Unix
// socket's trust semantics — local jtag/SDK clients connect over
// 127.0.0.1:<port> and retain owner privileges. Host from CONTINUUM_CORE_BIND
// (default 127.0.0.1 — loopback only), port from CONTINUUM_CORE_TCP
// (default 9100). BEHAVIORAL NOTE: local Windows clients must connect over
// this TCP port instead of a Unix socket path.
#[cfg(windows)]
pub(super) fn serve_windows_tcp_connections(
    state: Arc<ServerState>,
    phase_started: std::time::Instant,
) -> std::io::Result<()> {
    let bind_host =
        std::env::var("CONTINUUM_CORE_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = endpoint_paths::core_tcp_port();
    let bind_addr = format!("{bind_host}:{port}");
    // The same two phase rows the Unix path emits around ITS bind. They were
    // `#[cfg(unix)]`-gated with the Unix bind (the windows-msvc fix on #3714),
    // so on Windows the stretch after load_state — where BigMama's boot spent
    // 108 unattributed seconds and used to die — had no row at all (2026-09-05).
    crate::probe!(
        class = "boot.phase",
        phase = "post_init_to_bind",
        ms = phase_started.elapsed().as_millis() as u64,
        "pre-bind phase (Windows TCP loopback)"
    );
    let bind_started = std::time::Instant::now();
    let listener = TcpListener::bind(&bind_addr)?;
    crate::probe!(
        class = "boot.phase",
        phase = "bind",
        ms = bind_started.elapsed().as_millis() as u64,
        "the bind call itself — the TCP listener exists after this"
    );
    log_info!(
        "ipc",
        "server",
        "IPC server ready on TCP {} (Windows primary transport — no Unix-domain sockets)",
        bind_addr
    );
    for stream in listener.incoming() {
        match stream {
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
    Ok(())
}
