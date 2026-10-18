//! Reaching the running core: the socket and pidfile paths, the uniform `Connection`,
//! the bounded `ping` that decides whether a core is answering, and the bounded reads of
//! the live process's own provenance (`buildSha` over `ping`).

use std::time::Duration;

use continuum_client::core_ipc::CoreIpcTransport;
use continuum_client::Connection;

use super::CliError;
use serde_json::Value;

/// Where `continuum start` records the detached core's PID so `continuum stop` can find it.
pub(super) fn pidfile_for(socket: &str) -> String {
    format!("{socket}.pid")
}

pub(super) fn start_logfile() -> String {
    continuum_core::ipc::endpoint_paths::core_start_logfile()
}

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

/// The RUNNING core's build SHA, from the process itself over `ping`, bounded at 30 s: a
/// core that accepts the socket mid-boot but never answers made `continuum reboot` hang
/// for good (IntelMac, 50 min, 2026-09-05). `what` names the caller in the error. The one
/// read both the deploy receipt and the rollback's identity check use.
pub(super) async fn running_core_build_sha(what: &str) -> Result<Option<String>, String> {
    let reply = running_core_provenance("ping", what).await?;
    Ok(reply.get("buildSha").and_then(|v| v.as_str()).map(str::to_string))
}

pub(super) async fn running_core_provenance(command: &str, what: &str) -> Result<Value, String> {
    let socket = socket_path();
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection()
            .commands()
            .execute_value(command, Value::Object(Default::default())),
    )
    .await
    .map_err(|_| {
        format!(
            "{what}: the core on {socket} accepted the socket but did not answer `{command}` \
             within 30 s — mid-boot (retry) or wedged (read boot.phase / boot.module_init)"
        )
    })?
    .map_err(|e| format!("{what}: no core answering on {socket}: {e}"))?;
    Ok(reply)
}

pub(super) async fn running_build_sha() -> Option<String> {
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        connection()
            .commands()
            .execute_value("ping", Value::Object(Default::default())),
    )
    .await
    .ok()?
    .ok()?;
    reply.get("buildSha").and_then(|v| v.as_str()).map(str::to_string)
}
