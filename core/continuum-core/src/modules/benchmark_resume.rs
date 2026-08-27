//! Boot resume for benchmark rounds — REJOIN, never re-dispatch (plan A5).
//!
//! **The law**: continuity is the default, reset is the exception. A `Working`
//! round survives a reboot on disk (its room, cards, driver, per-card activity
//! rooms and assignees — `bench_round`'s file); the solves a restart killed were
//! journaled `failed` by the boot reaper, which releases the `claim-<card>`
//! in-flight guard. So resuming is not a subsystem: it is the SAME ONE driver
//! decision every other edge uses — [`bench_round::next_unworked_per_round`] →
//! [`work::dispatch_staged_swe_solve`] — fired once after boot, whereupon the
//! card-settled edge (benchmark_grade) chains the rest. Each re-fired solve
//! REJOINS its recorded per-instance activity room and re-enters its preserved
//! workspace mid-stride: resume is recall, not restore.
//!
//! **Placement**: this lives on the BENCHMARK side and is invoked from the
//! benchmark module's init — never the serving daemon, whose boot reap
//! explicitly refuses to re-dispatch (a serving daemon that posts work is the
//! parallel-runner shape, BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md).
//!
//! **The acceptance test this exists for**: dispatch a round, `continuum reboot
//! --force` mid-solve, hands off — the round rejoins its rooms, solves re-fire
//! (`bench.round.resumed`), cards reach terminal, the round reaches Done, with
//! zero operator commands after the reboot.

use crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry;

/// How long to park on serving decode-readiness before declaring the resume
/// blocked for this boot. Generous: model load on the M5 takes minutes.
const SERVING_PARK: std::time::Duration = std::time::Duration::from_secs(600);

/// How long to park on citizen residency. The persona reconciler's post-boot
/// window is documented at ~10–15 minutes on this box; the park outwaits it.
const RESIDENCY_PARK: std::time::Duration = std::time::Duration::from_secs(20 * 60);
const RESIDENCY_POLL: std::time::Duration = std::time::Duration::from_secs(10);

/// Spawn the one-shot boot resume. Cheap when there is nothing to resume.
pub fn spawn_boot_resume(registry: PersonaAircRuntimeRegistry) {
    if !crate::cognition::bench_round::any_working_round() {
        return; // nothing survived — no task, no waiting, no noise
    }
    tokio::spawn(async move {
        // EVERY attempt re-parks (the one-shot park was measured failing live
        // 2026-08-26: serving became decode-ready ~3 min AFTER a single 600s park
        // expired, and the whole boot's resume was forfeited — a dead-reckoned
        // timeout, the exact shape ROUND-LIFECYCLE §7 bans). The loop keeps
        // waiting while the daemon is still trying; each blocked attempt says so.
        const RESUME_RETRIES: u32 = 12;
        const RETRY_SPACING: std::time::Duration = std::time::Duration::from_secs(90);
        // After the fast post-boot window, the task DOES NOT EXIT — it degrades
        // to a slow standing watch. Measured live 2026-08-26: serving came
        // decode-ready ~2 minutes AFTER the 12th attempt, and the round sat
        // becalmed — Working, serving ready, citizens resident, zero drivers —
        // with nothing scheduled to ever revive it ("next boot or settle edge",
        // and no settle can come when nothing runs). The resident assignee even
        // RENEWS the dead solves' claims, so the lapse sweeper can't free them
        // either. A watchdog tick is the missing edge; `bench.round.becalmed`
        // is the sensor that makes a stuck round LOUD instead of silent.
        const SLOW_WATCH: std::time::Duration = std::time::Duration::from_secs(300);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let fast = attempt <= RESUME_RETRIES;
            if attempt == RESUME_RETRIES + 1 {
                crate::probe!(
                    class = "bench.round.slow_watch",
                    "fast resume window spent — degrading to a standing 5-minute watch \
                     (the round can no longer be silently becalmed)"
                );
            }
            // Park 1: serving decode-verified (same primitive dispatch parks on).
            if crate::inference::llama_server::await_ready_serving(SERVING_PARK)
                .await
                .is_none()
            {
                crate::probe!(
                    class = "bench.round.resume_blocked",
                    reason = "serving",
                    attempt = attempt as u64,
                    "serving not decode-ready within this attempt's park — re-parking"
                );
                if !fast {
                    tokio::time::sleep(SLOW_WATCH).await; // slow watch: no hot spin while serving is down
                }
                continue;
            }
            // Park 2: residency (a service loop, not mere registration — #455).
            let started = std::time::Instant::now();
            let resident = loop {
                if !registry.resident_snapshot().await.is_empty() {
                    break true;
                }
                if started.elapsed() > RESIDENCY_PARK {
                    break false;
                }
                tokio::time::sleep(RESIDENCY_POLL).await;
            };
            if !resident {
                crate::probe!(
                    class = "bench.round.resume_blocked",
                    reason = "residency",
                    attempt = attempt as u64,
                    "no citizen resident within this attempt's park — re-parking"
                );
                if !fast {
                    tokio::time::sleep(SLOW_WATCH).await; // slow watch: residency park already waited 20min
                }
                continue;
            }
            let due = crate::cognition::bench_round::next_unworked_per_round();
            if due.is_empty() {
                if !crate::cognition::bench_round::any_working_round() {
                    return; // every round terminal — the watch has nothing left to guard
                }
                // In flight (the settle edge owns the chain) — keep the slow
                // watch alive as the backstop for the NEXT becalming.
                tokio::time::sleep(SLOW_WATCH).await;
                continue;
            }
            if !fast {
                // TRULY becalmed means NO solve is running anywhere — the fast
                // window may legitimately fan out boot re-fires, but the slow
                // watch reviving one more card per tick while solves are LIVE
                // is drift into parallel solving nobody decided (B7 is a
                // deliberate, measured decision — never a watchdog side
                // effect; caught live 2026-08-27, one extra solve per 5min).
                let live = crate::cognition::swe_bench::in_flight_solve_runs();
                if !live.is_empty() {
                    tokio::time::sleep(SLOW_WATCH).await;
                    continue;
                }
                crate::probe!(
                    class = "bench.round.becalmed",
                    unworked = due.len() as u64,
                    "Working round with unworked cards, serving ready, citizens \
                     resident, and NO driver — the watchdog is reviving it now"
                );
            }
            for next in due {
                let airc = registry
                    .get(next.assignee)
                    .or_else(|| registry.any_live_citizen())
                    .map(|rt| rt.airc().clone());
                let Some(airc) = airc else {
                    crate::probe!(
                        class = "bench.round.resume_blocked",
                        reason = "no_citizen_runtime",
                        card_id = %next.card,
                        "resident roster answered but no runtime can author — retrying"
                    );
                    continue;
                };
                // RECONCILE BEFORE RE-FIRING: a settle that happened while a core
                // was down fired its event into the void, so the round may owe a
                // card the board already finished (or dropped). Read the run room's
                // board; a terminal or absent card is settled directly instead of
                // being re-fired forever.
                if let Some(state) = board_state_of(&airc, next.run_room, next.card).await {
                    match state {
                        BoardCardState::Terminal(s) => {
                            crate::probe!(
                                class = "bench.round.reconciled",
                                card_id = %next.card,
                                state = %s,
                                "card settled while a core was down — reconciled from                                  the board, not re-fired"
                            );
                            crate::cognition::bench_round::settle_card_direct(next.card, &s);
                            continue;
                        }
                        BoardCardState::Absent if attempt >= 4 => {
                            // ABSENT is negative evidence: post-boot the board may
                            // simply still be replicating — measured live 2026-08-26,
                            // a FRESH round's card read Absent minutes after boot and
                            // an eager reconcile settled a live card as a ghost
                            // (false completion, worse than retrying). Only after
                            // several spaced attempts (~5 min of misses) does Absent
                            // mean gone. Terminal reads stay immediate — a state is
                            // positive evidence.
                            crate::probe!(
                                class = "bench.round.reconciled",
                                card_id = %next.card,
                                state = "absent",
                                attempt = attempt as u64,
                                "card absent across several spaced attempts — settled \
                                 closed so the round completes instead of waiting on a ghost"
                            );
                            crate::cognition::bench_round::settle_card_direct(
                                next.card, "closed",
                            );
                            continue;
                        }
                        BoardCardState::Absent => {
                            crate::probe!(
                                class = "bench.round.reconcile_deferred",
                                card_id = %next.card,
                                attempt = attempt as u64,
                                "card not on the board yet — deferring judgment while \
                                 replication catches up; the dispatch attempt's own \
                                 named abort covers the still-absent case"
                            );
                        }
                        BoardCardState::Workable => {}
                    }
                }
                crate::probe!(
                    class = "bench.round.resumed",
                    card_id = %next.card,
                    assignee = %next.assignee,
                    run_room = %next.run_room,
                    attempt = attempt as u64,
                    "boot resume REJOINS the surviving round — re-firing its next unworked card"
                );
                crate::modules::work::dispatch_staged_swe_solve(
                    &Default::default(),
                    &airc,
                    crate::modules::work::StagedSolveDispatch {
                        claimer: crate::identity::PeerId::from_uuid(next.assignee),
                        card: airc_work::WorkCardId::from_uuid(next.card),
                        room: airc_core::RoomId::from_u128(next.run_room.as_u128()),
                    },
                )
                .await;
            }
            tokio::time::sleep(if fast { RETRY_SPACING } else { SLOW_WATCH }).await;
        }
    });
}

/// What the run room's board says about a card, read through the claimer's airc.
enum BoardCardState {
    /// On the board in a workable (non-terminal) state — re-fire it.
    Workable,
    /// On the board in a terminal state (the string is that state).
    Terminal(String),
    /// Not on the board at all.
    Absent,
}

/// `None` = the board could not be read (subscriptions resuming) — decide nothing.
async fn board_state_of(
    airc: &std::sync::Arc<airc_lib::Airc>,
    run_room: uuid::Uuid,
    card: uuid::Uuid,
) -> Option<BoardCardState> {
    let subs = airc.subscription_set().await.ok()?;
    let room = subs
        .all()
        .into_iter()
        .map(|s| s.as_room())
        .find(|r| r.channel.as_uuid() == run_room)?;
    let board = airc.work_board_in(&room).await.ok()?;
    let snapshot = board.snapshot();
    let Some(c) = snapshot
        .cards
        .iter()
        .find(|c| c.card_id.as_uuid() == card)
    else {
        return Some(BoardCardState::Absent);
    };
    let state = format!("{:?}", c.state).to_ascii_lowercase();
    if crate::cognition::bench_round::is_terminal_card_state(&state) {
        Some(BoardCardState::Terminal(state))
    } else {
        Some(BoardCardState::Workable)
    }
}
