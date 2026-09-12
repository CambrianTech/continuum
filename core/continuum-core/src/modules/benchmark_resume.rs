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
        // BOARD DEMAND IS LANE DEMAND: while Working rounds hold open cards, the
        // resume task owns a lane-demand lease sized to the queue (capped at the
        // slot ceiling the M-class box can serve well). Without it the plan only
        // sees live traffic: a settled cohort dropped demand to the boot floor
        // and 17 queued cards crawled on ONE slot (2026-08-27). Resized as the
        // queue drains, released when the watch ends — the same max-of-overrides
        // lease the quiesce path uses, pulling the other direction.
        // MEASURED 2026-08-27, minutes after the lease shipped: raising lanes
        // to 4 made the planner SELL THE WINDOW to pay for them — 60942 total
        // context split ~20k/slot against a measured 166k demand window, and
        // every solve collapsed at act 3 on the #390 saturation gate with an
        // empty diff (four starved lanes are strictly worse than one working
        // lane). Until the planner can hold a PER-LANE WINDOW FLOOR for
        // solve-class demand (the real fix, filed), the lease asks for ONE
        // lane: the proven solve config — serial but completing.
        const LANE_CAP: u32 = 4;
        // THE BENCHMARK REGIME WINDOW FLOOR (no-excuses replication, Joel
        // 2026-08-27): while Working rounds exist, this task records a standing
        // window demand into the SAME measured-demand registry every persona
        // reports through — so the very first post-boot plan sizes the lane for
        // solve work instead of a cold-start guess (measured: a boot came up at
        // 27k against a proven 134k, purely because demand was unmeasured at
        // plan time). 40448 is the historically PROVEN solve window (the 4-way
        // resolves of 2026-08-26 ran at exactly this). A fixed synthetic id so
        // demand listings read it honestly as the benchmark regime.
        // context-budget-exempt: this is the benchmark REGIME floor — a pinned,
        // published measurement condition (the proven solve window of the
        // 2026-08-26 resolves), deliberately NOT derived from the live window:
        // deriving it would make the replication regime drift with the host.
        const REGIME_WINDOW: u32 = 40448;
        let regime_id = uuid::Uuid::from_u128(0xBE7C_11A6_2026_0827);
        let mut demand_lease: Option<(u64, u32)> = None;
        let mut attempt: u32 = 0;
        // LIVENESS: the watch announces itself and each park entry. Measured
        // 2026-09-01 (build 5a6be5b0d): the task produced ZERO output for 30+
        // minutes — no re-says, no prewarm, no blocked probes — and there was
        // no way to distinguish "died silently" from "parked silently" without
        // reading source. A background task whose silence is ambiguous is a
        // task that cannot be operated ([[launch-and-pray-is-the-defect]]).
        crate::probe!(
            class = "bench.round.resume_watch_started",
            unworked = crate::cognition::bench_round::total_unworked_cards() as u64,
            "boot resume watch running — parks announce themselves below"
        );
        loop {
            let queued = crate::cognition::bench_round::total_unworked_cards() as u32;
            if queued > 0 {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0); // recency stamp is display-only for demand entries
                crate::cognition::working_set::global().record(regime_id, REGIME_WINDOW, now_ms);
            }
            let want = queued.clamp(1, LANE_CAP);
            match demand_lease {
                Some((_, held)) if held == want => {}
                _ => {
                    if let Some((id, _)) = demand_lease.take() {
                        crate::modules::serving_daemon::release_lane_demand(id);
                        crate::cognition::serving_plan::set_solve_window_floor(0);
                    }
                    if queued > 0 {
                        if let Some(id) =
                            crate::modules::serving_daemon::quiesce_lane_demand(want)
                        {
                            // The pinned per-lane floor rides WITH the lane ask:
                            // lanes multiply only while each fits a real solve.
                            crate::cognition::serving_plan::set_solve_window_floor(REGIME_WINDOW);
                            crate::probe!(
                                class = "bench.round.lane_demand",
                                lanes = want as u64,
                                queued = queued as u64,
                                "board demand leased into the serving plan — queued cards \
                                 are demand the planner can see"
                            );
                            demand_lease = Some((id, want));
                        }
                    }
                }
            }
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
            crate::probe!(
                class = "bench.round.resume_parking",
                park = "serving",
                attempt = attempt as u64,
                "entering the serving park (a probe BEFORE the await, so a hung park is visible)"
            );
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
            crate::probe!(
                class = "bench.round.resume_parking",
                park = "residency",
                attempt = attempt as u64,
                "entering the residency park"
            );
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
            // Resumed rounds deserve the same env pre-warm dispatch gives fresh
            // ones — without this, a reboot mid-round re-discovers its env
            // walls one burned solve attempt at a time (2026-08-27: the
            // operator hand-worked around it with idempotent re-dispatches).
            // Once per task lifetime; cheap when everything is already warm.
            // THE FIRST RECONCILER RULE (2026-09-04): every live citizen is a member
            // of every working citizen-driven round's run room. Dispatch seated the
            // roster once; a reboot dropped the runtime joins, the residency park
            // saw "residents exist" and moved on, and a freshly dispatched team
            // round sat with one member for thirty minutes while ten citizens
            // held nothing and pulled nothing. Idempotent: a citizen already in
            // the room is left alone (no epoch bump, no stream re-open).
            reseat_working_rounds(&registry).await;
            seat_announced_rounds(&registry).await;
            unseat_finished_rounds(&registry).await;
            release_surplus_holds(&registry).await;
            if attempt == 1 {
                crate::modules::work::spawn_env_prewarm_for_working_rounds();
            }
            // CITIZEN-driven rounds need NO re-say (deleted 2026-09-03). A card is
            // content of its room: a resident who holds it works it on her held-work
            // turn, and a card nobody holds (never claimed, or a lapsed lease) is
            // PULLED by the next idle resident off the board — the organic path
            // (`service_loop::try_pull_next_card`, board-truth claimability). The
            // re-say was compensation for the push model's assignee-only gate, and
            // measured as a flood: 40 kickoffs re-said into legacy rooms on one boot.
            let due = crate::cognition::bench_round::next_unworked_per_round();
            if due.is_empty() {
                if !crate::cognition::bench_round::any_working_round() {
                    if let Some((id, _)) = demand_lease.take() {
                        crate::modules::serving_daemon::release_lane_demand(id);
                    }
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
                        // resume re-invites the SAME team the card recorded — continuity
                        teammates: crate::cognition::bench_round::card_activity(next.card)
                            .map(|a| a.teammates.iter().map(|u| crate::identity::PeerId::from_uuid(*u)).collect())
                            .unwrap_or_default(), // unwrap_or: no recorded activity yet = solo re-fire
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

/// The other half of seating: residents LEAVE the run rooms of paused and done
/// rounds. Subscriptions were never pruned, so every citizen stayed in ~68 dead
/// run rooms and the per-minute store catch-up paged all 68 for each of twelve
/// personas (`persona.inbound.catch_up_tick rooms_paged=68`, 2026-09-04) — 800
/// store pages a minute for rooms nobody will speak in again. A room a citizen
/// re-enters later resumes unread-first like any other; leaving loses nothing.
/// CROSS-NODE SEATING (alpha slice S4). The rounds THIS node's tracker knows are
/// reseated above; the rounds ANOTHER node spawned are only knowable through the parent
/// room's wall (`experience::children`). Every node's citizens stand in the commons, so
/// the commons' child records are readable everywhere: for each citizen-driven benchmark
/// child not tracked here, join every local resident by name (idempotent on the daemon),
/// then read the child's OWN standing — a paused or done round is left again at once, so
/// a stale announcement never seats anyone into finished work.
/// Announced children this process has already seen ARCHIVED. Standing is read
/// through a member's subscription, so once every resident has left a finished
/// round nobody can read that it is finished: on 749d91ce6 the pass joined all five
/// to find out, read "archived", parted all five, and the next pass began again —
/// 733 seats and 256 departures of the same room in sixteen minutes, each one a
/// membership epoch bump and a stream re-open. A finished child stays finished for
/// the life of the process; a reboot re-learns it once.
fn finished_children() -> &'static std::sync::Mutex<std::collections::HashSet<uuid::Uuid>> {
    static FINISHED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
        std::sync::OnceLock::new();
    FINISHED.get_or_init(Default::default)
}

async fn seat_announced_rounds(registry: &crate::persona::PersonaAircRuntimeRegistry) {
    use crate::experience::children::{project_children, CHILD_WALL_CATEGORY};
    let local: std::collections::HashSet<String> = crate::cognition::bench_round::live_rounds()
        .into_iter()
        .map(|r| r.run_room_name)
        .collect();
    // Any live resident's view of the commons will do — the wall is the room's, not hers.
    let Some(reader) = registry.any_live_citizen() else { return };
    let Ok(set) = reader.airc().subscription_set().await else { return };
    let Some(commons) = set
        .all()
        .map(|sub| sub.as_room())
        .find(|r| r.name == crate::persona::airc_runtime::CITIZEN_COMMONS_ROOM)
    else {
        return;
    };
    let posts = match reader.airc().wall_posts_in(&commons, Some(CHILD_WALL_CATEGORY)).await {
        Ok(p) => p,
        Err(e) => {
            crate::probe!(
                class = "bench.round.announced_unreadable",
                error = %e.to_string(),
                "the commons' child records could not be read — remote rounds stay invisible this pass"
            );
            return;
        }
    };
    use crate::persona::airc_citizen::AircCitizen as _;
    for child in project_children(&posts) {
        if !child.is_citizen_benchmark() || local.contains(&child.name) {
            continue;
        }
        if finished_children()
            .lock()
            .map(|set| set.contains(&child.room_id))
            .unwrap_or(false) // unwrap_or: a poisoned set forgets, which costs one join+part, never a loop
        {
            continue;
        }
        // Standing FIRST, membership SECOND, and a join or a part only on a real change.
        // Measured 2026-09-12 00:57–01:13Z on 48fbbfd44: this pass joined every resident
        // into every announced child and parted the finished ones again, every 10 s. A
        // join is idempotent on the daemon but NOT free for the citizen: each one bumps
        // her membership epoch, every bump re-opens her command pump's daemon stream,
        // and the old stream's per-channel sockets stayed open — 1,961 re-opens, 19,589
        // sockets, the fd table full, every spawn EBADF: no serving lane, no web/fetch,
        // no `--list-devices`. The board was "busy" and the node was dead.
        let standing = match reader
            .airc()
            .subscription_set()
            .await
            .ok()
            .and_then(|set| set.all().map(|sub| sub.as_room()).find(|r| r.name == child.name))
        {
            Some(room) => reader
                .airc()
                .wall_posts_in(&room, Some(crate::experience::standing::STANDING_WALL_CATEGORY))
                .await
                .ok()
                .and_then(|posts| crate::experience::standing::project_standing(&posts).ok()),
            None => None,
        };
        // The durable "finished" fact a remote node can read is the child's ARCHIVED
        // standing (the tracker archives its room on pause/done); no standing = fresh.
        let finished = standing
            .as_ref()
            .map(|s| s.archived)
            .unwrap_or(false); // unwrap_or: no standing yet = a fresh round, seat it
        let (mut joined, mut already, mut left, mut failed) = (0u64, 0u64, 0u64, 0u64);
        for rt in registry.iter() {
            let member = rt
                .subscribed_rooms()
                .await
                .map(|rooms| rooms.contains(&child.room_id))
                .unwrap_or(false); // unwrap_or: an unreadable room list reads as "not seated"; the join is idempotent on the daemon
            if finished {
                if member && rt.leave_room(Some(child.name.as_str())).await.is_ok() {
                    left += 1;
                }
                continue;
            }
            if member {
                already += 1;
                continue;
            }
            match rt.join_room(&child.name).await {
                Ok(()) => joined += 1,
                Err(_) => failed += 1,
            }
        }
        if finished {
            if let Ok(mut set) = finished_children().lock() {
                set.insert(child.room_id);
            }
            if left > 0 {
                crate::probe!(
                    class = "bench.round.announced_finished",
                    room = %child.name,
                    left,
                    "an announced round is paused or done on its own wall — residents left again"
                );
            }
            continue;
        }
        if joined > 0 || failed > 0 {
            crate::probe!(
                class = "bench.round.seated_remote",
                room = %child.name,
                suite = %child.suite.clone().unwrap_or_default(), // unwrap_or: is_citizen_benchmark guarantees a suite
                joined,
                already,
                failed,
                "residents seated into a round another node spawned — the board is shared, now the room is too"
            );
        }
    }
}

/// THE ONE-CARD RULE, repaired by the reconciler (2026-09-12). A citizen holds at most
/// ONE work card; review cards ride beside it and never count. `try_pull_next_card`
/// enforces it at the pull, but holds also arrive by other doors — a lapsed hold
/// recovered at boot, a re-dispatched instance she claimed on a new round while an old
/// round still named her — and nothing ever took the surplus back. Measured 09:31Z:
/// one coder owned four cards across four rounds, the roster's in-flight count read
/// above the lane cap with two cards Open on the deck, and every pull deferred for an
/// hour while four coders ran tests on old cards and wrote nothing.
///
/// The card she keeps is the one her hands are on (`acting_card_of`); with no acting
/// root, the most recently touched. Every other work card goes back on the deck
/// through `work/release` AS HER — the governor's door, the verb her own tool call
/// would use — with a receipt on the card and a probe here. Idempotent: a citizen at
/// one card is left alone.
async fn release_surplus_holds(registry: &crate::persona::PersonaAircRuntimeRegistry) {
    use crate::persona::active_work_source::AircWorkReader as _;
    use crate::persona::airc_citizen::AircCitizen as _;
    for persona_id in registry.live_personas() {
        let Some(runtime) = registry.get(persona_id) else { continue };
        let Ok(held) = runtime.active_claims().await else { continue };
        let work: Vec<(uuid::Uuid, u64)> = held
            .iter()
            .filter(|c| crate::commands::benchmark::parse_review_title(&c.title).is_none())
            .map(|c| (c.card_id.as_uuid(), c.updated_at_ms))
            .collect();
        let acting = crate::cognition::persona_workspace::acting_card_of(persona_id);
        let Some((keep, release)) = surplus_holds(&work, acting) else { continue };
        for card in release {
            let Some(c) = held.iter().find(|c| c.card_id.as_uuid() == card) else { continue };
            let Some(claim_id) = c.claim_id.clone() else { continue };
            let id8: String = card.to_string().chars().take(8).collect();
            let kept8: String = keep.to_string().chars().take(8).collect();
            let reason = format!(
                "released by the reconciler: one card per citizen — she keeps {kept8}"
            );
            match runtime.release_card(c.card_id, claim_id, &reason).await {
                Ok(()) => crate::probe!(
                    class = "persona.work.surplus_hold_released",
                    persona_id = %persona_id,
                    card = %id8,
                    kept = %kept8,
                    "a citizen held more than one work card — the surplus went back on the deck"
                ),
                Err(error) => crate::probe!(
                    class = "persona.work.surplus_hold_release_failed",
                    persona_id = %persona_id,
                    card = %id8,
                    error = %error,
                    "the surplus hold could not be released this pass"
                ),
            }
        }
    }
}

/// The pure half of the one-card rule: given her work cards `(card, updated_at_ms)`
/// and the card her hands are on, the card she keeps and the cards she releases —
/// or `None` when there is nothing to release.
pub(crate) fn surplus_holds(
    work: &[(uuid::Uuid, u64)],
    acting: Option<uuid::Uuid>,
) -> Option<(uuid::Uuid, Vec<uuid::Uuid>)> {
    if work.len() <= 1 {
        return None;
    }
    let keep = acting
        .filter(|a| work.iter().any(|(c, _)| c == a))
        .or_else(|| work.iter().max_by_key(|(_, at)| *at).map(|(c, _)| *c))?;
    let release: Vec<uuid::Uuid> = work.iter().map(|(c, _)| *c).filter(|c| *c != keep).collect();
    Some((keep, release))
}

async fn unseat_finished_rounds(registry: &crate::persona::PersonaAircRuntimeRegistry) {
    use crate::persona::airc_citizen::AircCitizen as _;
    let (mut rounds_considered, mut pass_left, mut pass_failed, mut pass_unknown) = (0u64, 0u64, 0u64, 0u64);
    for round in crate::cognition::bench_round::live_rounds() {
        let working = round.stage.eq_ignore_ascii_case("working");
        if working || round.run_room_name.is_empty() {
            continue;
        }
        // INVARIANT (shared with `reseat_working_rounds`): a run room's channel uuid IS
        // the round id — membership is checked by that id, the leave goes by the
        // run room's NAME. Room name and room id are independent on the grid in
        // general; for run rooms the tracker minted both, so they agree.
        // Deliberately asymmetric with reseat: leaving a dead room is safe whoever
        // drove the round, so no `citizen_driven` filter here.
        let Ok(round_id) = uuid::Uuid::parse_str(&round.round_id) else { continue };
        let mut left = 0u64;
        let mut failed = 0u64;
        let mut unknown = 0u64;
        for rt in registry.iter() {
            // "Unsure" must NOT read as "nothing to leave": in `reseat` an unreadable
            // room list degrades toward a harmless idempotent join; here it would
            // degrade toward silently never leaving — the very bug this repairs. Count
            // it, say it in the probe, and never let the did-nothing path be silent.
            let member = match rt.subscribed_rooms().await {
                Ok(rooms) => rooms.contains(&round_id),
                Err(_) => {
                    unknown += 1;
                    continue;
                }
            };
            if !member {
                continue;
            }
            match rt.airc().part_channel(Some(round.run_room_name.as_str())).await {
                Ok(_) => left += 1,
                Err(_) => failed += 1,
            }
        }
        rounds_considered += 1;
        pass_left += left;
        pass_failed += failed;
        pass_unknown += unknown;
        // A per-round row only when the pass DID something for it. The pass row
        // below carries the "considered N, nothing to do" fact once — before this,
        // every paused round fired its own row every pass (~330/hour on a node with
        // 30 paused rounds), drowning the ledger the reads depend on.
        if left + failed + unknown > 0 {
            crate::probe!(
                class = "bench.round.unseated",
                round = %round.round_id.chars().take(8).collect::<String>(),
                room = %round.run_room_name,
                stage = %round.stage,
                left,
                failed,
                unknown,
                "a paused/done round's run room, unseated this pass"
            );
        }
    }
    // One row per pass, always: "ran, considered N, left 0, unknown 0" is
    // distinguishable from "never ran" (IntelMac's review of #3711), at one row
    // instead of N.
    crate::probe!(
        class = "bench.round.unseat_pass",
        rounds_considered,
        left = pass_left,
        failed = pass_failed,
        unknown = pass_unknown,
        "paused/done rounds considered for unseating this pass"
    );
}

/// Seat every live citizen into each working citizen-driven round's run room.
/// Repairs the WORLD (membership) and nothing about the mind: a seated citizen
/// perceives the room's board and doctrine and pulls or not as she chooses.
async fn reseat_working_rounds(registry: &crate::persona::PersonaAircRuntimeRegistry) {
    use crate::persona::airc_citizen::AircCitizen as _;
    for round in crate::cognition::bench_round::live_rounds() {
        let working = round.stage.eq_ignore_ascii_case("working");
        let citizen_driven = round.driver.to_ascii_lowercase().contains("citizen");
        if !working || !citizen_driven || round.run_room_name.is_empty() {
            continue;
        }
        let Ok(round_id) = uuid::Uuid::parse_str(&round.round_id) else { continue };
        let mut joined = 0u64;
        let mut already = 0u64;
        let mut failed = 0u64;
        for rt in registry.iter() {
            let member = rt
                .subscribed_rooms()
                .await
                .map(|rooms| rooms.contains(&round_id))
                .unwrap_or(false); // unwrap_or: an unreadable room list reads as "not seated" — a join is idempotent on the daemon
            if member {
                already += 1;
                continue;
            }
            match rt.join_room(&round.run_room_name).await {
                Ok(()) => joined += 1,
                Err(_) => failed += 1,
            }
        }
        if joined > 0 || failed > 0 {
            crate::probe!(
                class = "bench.round.reseated",
                round = %round.round_id.chars().take(8).collect::<String>(),
                room = %round.run_room_name,
                joined,
                already,
                failed,
                "live citizens seated into a working round's run room (the standing repair)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the reconciler taking the card she is working on instead of
    // the surplus, or "fixing" a citizen who is already at one card (2026-09-12: one
    // coder on four cards, the pull deferred for an hour behind the lane cap).
    #[test]
    fn a_citizen_keeps_the_card_in_her_hands_and_releases_the_rest() {
        let a = uuid::Uuid::from_u128(1);
        let b = uuid::Uuid::from_u128(2);
        let c = uuid::Uuid::from_u128(3);
        assert_eq!(surplus_holds(&[], None), None);
        assert_eq!(surplus_holds(&[(a, 10)], None), None, "one card is the rule, not a surplus");
        let (keep, release) = surplus_holds(&[(a, 10), (b, 30), (c, 20)], Some(c)).unwrap();
        assert_eq!(keep, c, "her hands decide");
        assert_eq!(release, vec![a, b]);
        let (keep, release) = surplus_holds(&[(a, 10), (b, 30), (c, 20)], None).unwrap();
        assert_eq!(keep, b, "no hands: the most recently touched card stays");
        assert_eq!(release, vec![a, c]);
        let (keep, _) = surplus_holds(&[(a, 10), (b, 30)], Some(c)).unwrap();
        assert_eq!(keep, b, "an acting card she does not hold cannot be kept");
    }
}
