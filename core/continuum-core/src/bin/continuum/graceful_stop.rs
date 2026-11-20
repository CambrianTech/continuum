//! The graceful-stop protocol: budget, authority, drain permission and request.

use super::*;

/// `continuum stop` — stop the running core (the detached session started by `continuum start`).
/// How long the core gets to drain, save and join before the CLI stops waiting.
///
/// `Runtime::shutdown` runs three 2s-bounded phases per module in parallel, so a healthy
/// stop is ~6s worst case; the extra room is for the response to travel back. A stop that
/// exceeds this is not assumed dead — it is assumed UNKNOWN, and the caller says so.
///
/// Plus the turn settle's cap (card 32fa22ba): before any module saves, the core lets the
/// citizens' admitted turns finish, for their measured remaining time and never longer than
/// `TURN_SETTLE_CAP`. The same constant the core waits on, so the CLI never gives up on a
/// stop that is still letting a turn finish. An older core does not settle and answers in
/// the first 20 s as before.
pub(super) const GRACEFUL_STOP_BUDGET: std::time::Duration = std::time::Duration::from_secs(
    20 + continuum_core::cognition::turn_ingress::TURN_SETTLE_CAP.as_secs(),
);

/// Say the stop may wait before it does (Fable on #4684): a stop letting turns finish for
/// up to `TURN_SETTLE_CAP` must read as busy, never as hung. Printed before every graceful
/// stop request, so the operator's `stop` and the deploy's reboot say the same thing.
pub(super) fn say_turns_may_settle() {
    println!(
        "▶ stopping: turns already in flight finish first (their measured remaining time, at most {}s), then the core saves",
        continuum_core::cognition::turn_ingress::TURN_SETTLE_CAP.as_secs()
    );
}

/// What the receipt-bound shutdown owner attested. Transport and capability failures
/// return errors before this value is constructed; they never authorize forced teardown.
#[derive(Debug)]
pub(super) enum GracefulStop {
    Durable(String),
    Incomplete(String),
    NothingRunning,
}

impl GracefulStop {
    pub(super) fn require_durable(&self) -> Result<(), String> {
        match self {
            Self::Durable(_) | Self::NothingRunning => Ok(()),
            Self::Incomplete(summary) => Err(format!(
                "shutdown refused without durable custody: {summary}; the core remains running"
            )),
        }
    }
}

/// The core pid the pidfile CLAIMS. `None` means no pidfile, or one that names nothing
/// parseable.
pub(super) fn core_pid_for_teardown() -> Option<i32> {
    std::fs::read_to_string(pidfile_for(&socket_path()))
        .ok()
        .and_then(|c| c.trim().parse::<i32>().ok())
}

/// Every core this stop would have to end.
///
/// THE PIDFILE IS NOT SUFFICIENT, and on the node this rail exists for it is already
/// gone: the old `stop` removed it whether or not the kill took, so the file is absent
/// while the process still answers ping (Astra, 2026-09-22). Preflighting only the
/// pidfile would then find nothing to check, pass, and drain a core it cannot end —
/// the exact failure the preflight is for. The process table is the second source, and
/// the one that cannot be deleted by a previous mistake.
pub(super) fn cores_to_tear_down() -> Vec<i32> {
    let mut pids: Vec<i32> = core_pid_for_teardown().into_iter().collect();
    for pid in running_core_pids() {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    pids
}

/// How long a receipt-acknowledged core gets to leave the process table.
pub(super) const TEARDOWN_EXIT_DEADLINE: Duration = Duration::from_secs(30);

/// Wait, bounded, for `pid` to leave the process table.
///
/// The commit closes the core asynchronously. A timeout leaves its pidfile intact
/// and refuses handoff; this observer never terminates the process.
pub(super) async fn exited_within(pid: i32, deadline: Duration) -> Result<(), String> {
    let until = std::time::Instant::now() + deadline;
    while pid_alive(pid) {
        if std::time::Instant::now() >= until {
            return Err(format!(
                "core pid {pid} is still running {}s after receipt-bound exit acknowledgment; \
                 its pidfile is left in place because the process it names still \
                 exists, and no handoff is issued over a core that did not go",
                deadline.as_secs()
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Ok(())
}

/// Operator receipt; an incomplete save never claims a forced exit or data loss.
pub(super) fn render_graceful_outcome(outcome: &GracefulStop) -> (bool, String) {
    match outcome {
        GracefulStop::Durable(summary) => (false, format!("core stopped gracefully — {summary}")),
        GracefulStop::Incomplete(summary) => (true, format!("shutdown refused; core retained with unsaved state — {summary}")),
        GracefulStop::NothingRunning => (false, "no core is listening; checking owned lane receipts".into()),
    }
}
