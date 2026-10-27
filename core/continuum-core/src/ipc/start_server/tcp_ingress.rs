//! Boot phase 17: the optional TCP listener for container callers (Unix only;
//! on Windows the TCP loopback listener is the primary IPC).

use super::*;

pub(super) fn spawn_optional_tcp_listener(state: &Arc<ServerState>) {
    // Optional TCP listener — exposes the same IPC protocol over TCP for
    // callers that can't reach a Unix socket (containerized node-server on
    // Mac, where the Unix socket lives on the host outside the Docker VM
    // boundary). Set CONTINUUM_CORE_TCP=<port> (typically 9100) to enable.
    //
    // Bind address: CONTINUUM_CORE_BIND env, default 127.0.0.1 (safe —
    // loopback only). Mac Option B install.sh sets 0.0.0.0 explicitly
    // because Docker Desktop's `host.docker.internal` resolves to the
    // host's docker-bridge IP (~192.168.65.254), NOT to 127.0.0.1 — a
    // loopback-bound listener is unreachable from containers. Binding
    // 0.0.0.0 accepts on the docker bridge; Mac's application firewall
    // blocks LAN inbound for unsigned dev binaries by default, so the
    // exposure stays local in practice. Explicit env-driven choice beats
    // hidden platform detection.
    //
    // Unix socket remains the primary path — same binary, same server state,
    // same handle_client code via the IpcStream trait. TCP is additive.
    //
    // SECURITY (adversarial review 2026-06-21): TCP callers are stamped
    // CallerSource::Tcp → Provisional ceiling, so Owner-gated commands
    // (data/delete, grid/trust, …) are REFUSED — no unauthenticated Owner
    // execution. BUT the Provisional AiSafe surface IS reachable over this socket
    // UNauthenticated: arbitrary `data/list`/`data/query` reads, `chat/send`
    // writes, and `ai/generate` (the intended container use). That is safe on the
    // default loopback bind; binding 0.0.0.0 exposes that surface to anyone on the
    // bridge/LAN. TODO(authenticated-tcp): require a shared secret / signed
    // handshake for the TCP listener (and/or a sub-Provisional read-only ceiling)
    // before relying on a non-loopback bind — pairs with the airc↔grid per-peer
    // trust bridge. Until then: do NOT bind 0.0.0.0 on an untrusted network.
    //
    // Unix-only: this is the containerized-node-server-on-Mac path (Docker VM
    // boundary crossing). On Windows the TCP listener is the PRIMARY IPC (bound
    // in the accept section below), so gating this here prevents a double-bind
    // of CONTINUUM_CORE_TCP.
    if let Ok(tcp_port_str) = std::env::var("CONTINUUM_CORE_TCP") {
        if let Ok(port) = tcp_port_str.parse::<u16>() {
            if port > 0 {
                let bind_host = std::env::var("CONTINUUM_CORE_BIND")
                    .unwrap_or_else(|_| "127.0.0.1".to_string());
                let bind_addr = format!("{}:{}", bind_host, port);
                match TcpListener::bind(&bind_addr) {
                    Ok(tcp_listener) => {
                        log_info!(
                            "ipc",
                            "server",
                            "TCP listener ready on {} (for container callers via host.docker.internal)",
                            bind_addr
                        );
                        let tcp_state = state.clone();
                        std::thread::spawn(move || {
                            for stream in tcp_listener.incoming() {
                                match stream {
                                    Ok(stream) => {
                                        let state = tcp_state.clone();
                                        std::thread::spawn(move || {
                                            // TCP = unauthenticated remote socket →
                                            // stamp a non-owner identity so the
                                            // dispatch boundary ACL-gates it (no Owner
                                            // commands over TCP). peer_id is nil (no
                                            // verified peer); trust comes from the Tcp
                                            // source, not the id.
                                            let caller = Some(crate::routing::CallerIdentity::tcp(
                                                crate::identity::PeerId::from_uuid(
                                                    uuid::Uuid::nil(),
                                                ),
                                            ));
                                            if let Err(e) = handle_client(stream, state, caller) {
                                                log_error!(
                                                    "ipc",
                                                    "server",
                                                    "TCP client error: {}",
                                                    e
                                                );
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        log_error!("ipc", "server", "TCP accept error: {}", e);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log_error!(
                            "ipc",
                            "server",
                            "TCP listener failed to bind {}: {}",
                            bind_addr,
                            e
                        );
                    }
                }
            }
        }
    }
}
