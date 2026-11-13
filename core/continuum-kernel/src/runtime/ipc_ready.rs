//! The IPC server's ready edge: a kernel lifecycle fact (the socket is bound and answering)
//! that the IPC layer flips and anything may await, kernel modules included (a disk scan waits
//! for it before its first walk).

/// Ready-edge watch for the IPC server — flips `false → true` exactly
/// once when the Unix socket is bound + chmod'd. Any consumer can
/// `subscribe_ready()` to get a `watch::Receiver<bool>` and await the
/// transition; subscribers added after the flip see the current `true`
/// value on first `borrow_and_update`. Per the substrate concurrency
/// doctrine: signals replace races. Same shape as
/// `bevy_renderer::subscribe_ready` and `ServiceModule::ready_edge`.
static IPC_READY: std::sync::OnceLock<tokio::sync::watch::Sender<bool>> =
    std::sync::OnceLock::new();

pub fn ipc_ready_tx() -> &'static tokio::sync::watch::Sender<bool> {
    IPC_READY.get_or_init(|| tokio::sync::watch::channel(false).0)
}

/// Subscribe to the IPC server's ready edge. Returns a receiver that
/// starts at `false` and flips to `true` exactly once when the Unix
/// socket is bound + world-rw chmod'd. Replaces the prior oneshot
/// parameter that had to be threaded through `start_server` — now any
/// number of consumers can await the same edge without coordination
/// at the call site.
pub fn subscribe_ready() -> tokio::sync::watch::Receiver<bool> {
    ipc_ready_tx().subscribe()
}
