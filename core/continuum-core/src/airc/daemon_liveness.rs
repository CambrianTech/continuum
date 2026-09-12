//! DAEMON LIVENESS — the transport daemon has an owner that ACTS while the core runs.
//!
//! The daemon is the one service whose absence makes the whole node inert: no rooms, no
//! roster, no residency, every citizen deaf while still reading "resident". Until this
//! module the core spawned it once at boot (`boot_plan::step_airc_daemon`) and restarted
//! it only when the CORE's own descriptor table hit pressure (`fd_pressure`). Nothing
//! watched whether it was still there.
//!
//! 2026-09-12: the daemon's updater stopped it at the 06:53Z canary merge, failed to
//! install over a write-locked binary, and never restarted it. The core stayed up for
//! 100 minutes with four residents "resident", 2,404 `room_roster failed` warnings, and
//! nothing that could act — a human ran `airc status`, which happened to start one. Joel:
//! "has to be reliable out of the box, recover itself, and not just for our machine."
//!
//! RTOS shape per CONCURRENCY-STYLE-GUIDE: own task, `tokio::time::interval`, the probe
//! and the spawn on `spawn_blocking` under a bound, a pure decision function, one probe
//! class per outcome. No lock, nothing on a hot path.
//!
//! The grace before reviving is longer than the updater's own stop→start on a warm
//! rebuild, so a healthy update finishes on its own; a revive during a COLD rebuild is
//! bounded churn (the updater's start finds a daemon answering and the next update tick
//! repeats) — the airc-side fix for that is build-then-swap (card b551842e). Every revive
//! is loud, so the churn is visible, never silent.
use std::time::{Duration, Instant};

use crate::airc::daemon_supervisor::{self, Spawned};

/// How often the owner asks the socket whether a daemon answers.
pub const PERIOD: Duration = Duration::from_secs(15);
/// Consecutive absent periods before the owner spawns a daemon (60 s at `PERIOD`):
/// above a warm `airc update` restart, below the point where a dark node costs a turn.
pub const GRACE_PERIODS: u32 = 4;
/// Bound on the liveness probe itself: a connect that hangs is "not answering", named.
pub const PROBE_BOUND: Duration = Duration::from_secs(2);
/// Ceiling on the doubling back-off between failed revives.
pub const BACKOFF_CAP: Duration = Duration::from_secs(300);

/// What the owner does this tick, decided purely from what it observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// A daemon answers; nothing to do.
    Alive,
    /// Absent, but inside the grace window (`periods` so far).
    Grace { periods: u32 },
    /// Absent past the grace window and past the back-off: spawn one now.
    Revive { attempt: u32 },
    /// Absent past grace, but a failed attempt is still backing off.
    BackingOff { remaining: Duration },
}

/// Back-off after `failed_attempts` failed revives: `PERIOD × 2^n`, capped.
pub fn backoff_after(failed_attempts: u32) -> Duration {
    let mult = 1u32 << failed_attempts.min(10);
    (PERIOD * mult).min(BACKOFF_CAP)
}

/// The decision, pure. `absent_periods` counts THIS tick when `answering` is false.
pub fn decide(
    answering: bool,
    absent_periods: u32,
    failed_attempts: u32,
    since_last_attempt: Option<Duration>,
) -> Decision {
    if answering {
        return Decision::Alive;
    }
    if absent_periods < GRACE_PERIODS {
        return Decision::Grace { periods: absent_periods };
    }
    match since_last_attempt {
        Some(elapsed) if failed_attempts > 0 => {
            let wait = backoff_after(failed_attempts);
            if elapsed < wait {
                return Decision::BackingOff { remaining: wait - elapsed };
            }
            Decision::Revive { attempt: failed_attempts + 1 }
        }
        _ => Decision::Revive { attempt: failed_attempts + 1 },
    }
}

/// Spawn the owner. Called once from main after the runtime is up.
pub fn spawn() {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(PERIOD);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut absent_periods: u32 = 0;
        let mut failed_attempts: u32 = 0;
        let mut last_attempt: Option<Instant> = None;
        let mut absent_since: Option<Instant> = None;
        let mut revived_by_us = false;
        loop {
            ticker.tick().await;
            let answering = match tokio::time::timeout(
                PROBE_BOUND,
                tokio::task::spawn_blocking(daemon_supervisor::answering),
            )
            .await
            {
                Ok(Ok(a)) => a,
                // A hung connect or a panicked probe both read as "not answering" — the
                // owner acts on absence, and the row below says why.
                Ok(Err(_)) | Err(_) => false,
            };
            if answering {
                if let Some(since) = absent_since.take() {
                    crate::probe!(
                        class = "airc.daemon.back",
                        absent_s = since.elapsed().as_secs(),
                        revived_by_us = revived_by_us,
                        "the transport daemon answers again"
                    );
                }
                absent_periods = 0;
                failed_attempts = 0;
                last_attempt = None;
                revived_by_us = false;
                continue;
            }
            absent_periods = absent_periods.saturating_add(1);
            if absent_since.is_none() {
                absent_since = Some(Instant::now());
                crate::probe!(
                    class = "airc.daemon.absent",
                    grace_s = (PERIOD * GRACE_PERIODS).as_secs(),
                    "no daemon answers on the machine socket — reviving after the grace window"
                );
            }
            match decide(
                false,
                absent_periods,
                failed_attempts,
                last_attempt.map(|t| t.elapsed()),
            ) {
                Decision::Alive | Decision::Grace { .. } | Decision::BackingOff { .. } => {}
                Decision::Revive { attempt } => {
                    last_attempt = Some(Instant::now());
                    let outcome = match tokio::time::timeout(
                        daemon_supervisor::ANSWER_BOUND + Duration::from_secs(2),
                        tokio::task::spawn_blocking(daemon_supervisor::spawn),
                    )
                    .await
                    {
                        Ok(Ok(outcome)) => outcome,
                        Ok(Err(e)) => Spawned::Failed(format!("spawn task panicked: {e}")),
                        Err(_) => Spawned::Failed("spawn did not return within its bound".into()),
                    };
                    match outcome {
                        Spawned::Started { pid } => {
                            revived_by_us = true;
                            crate::probe!(
                                class = "airc.daemon.revived",
                                pid = pid,
                                attempt = attempt,
                                absent_s = absent_since.map(|s| s.elapsed().as_secs()).unwrap_or(0), // unwrap_or: 0 = absence start unrecorded, a legible value in the row
                                "the owner spawned a transport daemon and it answers"
                            );
                        }
                        Spawned::Answering => {
                            // Someone else brought it back between the probe and the
                            // spawn (the updater's own restart, a CLI). Next tick reads
                            // it as alive and emits `airc.daemon.back`.
                        }
                        Spawned::BinaryAbsent | Spawned::NoHome | Spawned::Failed(_) => {
                            failed_attempts = failed_attempts.saturating_add(1);
                            crate::probe!(
                                class = "airc.daemon.revive_failed",
                                attempt = attempt,
                                reason = format!("{outcome:?}"),
                                next_try_s = backoff_after(failed_attempts).as_secs(),
                                "the owner could not revive the transport daemon"
                            );
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the owner acting on a blip (one absent tick during a warm
    // update restart would double-start the daemon) or sleeping through a real
    // absence (the 100-minute dark node of 2026-09-12).
    #[test]
    fn a_blip_waits_out_the_grace_and_a_real_absence_is_revived() {
        assert_eq!(decide(true, 0, 0, None), Decision::Alive);
        assert_eq!(decide(false, 1, 0, None), Decision::Grace { periods: 1 });
        assert_eq!(decide(false, 3, 0, None), Decision::Grace { periods: 3 });
        assert_eq!(decide(false, 4, 0, None), Decision::Revive { attempt: 1 });
        assert!(
            PERIOD * GRACE_PERIODS >= Duration::from_secs(45),
            "grace must outlast a warm `airc update` stop→start"
        );
    }

    // what this catches: a revive that fails (binary absent on a fresh box) retrying
    // every tick forever, or backing off past the cap into a de-facto give-up.
    #[test]
    fn a_failed_revive_backs_off_doubling_and_capped_then_tries_again() {
        assert_eq!(backoff_after(1), Duration::from_secs(30));
        assert_eq!(backoff_after(2), Duration::from_secs(60));
        assert_eq!(backoff_after(4), Duration::from_secs(240));
        assert_eq!(backoff_after(5), BACKOFF_CAP);
        assert_eq!(backoff_after(40), BACKOFF_CAP, "the shift is bounded, the cap holds");
        assert_eq!(
            decide(false, 9, 1, Some(Duration::from_secs(10))),
            Decision::BackingOff { remaining: Duration::from_secs(20) }
        );
        assert_eq!(
            decide(false, 9, 1, Some(Duration::from_secs(30))),
            Decision::Revive { attempt: 2 }
        );
        assert_eq!(decide(true, 9, 3, Some(Duration::from_secs(1))), Decision::Alive);
    }
}
