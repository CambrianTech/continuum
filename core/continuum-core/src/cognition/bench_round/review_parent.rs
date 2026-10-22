use super::*;

/// The parent a review card reviews, if `card` is a live review card.
pub fn review_parent(card: Uuid) -> Option<Uuid> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .values()
        .find_map(|r| r.review_cards.get(&card).copied())
}

/// The reviewer's verdict landed: the review card retires; a pass unlocks the
/// parent's `done`. Returns the parent.
/// The review card opened for `parent`, if the gate opened one and it is still pending.
pub fn review_card_of(parent: Uuid) -> Option<Uuid> {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
    rounds
        .values()
        .find_map(|r| r.review_cards.iter().find(|(_, p)| **p == parent).map(|(rv, _)| *rv))
}

/// THE VERDICT LAW (card 52842311): a verdict settles every card of its instance.
/// Cards the tracker still counts as unsettled whose instance already HAS a verdict
/// (`lookup`), with that verdict — the ones a board move never reached (a parent
/// parked in REVIEW when the grade landed, a review card nobody pulled, a duplicate
/// in another round). The reconciler closes them each pass until the board agrees.
pub fn unsettled_cards_with<V>(lookup: impl Fn(&str) -> Option<V>) -> Vec<(Uuid, String, V)> {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // JUSTIFIED unwrap_or_else: poisoned lock = read the last state, same policy as every ROUNDS lock
    let mut out = Vec::new();
    for r in rounds.values() {
        if r.stage != RoundStage::Working {
            continue;
        }
        for (card, settled) in &r.cards {
            if settled.is_some() {
                continue;
            }
            let Some(instance) = r.card_instances.get(card) else { continue };
            if let Some(v) = lookup(instance) {
                out.push((*card, instance.clone(), v));
            }
        }
    }
    out
}

pub fn settle_review_card(review: Uuid, passed: bool) -> Option<Uuid> {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    let r = rounds.values_mut().find(|r| r.review_cards.contains_key(&review))?;
    let parent = r.review_cards.remove(&review)?;
    if passed {
        r.reviews_passed.insert(parent);
    }
    persist_round_in(&rounds_state_dir(), r);
    Some(parent)
}

/// Add a freshly posted card to an open round. Unknown round = no-op (dispatch always
/// opens first; a card that arrives without one is not part of a tracked round).
pub fn add_card(round_id: Uuid, card_id: Uuid) {
    if let Some(r) = ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .get_mut(&round_id)
    {
        r.cards.entry(card_id).or_insert(None);
        persist_round_in(&rounds_state_dir(), r);
    }
}

/// Close the dispatch phase and announce the round (`bench.round.staged`). Called by
/// `benchmark/dispatch` after its card loop. A round that posted zero cards (everything
/// skipped / already on board) stages and immediately ends — an honest empty round,
/// never a map entry that no event can ever settle.
pub fn seal_round(round_id: Uuid) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    let Some(round) = rounds.get(&round_id) else {
        return;
    };
    let (benchmark, dispatched, driver) =
        (round.benchmark.clone(), round.dispatched(), round.driver);
    continuum_base::probe!(
        class = "bench.round.staged",
        round_id = %round_id,
        benchmark = %benchmark,
        cards = dispatched,
        driver = ?driver,
        "benchmark round staged — cards posted, kickoffs sent, round is Working"
    );
    if dispatched == 0 {
        rounds.remove(&round_id);
        remove_round_file_in(&rounds_state_dir(), round_id);
        continuum_base::probe!(
            class = "bench.round.done",
            round_id = %round_id,
            benchmark = %benchmark,
            dispatched = 0usize,
            settled = 0usize,
            "benchmark round END — nothing was dispatched"
        );
    }
}

/// Who drives the work for this card — the question `work/claim` asks before deciding
/// whether to fire a detached solve.
///
/// A card belonging to no live round answers [`WorkDriver::DetachedSolve`]: that covers
/// a human-claimed card, an undirected board card, and a leftover claimed after its
/// round ended. All three are the proven path, so the default is the conservative one.
/// The round ROOM a card belongs to — which IS the round id (a round is its
/// room's activity). `None` for a card outside every tracked round (human-created
/// boards, ended rounds). The FOCUS rule reads this: a citizen's self-tick binds
/// to the room of her freshest live claim, so she stops alternating rooms —
/// measured 2026-08-22: two live claims in two rooms swapped her pinned slot's
/// room-scoped context every tick, `cached: 0` by her own hand.
/// The dispatch-time assignee of `card_id`, if the round recorded one. The LIVE holder is
/// the board's business (`verdict_board::holder_of`); this is the fallback when no board
/// read is possible.
pub fn card_assignee(card_id: Uuid) -> Option<Uuid> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .values()
        .find_map(|r| r.card_assignees.get(&card_id).copied())
}

pub fn room_for_card(card_id: Uuid) -> Option<Uuid> {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .map(|r| r.round_id)
}

/// The recorded activity for `card_id`'s solve, if one was ever minted — the
/// REJOIN half of mint-or-rejoin. Scans in-flight rounds (same shape as
/// [`room_for_card`]).
pub fn card_activity(card_id: Uuid) -> Option<CardActivity> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .and_then(|r| r.card_activities.get(&card_id).cloned())
}

/// The team standing in a solve ROOM — the inverse of [`card_activity`], for
/// callers that hold an activity's room but not its card (the experience
/// stream's grade-time seam). `None` = no tracked round runs a solve there
/// (solo work, non-bench rooms).
/// The RUN room of the round whose solve runs in `room` — the tree edge the
/// nav rail renders (#2632): a solve activity nests under its round. `None`
/// = no tracked round solves there (top-level activity).
pub fn run_room_for_solve(room: Uuid) -> Option<Uuid> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .find(|r| r.card_activities.values().any(|a| a.solve_room == room))
        .map(|r| r.round_id)
}

pub fn team_for_room(room: Uuid) -> Option<CardActivity> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .flat_map(|r| r.card_activities.values())
        .find(|act| act.solve_room == room)
        .cloned()
}

/// Record (idempotently) the activity `card_id`'s solve runs in — called at the
/// MINT, persisted with the round so a post-reboot re-fire rejoins it.
pub fn record_card_activity(card_id: Uuid, activity: CardActivity) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_activities.insert(card_id, activity);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// Record which citizen `card_id` was staged for — at DISPATCH time, so every
/// card has an owner before any solve fires.
pub fn record_card_assignee(card_id: Uuid, assignee: Uuid) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_assignees.insert(card_id, assignee);
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// WHAT one card tests, from the staged record or the activity room name —
/// so a re-say can name the instance the way the 2026-09-02 hand-written
/// operator note did (the note broke a wedged round; the substrate's own
/// kickoff must carry the same information or the hand stays in the loop).
/// Every card, across live rounds, that carries `instance` — the set a verdict
/// must close on the BOARD. A verdict settled the tracker but never the board
/// card, so a resolved instance read `open` and was re-claimed (sympy-22456,
/// 2026-09-07: resolved 14:13Z, re-claimed and re-worked by another citizen an
/// hour later).
pub fn cards_for_instance(instance: &str) -> Vec<Uuid> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: poisoned lock = read the last state, same as every reader here
    rounds
        .values()
        .flat_map(|r| {
            r.card_instances
                .iter()
                .filter(|(_, i)| i.as_str() == instance)
                .map(|(c, _)| *c)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn instance_for_card(card_id: Uuid) -> Option<String> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner()); // safe: poisoned lock = read the last state, same policy as every ROUNDS lock here
    rounds.values().find_map(|r| {
        if !r.cards.contains_key(&card_id) {
            return None;
        }
        r.card_instances
            .get(&card_id)
            .cloned()
            .filter(|i| !i.is_empty())
            .or_else(|| {
                r.card_activities.get(&card_id).and_then(|a| {
                    match instance_of_room_name(&a.room_name) {
                        "" => None,
                        parsed => Some(parsed.to_string()),
                    }
                })
            })
    })
}

/// WHAT a card tests, recorded at dispatch staging (the dispatcher holds the
/// instance in hand there) — see the `card_instances` field for why waiting
/// until the solve activity mints is too late.
pub fn record_card_instance(card_id: Uuid, instance: &str) {
    let mut rounds = ROUNDS.lock().expect("bench rounds mutex");
    if let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card_id)) {
        round.card_instances.insert(card_id, instance.to_string());
        persist_round_in(&rounds_state_dir(), round);
    }
}

/// The next card the round owes a solve — passed as a struct (typed UUIDs).
#[derive(Debug, Clone)]
pub struct NextCard {
    pub card: Uuid,
    pub assignee: Uuid,
    pub run_room: Uuid,
    /// The card's recorded team (empty = solo). Carried so every driver edge
    /// re-fires the solve WITH its team — a re-dispatch that drops teammates
    /// silently converts a team round into a solo round mid-flight.
    pub teammates: Vec<Uuid>,
}

/// ONE driver decision, three edges (dispatch, card-settled, boot resume): given
/// a card of some round, the FIRST still-unsettled card with no live run and a
/// known assignee — or None when the round is fully in flight / done / not a
/// detached round (Citizen rounds drive themselves through claims). Pure over
/// the round state plus the in-flight ledger, so every edge fires the same
/// decision — never a second scheduler.
pub fn next_unworked_after(card_of_round: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let round = rounds
        .values()
        .find(|r| r.cards.contains_key(&card_of_round))?;
    // The settle-driven follow-on is the DETACHED driver's motion: a citizen
    // round's next card is pulled by a resident, never pushed as a detached solve
    // (it would collide with WIP = lanes and take the card from the deck).
    if round.driver != WorkDriver::DetachedSolve {
        return None;
    }
    first_unworked(round, &live)
}

/// The NON-SETTLING edge of the same driver decision — the fourth edge.
///
/// A run can finish WITHOUT its card reaching a terminal state: she produced no
/// diff, the env was absent, the cached env would not re-point. Those are honest
/// outcomes (the card stays open for a retake) but they emit no card-settled
/// event, so the settle edge never fires and the round would sit still until the
/// 5-minute becalmed watchdog noticed. Measured 2026-08-28: a non-settling run at
/// 01:58 and another at 03:30 each left the round idle behind one finished solve.
/// A round that advances only by TIMEOUT is polling wearing an actuator's coat;
/// it should move the instant the fact is known ([[the-whole-system-is-event-based-not-polling]]).
///
/// `just_finished` is EXCLUDED, and that exclusion is the loop guard: the card is
/// still unsettled and no longer live, so the unguarded decision would hand back
/// the very card that just failed and re-fire it forever. The watchdog remains the
/// backstop for everything this edge cannot see (a process that died without
/// running its close path at all).
pub fn next_unworked_excluding(just_finished: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let round = rounds
        .values()
        .find(|r| r.cards.contains_key(&just_finished))?;
    first_unworked_excluding(round, &live, Some(just_finished))
}

/// The boot-resume edge of the SAME decision: the first unworked card of EVERY
/// Working detached round. Called once at benchmark-module boot (after the
/// serving + residency parks); the card-settled edge then chains the rest.
pub fn next_unworked_per_round() -> Vec<NextCard> {
    let live = live_run_ids();
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working)
        // Re-fire is the DETACHED driver's compensation only (plan S4): a citizen
        // round's cards are pulled by residents through `pullable_cards`, and a
        // watchdog pushing a detached solve onto one collides with WIP = lanes.
        .filter(|r| r.driver == WorkDriver::DetachedSolve)
        .filter_map(|r| first_unworked(r, &live))
        .collect()
}

/// The next card a MEMBER can PULL from a shared team deck — kanban PULL, not
/// push. The first non-terminal card, with no live run, in any Working Citizen
/// round the peer belongs to (in its `team`, or already an assignee somewhere in
/// it), that is NOT already assigned to a DIFFERENT member (never poach a
/// teammate's card; Open or hers is fair game). `assignee` is the PULLER — she is
/// claiming it. Personifies a human team: when free, you take the next card off
/// the board rather than working a fixed pile pushed onto you (Joel 2026-09-02).
/// Load-balances (a fast member pulls more) and is resilient (a dropped member's
/// un-pulled cards stay in the deck for anyone).
pub fn pullable_cards(
    peer: Uuid,
    resident_rooms: &std::collections::HashSet<Uuid>,
) -> Vec<NextCard> {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    let live = live_run_ids();
    // THE FULLEST DECK DRAINS FIRST: rounds ordered by remaining cards (desc), so a
    // fresh 12-card round is worked ahead of an old round's last stragglers. Map
    // order starved a new round entirely (measured: 9 pulls to the older round, 0 to
    // the new one in 20 min).
    let mut ordered: Vec<&BenchRound> = rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working && r.driver == WorkDriver::Citizen)
        .collect();
    // REAL WORK BEFORE LESSONS (ONE-RESIDENT §10.2): a coursework round never outranks a real
    // round for her next card, whatever their deck sizes — lessons fill time real work leaves.
    ordered.sort_by(|a, b| {
        let lesson = |r: &BenchRound| r.coursework.is_some();
        lesson(a)
            .cmp(&lesson(b))
            .then(b.remaining().cmp(&a.remaining()))
            .then(a.round_id.cmp(&b.round_id))
    });
    ordered
        .into_iter()
        // ELIGIBILITY IS RESIDENCY. She may pull from any run room she is standing in
        // — the round's `team` (really its reviewer set, empty unless `--teammates`)
        // and the dispatch-time assignee were the gate that locked 7 of 12 residents
        // out of a "shared" deck for months (2026-09-03). A card is room content; a
        // resident works it. The `run_room` IS the round id.
        .filter(|r| resident_rooms.contains(&r.round_id))
        .flat_map(|r| {
            // Review cards FIRST: a fix waiting on a verdict is the round's critical
            // path, and a reviewer costs a short read, not a solve.
            r.review_cards
                .keys()
                .copied()
                .chain(
                    r.cards
                        .iter()
                        .filter(|(_, state)| state.is_none())
                        .filter(|(c, _)| !live.contains(&format!("claim-{}", c)))
                        .map(|(c, _)| *c),
                )
                .map(|c| NextCard {
                    card: c,
                    assignee: peer,
                    run_room: r.round_id,
                    teammates: r.team.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The coursework lesson set a card belongs to (`coursework-<sha12>`), when its round is a
/// coursework round; `None` for real work and for a card no round tracks. A round's review cards
/// belong to its set too. Read when a turn stages, so the staged row carries the set as data
/// (ONE-RESIDENT §10.2: lesson turns are told from real work by data, never by inference).
pub fn coursework_set_of(card: Uuid) -> Option<crate::cognition::coursework::CourseworkSet> {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every ROUNDS lock
    rounds
        .values()
        .find(|r| r.cards.contains_key(&card) || r.review_cards.contains_key(&card))
        .and_then(|r| r.coursework.clone())
}

pub fn total_unworked_cards() -> usize {
    let rounds = ROUNDS.lock().expect("bench rounds mutex");
    rounds
        .values()
        .filter(|r| r.stage == RoundStage::Working)
        .map(|r| r.cards.values().filter(|s| s.is_none()).count())
        .sum()
}

/// Is any Working round NOT citizen-driven — the only shape whose queued cards are a
/// lane demand of their own (a detached solve holds exclusive lanes). Citizen rounds
/// are worked by the resident roster, which is already the demand.
pub fn any_working_detached_round() -> bool {
    ROUNDS
        .lock()
        .expect("bench rounds mutex")
        .values()
        .any(|r| r.stage == RoundStage::Working && !matches!(r.driver, WorkDriver::Citizen))
}

/// Are any Working rounds tracked at all — the boot resume's cheap early-exit.
pub fn any_working_round() -> bool {
    ROUNDS
        .lock()
        .expect("bench rounds mutex")
        .values()
        .any(|r| r.stage == RoundStage::Working)
}

pub(super) fn live_run_ids() -> std::collections::HashSet<String> {
    crate::cognition::swe_bench::in_flight_solve_runs()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

/// The one shared decision body behind both edges.
pub(super) fn first_unworked(round: &BenchRound, live: &std::collections::HashSet<String>) -> Option<NextCard> {
    first_unworked_excluding(round, live, None)
}

pub(super) fn first_unworked_excluding(
    round: &BenchRound,
    live: &std::collections::HashSet<String>,
    exclude: Option<Uuid>,
) -> Option<NextCard> {
    if round.driver != WorkDriver::DetachedSolve {
        return None;
    }
    // A paused round is HELD: cards stay, in-flight runs finish, and every
    // driver edge (settle-advance, non-settling advance, boot resume — all of
    // which funnel through here) hands out nothing until resume.
    if round.stage != RoundStage::Working {
        return None;
    }
    round
        .cards
        .iter()
        .filter(|(_, state)| state.is_none())
        .filter(|(c, _)| Some(**c) != exclude)
        .filter(|(c, _)| !live.contains(&format!("claim-{}", c)))
        .find_map(|(c, _)| {
            round.card_assignees.get(c).map(|a| NextCard {
                card: *c,
                assignee: *a,
                run_room: round.round_id,
                teammates: round
                    .card_activities
                    .get(c)
                    .map(|act| act.teammates.clone())
                    .unwrap_or_else(|| round.team.clone()), // no activity yet = the ROUND's team, never silently solo
            })
        })
}

/// Operator hold: flip a Working round to Paused (persisted). Returns false
/// when the round is unknown or already terminal.
pub fn pause_round(round_id: Uuid) -> bool {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    match rounds.get_mut(&round_id) {
        Some(r) if r.stage == RoundStage::Working => {
            r.stage = RoundStage::Paused;
            persist_round_in(&rounds_state_dir(), r);
            continuum_base::probe!(
                class = "bench.round.paused",
                round_id = %round_id,
                "round paused — in-flight solves finish, driver edges hand out nothing"
            );
            true
        }
        _ => false,
    }
}

/// Lift a pause: flip Paused → Working (persisted) and return the round's
/// first unworked card so the caller can kick the driver immediately instead
/// of waiting for the next settle edge.
pub fn resume_round(round_id: Uuid) -> Option<NextCard> {
    let live = live_run_ids();
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    let r = rounds.get_mut(&round_id)?;
    match r.stage {
        RoundStage::Paused => {
            r.stage = RoundStage::Working;
            persist_round_in(&rounds_state_dir(), r);
            continuum_base::probe!(
                class = "bench.round.resumed_by_operator",
                round_id = %round_id,
                "round resumed from pause — driver kicked"
            );
        }
        // A WORKING round can still be IDLE — the zombie state a core reload
        // leaves behind (found live 2026-09-01: a lite round reloaded
        // 'working, 4 remaining', demand leased, and sat a full day with
        // card_activities empty, because the settle-edge driver needs a
        // settle that an idle round can never produce). The verb's own doc
        // promises 'the driver is kicked immediately' — make that true here
        // too: no stage change, just the kick. The in-flight guard
        // (`live_run_ids` via `first_unworked`) keeps this idempotent — a
        // round with a genuinely live run returns None and nothing double-
        // fires ([[launch-and-pray-is-the-defect-read-the-state-pipe-before-staging-work]]).
        RoundStage::Working => {
            continuum_base::probe!(
                class = "bench.round.kicked_while_working",
                round_id = %round_id,
                "working round kicked by operator — firing the next unworked card if any run is not already live"
            );
        }
        _ => return None,
    }
    first_unworked(r, &live)
}

pub fn driver_for_card(card_id: Uuid) -> WorkDriver {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())  // poisoned lock = read the last state, same policy as every ROUNDS lock
        .values()
        .find(|r| r.cards.contains_key(&card_id))
        .map(|r| r.driver)
        .unwrap_or_default()
}

/// React to one `work.card.state_changed` payload (`{card_id, state, room_id}` — the
/// #450 bridge contract). Non-terminal transitions and cards outside every tracked round
/// are silently not-ours. Emits `bench.round.card_settled` per newly settled card and
/// `bench.round.done` exactly once when the last card lands (the round is removed on
/// that transition). Grade info is NOT in this payload — the grade runs minutes after
/// close — so the END probe reports totals, not verdicts.
/// Settle a card by DIRECT observation (not a wire event) — the reconcile path
/// for settles that happened while this core was down: a card closed during
/// downtime fired its state-change into the void, and the round would wait on
/// it forever (measured 2026-08-26: the boot resume retried a card the board no
/// longer offered, every attempt honestly aborting "not on the board"). Routes
/// through the SAME payload-shaped observer so probes and the Done transition
/// stay single-sourced.
/// A BOARD REOPEN REOPENS THE TRACKER. `work/state open` on a settled card moved the
/// board column back to Open, but the tracker kept its settled mark — so the pull,
/// which offers `tracker-unsettled ∩ board-claimable`, never offered it again
/// (2026-09-13 08:0xZ: matplotlib-21568 reopened after an ungradeable "no patch"
/// refusal; two idle coders read `dd712faf:1` claimable and `none_claimable`). One
/// truth per card: the board's column is the state, the tracker follows it.
pub fn reopen_card(card: Uuid) -> bool {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner()); // poisoned lock = read the last state, same policy as every ROUNDS lock
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return false;
    };
    if !round.reopen_card(card) {
        return false;
    }
    persist_round_in(&rounds_state_dir(), round);
    continuum_base::probe!(
        class = "bench.round.card_reopened",
        round_id = %round.round_id,
        card_id = %card,
        remaining = round.remaining(),
        stage = ?round.stage,
        "a settled card returned to the deck — the board reopened it, the tracker follows"
    );
    true
}

pub fn settle_card_direct(card: Uuid, state: &str) {
    observe_card_event(&serde_json::json!({
        "card_id": card.to_string(),
        "state": state,
    }));
}

pub fn observe_card_event(payload: &Value) {
    let state = payload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_terminal_card_state(state) {
        return;
    }
    let Some(card) = payload
        .get("card_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
    else {
        return;
    };

    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());  // poisoned lock = read the last state, same policy as every ROUNDS lock
    // A card belongs to at most one round: dispatch's idempotence gate refuses a second
    // live card per task, and card uuids are minted per card. First match is THE match.
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return;
    };
    let round_id = round.round_id;
    match round.settle_card(card, state) {
        SettleOutcome::NotOurs | SettleOutcome::AlreadySettled => {}
        SettleOutcome::Settled { remaining } => {
            crate::modules::citizen_health::note_settle();
            persist_round_in(&rounds_state_dir(), round);
            continuum_base::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = remaining,
                "round card settled"
            );
        }
        SettleOutcome::RoundDone => {
            crate::modules::citizen_health::note_settle();
            let (dispatched, benchmark) = (round.dispatched(), round.benchmark.clone());
            continuum_base::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = 0usize,
                "round card settled"
            );
            continuum_base::probe!(
                class = "bench.round.done",
                round_id = %round_id,
                benchmark = %benchmark,
                dispatched = dispatched,
                settled = dispatched,
                "benchmark round END — every card reached a terminal state"
            );
            rounds.remove(&round_id);
            remove_round_file_in(&rounds_state_dir(), round_id);
        }
    }
}

/// One in-flight round, as anyone may ask about it.
///
/// # Why this type exists (2026-08-18, and it is the acceptance test)
///
/// The round entity below has tracked stage, card set, and driver since it was written —
/// and NOTHING could ask it. Transitions fired probes and the state lived in a private
/// static, so "has the round started / is it stuck / is it done" was answerable only by
/// probe archaeology. That is precisely the failure
/// [ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md](../../../../docs/architecture/ROUND-LIFECYCLE-AS-RECIPE-OWNED-STATE-MACHINE.md)
/// names in its own acceptance test:
///
/// > *Can a fresh driver — with no memory of this session — answer "is it ready, has it
/// > started, is it stuck, is it done" using only queries, with zero log reads, zero probe
/// > archaeology, and zero inference from an absence?*
///
/// A probe is a transition RECORD. It tells you what happened when someone was watching.
/// It cannot answer a question asked later, which is when every question is actually asked.
///
/// **`settled` is reported explicitly rather than left to subtraction.** `dispatched -
/// remaining` is the same number, and making the reader compute it is how an absence
/// becomes a guess ([[an-absence-is-an-unfinished-measurement]]). Law 3 of the design doc:
/// *an absence is never a state.*
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/RoundSnapshot.ts"
)]
pub struct RoundSnapshot {
    /// When the round was opened (wall ms); 0 for rounds older than the field.
    #[serde(default)]
    #[ts(type = "number")]
    pub opened_at_ms: u64,
    /// The run room's airc name — what a navigator matches a rail tab against to
    /// label it by what it is (`verified · working · 9/12 in hands`) instead of
    /// its raw name (2026-09-04: 49 identical `bench-swe-bench-verified-mini-…` rows).
    pub run_room_name: String,
    /// The round id — which IS its run room's id. A round is its room's activity; there is
    /// never a second identifier ([[killing-a-derived-id-needs-a-directory-at-every-scope-boundary]]).
    pub round_id: String,
    pub benchmark: String,
    /// `working` | `done`. Present here means IN FLIGHT — a completed round is removed the
    /// instant it finishes, so `done` is only ever observed in the transition probe.
    pub stage: String,
    /// Cards this round dispatched.
    pub dispatched: usize,
    /// Cards that have reached a terminal state.
    pub settled: usize,
    /// Cards still working. Zero here with the round still listed would be a defect —
    /// the settle that empties the set is what removes it.
    pub remaining: usize,
    /// Who works these cards: `citizen` (in the room, produces turns, feeds the curriculum)
    /// or `detached_solve`. Decided at dispatch, read at claim time.
    pub driver: String,
    /// Per-card status — the row the board renders under the round. `default`
    /// so pre-cards wires still deserialize. Filled from the tracker by
    /// [`live_rounds`]; run facts (acts, patch, recency) merge in via
    /// [`enrich_rounds`] wherever a run-ledger scan is in hand.
    #[serde(default)]
    pub cards: Vec<RoundCardSnapshot>,
    /// Glanceable health: `unstarted` (no card has produced a single work
    /// artifact — the shape 2026-09-01 exposed: `working 0/8` for three hours
    /// was pixel-identical to grinding) | `grinding` (an unsettled card acted
    /// within the stall window) | `stalled` (work exists but nothing acted
    /// within it) | `paused` | `done`. Never derived client-side.
    #[serde(default)]
    pub verdict: String,
    /// Seconds since the newest work artifact across this round's unsettled
    /// cards. `None` = no card has ever produced one — an absence reported
    /// as an absence, never as `0` ([[an-absence-is-an-unfinished-measurement]]).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub idle_secs: Option<u64>,
}
