//! The connection to the running core: socket, uniform `Connection`, liveness probe and bind decision.

use super::*;

pub(super) fn socket_path() -> String {
    continuum_core::ipc::endpoint_paths::core_socket_path()
}

pub(super) fn connection() -> Connection<CoreIpcTransport> {
    Connection::new(CoreIpcTransport::new(socket_path()))
}

/// How long a `ping` may take before we call the core "not answering".
///
/// Measured on a healthy local core: 20ms, five samples, no variance — this is a
/// unix-socket round trip, not a network call. 5s is 250× that, so it cannot fire on
/// a merely-busy core; it exists solely to bound a core that will NEVER answer.
pub(super) const PING_BUDGET: Duration = Duration::from_secs(5);

/// Is a core already answering on the socket? A real ping round-trip, not just a
/// socket-file existence check (a stale socket file lies) — and BOUNDED, which is
/// load-bearing rather than defensive.
///
/// An unbounded ping does not fail against an unresponsive core, it HANGS: the kernel
/// completes `connect()` into the listen backlog whether or not the process is
/// scheduled, the write succeeds, and the read then waits for a reply that never comes.
/// A timeout counts as NOT answering, which is the safe direction — paired with a
/// visible core process it yields `Occupied`, so the caller refuses and names the pids
/// rather than launching a competitor.
///
/// This bound is what makes the [`BindDecision::Occupied`] arm REACHABLE, not a
/// nicety. Glass-boxed 2026-08-14 on this box, both directions:
///
/// - unbounded: SIGSTOP every core, run `start` → no output, no decision, still hung
///   at 90s. The guard was green in unit tests and dead on the live path.
/// - bounded:   same setup → exits 1 in 8s with the refusal naming both live pids
///   (5s here plus the process-table read), and the cores resume unharmed on SIGCONT.
///
/// The 90s figure came from a binary whose build had failed, so it measures the
/// unbounded path either way — but it is the OLD behaviour, not evidence against the
/// timeout, which the 8s run then confirmed directly.
pub(super) async fn core_is_up() -> bool {
    // Bound to locals: `connection()` and `.commands()` yield temporaries that the
    // future borrows, so building the future inline drops them at the end of the
    // statement (E0716).
    let conn = connection();
    let cmds = conn.commands();
    let ping = cmds.execute_value("ping", Value::Object(Default::default()));
    matches!(tokio::time::timeout(PING_BUDGET, ping).await, Ok(Ok(_)))
}

/// Check the existing core without invoking any lifecycle path. Even a Free socket
/// can belong to a deploy whose compiler has not launched its server yet (cf63a02e).
pub(super) async fn ensure_core_running(command: &str) -> Result<(), CliError> {
    if core_is_up().await {
        Ok(())
    } else {
        Err(CliError::NoCore {
            socket: socket_path(),
            command: command.to_owned(),
        })
    }
}

/// Gather the two observations [`BindDecision`] is a function of — a real `ping`
/// round-trip and the core process table — and hand them to the shared truth table.
///
/// The observation half lives here because it is platform- and transport-shaped; the
/// DECISION half lives in the lib so it is unit-tested by the `--lib` CI gate.
/// Explicit starts go through this seam; ordinary dispatch only checks reachability:
/// the split brain existed because each launch path had its own ad-hoc guard.
pub(super) async fn bind_decision() -> BindDecision {
    let ping_ok = core_is_up().await;
    let running: Vec<i32> = running_core_pids()
        .into_iter()
        .filter(|p| pid_alive(*p))
        .collect();
    continuum_cli_lifecycle::core_bind_guard::decide(ping_ok, &running)
}
