//! Boot phase 16: bind the primary Unix socket and announce IPC readiness.

use super::*;

/// Unix: bind the primary Unix-domain socket (Windows binds its TCP loopback
/// listener in `accept_loop` instead).
#[cfg(unix)]
pub(super) fn bind_unix_listener(
    socket_path: &str,
    phase_started: std::time::Instant,
) -> std::io::Result<UnixListener> {
    // Unix: bind the primary Unix-domain socket. On Windows this is skipped
    // entirely — the server's primary IPC is the TCP loopback listener in the
    // accept section below (Windows has no Unix-domain sockets).
    // Everything between init's end and the bind call: signal handlers and every
    // module's periodic loop start (IntelMac's review of #3714 — the row is named
    // for what it measures, and the bind itself gets its own instant). The
    // `#[cfg(unix)]` sits on the BIND, not on the probe before it: an attribute
    // gates exactly one statement, and gating the probe left the Unix bind
    // compiling on Windows (the windows-msvc check caught it on the rebase).
    crate::probe!(
        class = "boot.phase",
        phase = "post_init_to_bind",
        ms = phase_started.elapsed().as_millis() as u64,
        "pre-bind phase"
    );
    let bind_started = std::time::Instant::now();
    let listener = UnixListener::bind(socket_path)?;
    crate::probe!(
        class = "boot.phase",
        phase = "bind",
        ms = bind_started.elapsed().as_millis() as u64,
        "the bind call itself — the socket exists after this"
    );
    // Make the socket world-rw so callers running under a different UID
    // than the server can connect. Concrete failure (#1008): on Windows
    // WSL2 + Docker Desktop, continuum-core runs as root inside the
    // container and binds the socket; the host-side jtag (running as
    // the WSL user, uid 1000) gets EACCES connecting to the root-owned
    // socket. Mac/Linux dev mode (server + caller both run as the same
    // user) is unaffected. 0o666 is appropriate for an IPC substrate
    // socket that lives in a path the caller can already see — same
    // blast radius as anything reading /tmp. Failing-loud (no `?` here
    // would suppress the error; let it propagate) is intentional per
    // the global "evidence is for the debugger" rule. Caught live by
    // continuum-b69f 2026-05-02 during Carl-OOTB Windows Phase 4.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o666))?;
    }
    Ok(listener)
}

pub(super) fn announce_ipc_ready(socket_path: &str) {
    // Socket is bound + world-rw chmod'd. Fire the ready watch so any
    // consumer (main.rs awaiting boot completion, future subsystems
    // wanting to know when IPC is up) observes the false→true edge.
    // The watch supersedes the previous oneshot parameter — any number
    // of late or eager subscribers can attach via `subscribe_ready()`.
    let _ = ipc_ready_tx().send(true);
    crate::probe!(
        class = "ready.observed",
        module = "ipc-server",
        socket = socket_path
    );
}
