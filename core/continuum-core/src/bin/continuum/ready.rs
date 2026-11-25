//! A started core is serving, or it is gone: two events, no clock.
//!
//! `start` used to poll every 2 s (ping, log tail, CPU of the build's process group) against a
//! 5-minute stall and a 30-minute ceiling, because it might be running a build. A core launched
//! directly needs none of that. The old cores' exits and the new core's are the kernel's events
//! (`continuum_host::process_exit::exited`); its socket appearing is the filesystem's (the core
//! binds it once its command surface is registered, `ipc/start_server.rs`, and a connect after the
//! bind waits in the backlog until the accept loop answers). So `start` waits on those and nothing
//! else: the core is up the moment it answers, and a core that dies says so the moment it does.

use std::path::Path;

use continuum_host::process_exit::exited;
use notify::{Event, RecursiveMode, Watcher};

/// How a launched core's start ended.
pub(super) enum Started {
    /// It answered a ping on its socket.
    Serving,
    /// It exited before it served.
    Exited,
    /// Its socket's directory could not be watched, so readiness has no event to wait on.
    Unwatchable(String),
}

/// Wait until every core in `old` has exited, then until the core `pid` serves on `socket` or exits.
pub(super) async fn serving_or_exited(pid: u32, socket: &Path, old: &[i32]) -> Started {
    // The ping that counts is the new core's: the old ones leave first.
    futures::future::join_all(old.iter().filter_map(|p| u32::try_from(*p).ok()).map(exited)).await;
    let Some(dir) = socket.parent() else {
        return Started::Unwatchable(format!("{} has no parent directory", socket.display()));
    };
    let (tx, mut changed) = tokio::sync::mpsc::unbounded_channel::<()>();
    let target = socket.to_path_buf();
    let watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.is_ok_and(|event| event.paths.iter().any(|path| *path == target)) {
            let _ = tx.send(()); // the receiver is gone only once start has its answer
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => return Started::Unwatchable(error.to_string()),
    };
    if let Err(error) = watcher.watch(dir, RecursiveMode::NonRecursive) {
        return Started::Unwatchable(error.to_string());
    }
    let died = exited(pid);
    tokio::pin!(died);
    loop {
        // The socket may already stand (the core bound it before the watch began): ask it first.
        if socket.exists() {
            tokio::select! {
                _ = &mut died => return Started::Exited,
                answered = answers() => if answered { return Started::Serving },
            }
        }
        tokio::select! {
            _ = &mut died => return Started::Exited,
            event = changed.recv() => if event.is_none() {
                return Started::Unwatchable("the socket watch ended".to_string());
            },
        }
    }
}

/// One ping on the core's socket, answered or refused, with no budget of its own: the caller races
/// it against the core's exit.
async fn answers() -> bool {
    let conn = super::core_conn::connection();
    let cmds = conn.commands();
    cmds.execute_value("ping", serde_json::Value::Object(Default::default())).await.is_ok()
}
