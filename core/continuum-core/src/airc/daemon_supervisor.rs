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

/// Bound on one liveness status round-trip.
pub const ANSWERING_PROBE_BOUND: Duration = Duration::from_secs(2);

/// Does a daemon answer on the machine socket right now? `false` covers "no socket
/// path could be resolved" — the spawn path names that case.
///
/// THE PROBE RUNS THE PRODUCT'S PATH: a `DaemonClient::status` round-trip through the
/// same IPC client every command uses. Until 2026-09-17 the Windows arm was
/// `path.exists()` on a `.sock` path that never exists there (the transport is a
/// named pipe; only the `.sock.lock` is on disk), so a healthy daemon with hours of
/// uptime read "absent" every period and the owner "revived" it every 5 min — 26
/// failed spawns in a row on the 5090, each one a child that could not bind against
/// the daemon that was already answering. The unix arm's raw connect was also
/// weaker than this: a wedged daemon that accepts and never answers read alive.
/// A dedicated thread with its own current-thread runtime, so the probe is callable
/// from every caller this module already has (sync boot, `spawn_blocking`, the
/// fd-pressure pool) without a runtime-in-runtime panic.
pub fn answering() -> bool {
    let Some(path) = socket_path() else { return false };
    let probe = std::thread::Builder::new()
        .name("airc-daemon-answering".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()?;
            rt.block_on(async move {
                airc_ipc::DaemonClient::new(path)
                    .status_with_timeout(ANSWERING_PROBE_BOUND)
                    .await
                    .ok()
            })
        });
    match probe {
        Ok(handle) => handle.join().ok().flatten().is_some(),
        // A thread that cannot be spawned is a starved host, not a dead daemon — but
        // "not answering" is the only honest reading of a probe that could not run.
        Err(_) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spawned {
    /// A daemon was already answering; nothing spawned.
    Answering,
    /// Spawned and answering within the bound.
    Started { pid: u32 },
    /// airc's own login supervisor started it (macOS LaunchAgent): airc owns it, not us.
    StartedByAirc,
    /// Spawned and answering, but without a GitHub token: local IPC works, while the
    /// registry refresh cannot run and every peer ages out within ten minutes. Never
    /// reported as healthy.
    StartedWithoutToken { pid: u32 },
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
    // M5 2026-10-02, twice: this core runs as a system LaunchDaemon, outside the user's
    // login session. A daemon it spawned got no GitHub token (`airc join` provisions one
    // from the user's gh; a bare `airc daemon` does not), so its registry refresh never
    // ran and every peer aged into a ghost ten minutes later: the node was off the mesh
    // while it looked healthy. airc's own supervisor runs in the session; use it.
    match start_route(&airc_supervisor_state()) {
        StartRoute::ThroughSupervisor => return start_through_airc_supervisor(),
        StartRoute::Refuse(why) => return Spawned::Failed(why),
        StartRoute::SpawnDirect => {}
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
    let mut command = std::process::Command::new("airc");
    command
        .arg("daemon")
        .current_dir(&home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(daemon_log(&home));
    // The token `airc join` would have provisioned, when this context can read it.
    let token = gh_token();
    if let Some(token) = &token {
        command.env("GH_TOKEN", token);
    }
    let child = command.spawn();
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
            return if token.is_some() { Spawned::Started { pid } } else { Spawned::StartedWithoutToken { pid } };
        }
    }
    Spawned::Failed(format!(
        "airc daemon (pid {pid}) spawned but never answered within {}s",
        ANSWER_BOUND.as_secs()
    ))
}

/// The whole of one [`spawn`], on every path: the supervisor route's two launchctl
/// probes plus its wait, or the direct route's token probe plus its wait. A caller that
/// bounds `spawn` uses this, so a slow but successful start is never reported failed
/// while its work goes on unobserved.
pub const SPAWN_BUDGET: Duration = Duration::from_secs(40);

/// What can be read about airc's own login supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorState {
    /// None registered (or this OS has none): the core starts the daemon itself.
    Absent,
    /// Registered: only it may start the daemon, because it provisions the token.
    Registered,
    /// launchd could not be asked (timed out, not runnable): neither route is known safe.
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartRoute {
    ThroughSupervisor,
    SpawnDirect,
    Refuse(String),
}

/// The pure rule. A registered supervisor is never bypassed: if it cannot start the
/// daemon, that failure is the answer, not a tokenless spawn behind its back.
pub fn start_route(state: &SupervisorState) -> StartRoute {
    match state {
        SupervisorState::Absent => StartRoute::SpawnDirect,
        SupervisorState::Registered => StartRoute::ThroughSupervisor,
        SupervisorState::Unreadable(why) => StartRoute::Refuse(format!(
            "cannot tell whether airc's login supervisor is registered ({why}); not starting a daemon around it"
        )),
    }
}

#[cfg(target_os = "macos")]
fn airc_supervisor_target() -> String {
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    format!("gui/{uid}/{}", crate::airc::discovery::AIRC_JOIN_SUPERVISOR)
}

fn airc_supervisor_state() -> SupervisorState {
    #[cfg(target_os = "macos")]
    {
        use crate::system_resources::bounded_command::{probe, Probed};
        match probe("launchctl", &["print", &airc_supervisor_target()], ENDPOINT_RESOLVE_BOUND) {
            Probed::Exited { success: true, .. } => SupervisorState::Registered,
            Probed::Exited { success: false, .. } => SupervisorState::Absent,
            other => SupervisorState::Unreadable(format!("launchctl print: {}", other.outcome())),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        SupervisorState::Absent
    }
}

/// Start the daemon through airc's registered LaunchAgent: `airc join` in the user's
/// session, which provisions the daemon's token. Every failure is returned as one.
fn start_through_airc_supervisor() -> Spawned {
    #[cfg(target_os = "macos")]
    {
        use crate::system_resources::bounded_command::{probe, Probed};
        let target = airc_supervisor_target();
        match probe("launchctl", &["kickstart", "-k", &target], ENDPOINT_RESOLVE_BOUND) {
            Probed::Exited { success: true, .. } => {}
            other => {
                return Spawned::Failed(format!(
                    "{target} is registered but launchd did not start it ({})",
                    other.outcome()
                ))
            }
        }
        // The answering probe, `launchctl print` and this kickstart each spend up to
        // one resolve bound before the wait starts.
        let wait = SPAWN_BUDGET - ENDPOINT_RESOLVE_BOUND * 3;
        let deadline = std::time::Instant::now() + wait;
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
            if answering() {
                return Spawned::StartedByAirc;
            }
        }
        Spawned::Failed(format!("kickstarted {target} but no daemon answered within {}s", wait.as_secs()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Spawned::Failed("airc's login supervisor route exists only on macOS".into())
    }
}

/// The user's GitHub token from `gh`, bounded; `None` when gh is absent or cannot read
/// its store from this context (a system service has no login keychain).
fn gh_token() -> Option<String> {
    let probed = crate::system_resources::bounded_command::probe("gh", &["auth", "token"], ENDPOINT_RESOLVE_BOUND);
    let token = probed.stdout_if_ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

/// Where a daemon this core spawns writes its stderr: a file, so a gate that keeps
/// skipping (no token, no route) is readable instead of going to /dev/null.
fn daemon_log(home: &std::path::Path) -> std::process::Stdio {
    let dir = home.join(".continuum").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("airc-daemon.log"))
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null())
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
        Spawned::StartedWithoutToken { pid } => Restart::Failed(format!(
            "restarted (pid {pid}) without a GitHub token; peers will age out"
        )),
        Spawned::Answering | Spawned::StartedByAirc => Restart::Restarted { old, new: 0 },
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

    // what this catches (review of #4672): a registered supervisor that could not start
    // the daemon fell through to the tokenless spawn the change exists to prevent, and an
    // unreadable launchd was treated as "no supervisor".
    #[test]
    fn a_registered_supervisor_is_never_bypassed_and_an_unreadable_one_refuses() {
        assert_eq!(start_route(&SupervisorState::Registered), StartRoute::ThroughSupervisor);
        assert_eq!(start_route(&SupervisorState::Absent), StartRoute::SpawnDirect);
        assert!(matches!(
            start_route(&SupervisorState::Unreadable("timed_out".into())),
            StartRoute::Refuse(_)
        ));
    }

    // what this catches (review of #4672): the supervisor route waited 30 s inside a
    // caller bounded at 17 s, so a slow successful start read as failed and its result
    // was lost. Every route must fit the one budget the caller uses.
    #[test]
    fn every_start_route_fits_the_budget_its_caller_waits() {
        // answering probe + launchctl print + kickstart, then the wait
        let supervisor_route = ENDPOINT_RESOLVE_BOUND * 3 + (SPAWN_BUDGET - ENDPOINT_RESOLVE_BOUND * 3);
        // answering probe + launchctl print + socket resolve + gh token, then the wait
        let direct_route = ENDPOINT_RESOLVE_BOUND * 4 + ANSWER_BOUND;
        assert!(supervisor_route <= SPAWN_BUDGET);
        assert!(direct_route <= SPAWN_BUDGET);
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
