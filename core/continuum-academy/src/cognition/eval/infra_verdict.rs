use super::*;

/// The Proctored Exam Session verdict: does a completed pass constitute a trustworthy
/// score, or did the serving lane die mid-exam? ≥1 unrecoverable infra fault ⇒ the run is
/// VOID and reported [`InfraUnavailable`] (never a fake `pass_rate: 0.0`). Pure so the
/// fault-injection test pins it directly, and ONE place decides void-ness for every render
/// site. [[proctored-exam-session-dependable-benchmark]]
pub(super) fn infra_verdict(outcome: &PassOutcome) -> Option<InfraUnavailable> {
    (outcome.infra_faults > 0).then(|| InfraUnavailable {
        reason: outcome
            .infra_reason
            .clone()
            .unwrap_or_else(|| "serving lane never recovered".to_string()),
        tasks_attempted: outcome.results.len() as u32,
        infra_faults: outcome.infra_faults,
    })
}

/// How many times a task that INFRA-FAULTED (the model call itself failed, not a wrong
/// answer) is re-verified and retried before it counts as unrecoverable. This is the
/// bounded recovery a transient lane bounce (grow-back relaunch, brief not-ready) gets —
/// distinct from `max_acts` (the persona's OWN act→observe budget) and MAX_FAIL_RETRIES
/// (agentic recovery from a WRONG answer). 3 covers a full not-ready→relaunch→ready cycle
/// without letting a genuinely-dead lane spin the exam forever.
pub(super) const INFRA_FAULT_RETRIES: u32 = 3;

/// Bound for a between-attempt lane re-verify. A grow-back relaunch or a #175 self-heal
/// respawn brings the lane back within tens of seconds; this waits out that window before
/// spending a retry, and its expiry (lane still not snapshot-ready) is itself the signal
/// the fault is unrecoverable. Generous vs a warm lane's instant readiness; it only ever
/// elapses on a genuinely down lane.
pub(super) const LANE_REVERIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

/// Re-verify that the live serving lane is READY to decode before retrying an infra-faulted
/// task. Returns the ready snapshot's active model, or `None` if the lane never returned to
/// ready within [`LANE_REVERIFY_TIMEOUT`] (⇒ the fault is unrecoverable). Snapshot-ready is
/// the cheap gate; the retried TASK is itself the real decode proof (a genuine generation),
/// so this is not status-optimism — a lane that answers ready but can't decode simply
/// infra-faults again on the retry and exhausts the budget. Slice A upgrades this to a
/// decode smoke-probe at ACQUIRE time; Slice B verifies via the real task attempt.
pub(super) async fn reverify_lane() -> Option<String> {
    crate::inference::llama_server::await_ready_serving(LANE_REVERIFY_TIMEOUT)
        .await
        .and_then(|s| s.active_model)
}
