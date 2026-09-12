//! The airc daemon this core spawned — owned, probed in-process, restartable.
//!
//! Before 2026-09-12 the boot asked `airc ipc-endpoint` (a CLI fork that prints a
//! path and proves nothing about liveness), spawned `airc daemon`, dropped the child
//! handle, and never looked again. When the descriptor table filled that night the
//! only recovery was a human killing and restarting the daemon by hand. Now the core
//! records the pid it spawned, answers "is it answering" by connecting to the socket
//! it resolves itself, and can restart the daemon it owns — the action the
//! [`FdPressurePool`](crate::system_resources::fd_pressure::FdPressurePool) takes.
//! A daemon someone else started is never killed: [`Restart::NotOurs`] is a named
//! outcome, not a silent no-op.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// The pid of the daemon THIS process spawned; 0 = none (adopted or absent).
static OWNED_PID: AtomicU32 = AtomicU32::new(0);

/// The machine-account scope the daemon serves: `<home>/.airc`, the scope
/// `airc daemon` run from `<home>` binds (the CLI's own default resolution).
pub fn scope_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|h| PathBuf::from(h).join(".airc"))
}

/// Where the machine daemon binds, as airc itself resolves it. `airc ipc-endpoint` is
/// resolve-only (no daemon required, starts nothing) and is the contract continuum's
/// discovery already depends on. Resolved once and cached; a failed resolution is not
/// cached, so a box where airc lands later resolves on the next tick.
///
/// 2026-09-12: this probe used `daemon_endpoint::default_socket_path_in`, a derivation
/// deprecated for DRIFTING from airc's resolver (`/tmp/airc-ipc-v5-<hash>.sock` vs the
/// real `~/.airc/runtime/airc-machine-<hash>-v5.sock`). It never saw any daemon: the boot
/// step reported "spawned but never answered" at every boot, the fd owner restarted a
/// daemon it could not see, and the liveness owner spawned a second daemon every
/// back-off while the first one answered fine.
pub fn socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(crate::airc::discovery::AIRC_DAEMON_SOCKET_ENV) {
        return Some(PathBuf::from(path));
    }
    static RESOLVED: Mutex<Option<PathBuf>> = Mutex::new(None);
    if let Ok(guard) = RESOLVED.lock() {
        if let Some(path) = guard.as_ref() {
            return Some(path.clone());
        }
    }
    let probed = crate::system_resources::bounded_command::probe(
        "airc",
        &["ipc-endpoint"],
        ENDPOINT_RESOLVE_BOUND,
    );
    let path = resolved_endpoint(probed.stdout_if_ok())?;
    if let Ok(mut guard) = RESOLVED.lock() {
        *guard = Some(path.clone());
    }
    Some(path)
}

/// Bound on `airc ipc-endpoint` (a pure print; anything slower is a wedged binary).
pub const ENDPOINT_RESOLVE_BOUND: Duration = Duration::from_secs(5);

/// The pure half of the resolution: the resolver's stdout → a path, or nothing when
/// the resolver said nothing usable (an empty line is not a socket).
pub fn resolved_endpoint(stdout: Option<&str>) -> Option<PathBuf> {
    let line = stdout?.lines().last()?.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// Does a daemon answer on the machine socket right now? `false` covers "no socket
/// path could be resolved" — the spawn path names that case.
pub fn answering() -> bool {
    let Some(path) = socket_path() else { return false };
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(&path).is_ok()
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spawned {
    /// A daemon was already answering; nothing spawned.
    Answering,
    /// Spawned and answering within the bound.
    Started { pid: u32 },
    /// The `airc` binary is not on PATH — a transportless box (CI, a fresh clone).
    BinaryAbsent,
    /// Neither USERPROFILE nor HOME is set: the machine-account scope is unresolvable.
    NoHome,
    /// Spawned (or failed to) and never answered within the bound.
    Failed(String),
}

/// How long a freshly spawned daemon gets to answer. Cold stores take seconds
/// (a 5 s sqlite pool acquire was measured on the busy M5 store).
pub const ANSWER_BOUND: Duration = Duration::from_secs(15);

/// Spawn the daemon from the scope's home (never the repo cwd — the ownership
/// guard refuses a socket served under the wrong identity) and wait for it to
/// answer. Idempotent: an answering daemon is left alone.
pub fn spawn() -> Spawned {
    if answering() {
        return Spawned::Answering;
    }
    let Some(scope) = scope_home() else { return Spawned::NoHome };
    let Some(home) = scope.parent().map(|p| p.to_path_buf()) else { return Spawned::NoHome };
    if socket_path().is_none() {
        return Spawned::Failed(
            "the machine socket path is unresolvable (`airc ipc-endpoint` answered nothing usable) — \
             a daemon could be spawned but never observed"
                .into(),
        );
    }
    let child = std::process::Command::new("airc")
        .arg("daemon")
        .current_dir(&home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let child = match child {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Spawned::BinaryAbsent,
        Err(e) => return Spawned::Failed(format!("airc daemon spawn: {e}")),
    };
    let pid = child.id();
    OWNED_PID.store(pid, Ordering::SeqCst);
    let deadline = std::time::Instant::now() + ANSWER_BOUND;
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        if answering() {
            return Spawned::Started { pid };
        }
    }
    Spawned::Failed(format!(
        "airc daemon (pid {pid}) spawned but never answered within {}s",
        ANSWER_BOUND.as_secs()
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restart {
    /// The daemon we own was killed and a new one answers.
    Restarted { old: u32, new: u32 },
    /// No daemon of ours to restart (adopted at boot, or absent) — left alone.
    NotOurs,
    /// Killed ours; the replacement did not come up.
    Failed(String),
}

/// Kill the daemon this process spawned and spawn another. Only ours: a daemon we
/// adopted belongs to whoever started it.
pub fn restart_if_owned() -> Restart {
    let old = OWNED_PID.load(Ordering::SeqCst);
    if old == 0 {
        return Restart::NotOurs;
    }
    crate::inference::lane_process::kill9(old);
    OWNED_PID.store(0, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(500));
    match spawn() {
        Spawned::Started { pid } => Restart::Restarted { old, new: pid },
        Spawned::Answering => Restart::Restarted { old, new: 0 },
        other => Restart::Failed(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a restart killing a daemon we did not spawn (the adopted
    // case must be a named NotOurs), and the scope resolving to something other
    // than the CLI's machine-account home.
    // what this catches: the probe reading a resolver's silence as a socket (an empty
    // line became `PathBuf::from("")`, which never answers — the 2026-09-12 shape).
    #[test]
    fn the_endpoint_is_the_resolvers_last_nonempty_line_or_nothing() {
        assert_eq!(
            resolved_endpoint(Some("/Users/x/.airc/runtime/airc-machine-ab-v5.sock\n")),
            Some(PathBuf::from("/Users/x/.airc/runtime/airc-machine-ab-v5.sock"))
        );
        assert_eq!(resolved_endpoint(Some("\n")), None);
        assert_eq!(resolved_endpoint(Some("")), None);
        assert_eq!(resolved_endpoint(None), None);
    }

    #[test]
    fn a_daemon_we_did_not_spawn_is_never_restarted() {
        OWNED_PID.store(0, Ordering::SeqCst);
        assert_eq!(restart_if_owned(), Restart::NotOurs);
        if let Some(scope) = scope_home() {
            assert!(scope.ends_with(".airc"), "{}", scope.display());
        }
    }
}
