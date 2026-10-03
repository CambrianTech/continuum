//! The ONE place the core starts, observes and restarts the airc daemon.
//!
//! Before 2026-09-12 the boot asked `airc ipc-endpoint` (a CLI fork that prints a
//! path and proves nothing about liveness), spawned `airc daemon`, dropped the child
//! handle, and never looked again. When the descriptor table filled that night the
//! only recovery was a human killing and restarting the daemon by hand. Now the core
//! answers "is it answering" by connecting to the socket it resolves itself, and
//! starts and restarts the daemon only through airc's own lifecycle (its login
//! supervisor, its gated autostart, `airc stop`), never by spawning or killing the
//! process itself, so airc's maintenance gate and token provisioning always apply.
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

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
    /// A start was issued through airc (its login supervisor on macOS, or its own gated
    /// autostart, which provisions the daemon's token); airc owns that daemon, not this
    /// core. `spawn` does not wait for it to answer: a large store takes 60-70 s to open
    /// (the 5090, 2.4 GB), so the caller notes the start and gives it its patience. The
    /// text says which route and anything the caller should know (a tokenless start).
    Issued(String),
    /// The `airc` binary is not on PATH — a transportless box (CI, a fresh clone).
    BinaryAbsent,
    /// Neither USERPROFILE nor HOME is set: the machine-account scope is unresolvable.
    NoHome,
    /// Spawned (or failed to) and never answered within the bound.
    Failed(String),
}

/// Start the daemon through airc, from the scope's home (never the repo cwd: the
/// ownership guard refuses a socket served under the wrong identity), and return once
/// the start is issued. It never waits for the daemon to answer; see [`Spawned::Issued`].
/// Idempotent: an answering daemon is left alone.
pub fn spawn() -> Spawned {
    if answering() {
        return Spawned::Answering;
    }
    // M5 2026-10-02, twice: this core runs as a system LaunchDaemon, outside the user's
    // login session. A daemon it spawned got no GitHub token (`airc join` provisions one
    // from the user's gh; a bare `airc daemon` does not), so its registry refresh never
    // ran and every peer aged into a ghost ten minutes later: the node was off the mesh
    // while it looked healthy. airc's own supervisor runs in the session; use it.
    let note = match start_route(&airc_supervisor_state()) {
        StartRoute::ThroughSupervisor => return start_through_airc_supervisor(),
        StartRoute::Refuse(why) => return Spawned::Failed(why),
        StartRoute::SpawnDirect(note) => note,
    };
    let Some(scope) = scope_home() else { return Spawned::NoHome };
    let Some(home) = scope.parent().map(|p| p.to_path_buf()) else { return Spawned::NoHome };
    if socket_path().is_none() {
        return Spawned::Failed(
            "the machine socket path is unresolvable (`airc ipc-endpoint` answered nothing usable) — \
             a daemon could be spawned but never observed"
                .into(),
        );
    }
    // Never a bare `airc daemon`: airc's own autostart (ensure_daemon_running, reached by
    // any CLI read) holds its lifecycle gate, so a maintenance window refuses it, and it
    // provisions the daemon's GitHub token itself, or logs that it could not. The core
    // starts airc only the way airc starts itself.
    let (log, log_note) = match daemon_log(&home) {
        Ok(file) => (std::process::Stdio::from(file), "its words are in ~/.continuum/logs/airc-daemon.log".to_string()),
        Err(e) => (std::process::Stdio::null(), format!("its words are lost: airc-daemon.log could not be opened ({e})")),
    };
    let mut command = std::process::Command::new("airc");
    command
        .args(["events", "list", "--limit", "0", "--json"])
        .current_dir(&home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: the boot task has no console
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Spawned::BinaryAbsent,
        Err(e) => return Spawned::Failed(format!("airc autostart could not run: {e}")),
    };
    // The read returns once airc's daemon answers; on a large store that outlasts this
    // bound, so a read still waiting here is a start in progress, not a refusal. Only
    // airc's own non-zero exit (a maintenance gate, a refused start) is a failure.
    match crate::system_resources::bounded_command::wait_bounded(&mut child, AUTOSTART_BOUND) {
        Ok(Some(status)) if status.success() => Spawned::Issued(format!("airc's gated autostart{note}")),
        Ok(Some(status)) => Spawned::Failed(format!("airc's autostart refused or failed ({status}); {log_note}")),
        Ok(None) => Spawned::Issued(format!(
            "airc's gated autostart{note}, still waiting on its daemon after {}s (a store opening); the read was stopped, the daemon was not",
            AUTOSTART_BOUND.as_secs()
        )),
        Err(e) => Spawned::Failed(format!("waiting on airc's autostart: {e}")),
    }
}

/// The bound a caller puts on one [`spawn`]. It must cover [`spawn_worst_case`]; a test
/// holds it there, so raising any bound below fails it instead of leaving a `spawn`
/// running with nobody reading its result.
pub const SPAWN_BUDGET: Duration = Duration::from_secs(40);

/// The longest one [`spawn`] can take, summed from the bounds it actually runs under.
/// Every bounded child adds [`REAP_GRACE`](crate::system_resources::bounded_command::REAP_GRACE)
/// on a timeout; `socket_path` resolves at most twice (cached once it succeeds).
pub fn spawn_worst_case() -> Duration {
    use crate::system_resources::bounded_command::REAP_GRACE;
    let resolve = ENDPOINT_RESOLVE_BOUND + REAP_GRACE;
    let answering = resolve + ANSWERING_PROBE_BOUND;
    let supervisor_query = resolve; // launchctl print, same bound
    let through_supervisor = resolve; // launchctl kickstart, same bound
    let direct = resolve + AUTOSTART_BOUND + REAP_GRACE;
    answering + supervisor_query + through_supervisor.max(direct)
}

/// How long the direct route waits for airc's autostart read to return.
const AUTOSTART_BOUND: Duration = Duration::from_secs(10);

/// What can be read about airc's own login supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SupervisorState {
    /// None registered (or this OS has none): the core starts the daemon itself.
    Absent,
    /// No login session for this user (launchd: no `gui/<uid>` domain, exit 112): a core
    /// that booted to the login screen. No supervisor can be running, so refusing would
    /// protect nothing; the core starts the daemon directly and says it is tokenless.
    NoSession,
    /// Registered: only it may start the daemon, because it provisions the token.
    Registered,
    /// launchd could not be asked (timed out, not runnable): neither route is known safe.
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StartRoute {
    ThroughSupervisor,
    /// A direct start; the text is appended to its description (empty, or why it is tokenless).
    SpawnDirect(String),
    Refuse(String),
}

/// The pure rule. A registered supervisor is never bypassed: if it cannot start the
/// daemon, that failure is the answer, not a tokenless spawn behind its back.
fn start_route(state: &SupervisorState) -> StartRoute {
    match state {
        SupervisorState::Absent => StartRoute::SpawnDirect(String::new()),
        SupervisorState::NoSession => StartRoute::SpawnDirect(
            " with no login session, so tokenless until someone logs in (peers age out after 10 min)".into(),
        ),
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
        supervisor_state_from(&crate::system_resources::bounded_command::capture(
            "launchctl",
            &["print", &airc_supervisor_target()],
            ENDPOINT_RESOLVE_BOUND,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        SupervisorState::Absent
    }
}

/// launchd's own answer to `launchctl print gui/<uid>/<label>`. "No such service"
/// (exit 113, "Could not find service") is Absent; "no such domain" (exit 112, measured
/// by Cormac on the IntelMac for a gui domain with no login) is NoSession. Both permit a
/// direct start. Anything else stays Unreadable with launchd's words.
fn supervisor_state_from(answer: &crate::system_resources::bounded_command::Captured) -> SupervisorState {
    use crate::system_resources::bounded_command::Captured;
    match answer {
        Captured::Exited { code: Some(0), .. } => SupervisorState::Registered,
        Captured::Exited { code: Some(113), stderr, .. } if stderr.contains("Could not find service") => {
            SupervisorState::Absent
        }
        Captured::Exited { code: Some(112), .. } => SupervisorState::NoSession,
        Captured::Exited { code, stderr, .. } => SupervisorState::Unreadable(format!(
            "launchctl print exited {}: {}",
            code.map_or_else(|| "on a signal".to_string(), |c| c.to_string()),
            stderr.trim()
        )),
        Captured::TimedOut => SupervisorState::Unreadable("launchctl print timed out".into()),
        Captured::Unstartable { error } => SupervisorState::Unreadable(format!("launchctl print could not run: {error}")),
    }
}

/// Start the daemon through airc's registered LaunchAgent: `airc join` in the user's
/// session, which provisions the daemon's token. Every failure is returned as one.
fn start_through_airc_supervisor() -> Spawned {
    #[cfg(target_os = "macos")]
    {
        use crate::system_resources::bounded_command::{capture, Captured};
        let target = airc_supervisor_target();
        // No `-k`: a supervisor that is running but not yet answering is opening its store,
        // and killing it would restart that from zero (Cormac's review of #4672). A plain
        // kickstart starts the job only if it is not running.
        match capture("launchctl", &["kickstart", &target], ENDPOINT_RESOLVE_BOUND) {
            Captured::Exited { code: Some(0), .. } => Spawned::Issued(format!("airc's login supervisor {target}")),
            other => Spawned::Failed(format!("{target} is registered but launchd did not start it: {other:?}")),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Spawned::Failed("airc's login supervisor route exists only on macOS".into())
    }
}

/// Where a daemon this core spawns writes its stderr: a file, so a gate that keeps
/// skipping (no token, no route) is readable instead of going to /dev/null. It lives in
/// `~/.continuum/logs`, a tracked dir with its own eviction story. A file that cannot be
/// opened is returned as the error, so the caller says the words are lost instead of
/// pointing a reader at a log that does not exist.
fn daemon_log(home: &std::path::Path) -> std::io::Result<std::fs::File> {
    let dir = home.join(".continuum").join("logs");
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new().create(true).append(true).open(dir.join("airc-daemon.log"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restart {
    /// airc stopped its daemon and a new one answers.
    Restarted,
    /// The stop or the start did not complete; the reason is airc's or the bound's.
    Failed(String),
}

/// Restart the machine daemon through airc's own lifecycle. Not `airc stop` then
/// start: once airc records an operator stop intent (card 8825182f), that sequence would
/// leave the daemon stopped for good, or erase an operator's own stop. A transient
/// restart belongs to airc's maintenance boundary; until airc offers one, this says so
/// instead of guessing.
pub fn restart_through_airc() -> Restart {
    Restart::Failed(
        "airc has no transient restart entry yet (card 8825182f owns the lifecycle boundary); \
         an operator restart is owed"
            .into(),
    )
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
        assert_eq!(start_route(&SupervisorState::Absent), StartRoute::SpawnDirect(String::new()));
        // what this also catches (Cormac's review of #4672): a core booted to the login
        // screen has no gui domain, so no supervisor can run; it starts directly and the
        // route says it is tokenless instead of refusing.
        match start_route(&SupervisorState::NoSession) {
            StartRoute::SpawnDirect(note) => assert!(note.contains("tokenless"), "{note}"),
            other => panic!("no session must start directly, got {other:?}"),
        }
        assert!(matches!(
            start_route(&SupervisorState::Unreadable("timed_out".into())),
            StartRoute::Refuse(_)
        ));
    }

    // what this catches (reviews of #4672): a spawn that outlives the bound its caller
    // waits, so its result is lost while it keeps running. The previous form of this test
    // reduced to `x <= x` and could not fail (Cormac). The worst case is now summed from
    // the bounds spawn really runs under, including the reap grace each timed-out child
    // adds, so raising AUTOSTART_BOUND or a resolve bound past the budget fails here.
    #[test]
    fn spawns_worst_case_fits_the_budget_its_caller_waits() {
        let worst = spawn_worst_case();
        assert!(worst <= SPAWN_BUDGET, "spawn can take {worst:?}, callers wait {SPAWN_BUDGET:?}");
        assert!(worst > AUTOSTART_BOUND + ENDPOINT_RESOLVE_BOUND * 2, "the sum must include every bound it runs under");
    }

    // what this catches (review of #4672): every nonzero `launchctl print` read as
    // Absent, so a denied or broken domain query permitted the direct start. Only
    // launchd's own "no such service" answer is Absent. Fixtures are launchd's real
    // outputs, captured on the M5 (macOS 26.5.2).
    #[test]
    fn only_launchds_no_such_service_answer_permits_a_direct_start() {
        use crate::system_resources::bounded_command::Captured;
        let exited = |code: i32, stderr: &str| Captured::Exited { code: Some(code), stdout: String::new(), stderr: stderr.into(), truncated: false, cleanup: None };
        assert_eq!(supervisor_state_from(&exited(0, "")), SupervisorState::Registered);
        assert_eq!(
            supervisor_state_from(&exited(113, "Bad request.\nCould not find service \"airc-join\" in domain for user gui: 501\n")),
            SupervisorState::Absent
        );
        assert_eq!(
            supervisor_state_from(&exited(112, "Bad request.\nCould not find domain for user gui: 99999\n")),
            SupervisorState::NoSession
        );
        for denied in [
            exited(113, "something else entirely"),
            exited(1, "Operation not permitted"),
            Captured::TimedOut,
            Captured::Unstartable { error: "No such file or directory".into() },
        ] {
            let state = supervisor_state_from(&denied);
            assert!(matches!(state, SupervisorState::Unreadable(_)), "{denied:?} read as {state:?}");
            assert!(matches!(start_route(&state), StartRoute::Refuse(_)));
        }
    }

    // what this catches: the scope resolving to something other than the CLI's
    // machine-account home, which would serve the socket under the wrong identity.
    #[test]
    fn the_scope_is_the_machine_account_home() {
        if let Some(scope) = scope_home() {
            assert!(scope.ends_with(".airc"), "{}", scope.display());
        }
    }
}
