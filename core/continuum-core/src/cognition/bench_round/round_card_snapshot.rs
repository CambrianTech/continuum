use super::*;

/// One card of a round, as the board renders it: WHAT (instance), WHO
/// (assignee), WHERE (solve room), and how it is going (state, acts, patch,
/// recency). Tracker fields fill at [`live_rounds`]; run-ledger fields fill
/// at [`enrich_rounds`] and stay `None` until a run exists — a card with no
/// workspace renders honestly as `unstarted`, never as an empty "working".
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/RoundCardSnapshot.ts"
)]
pub struct RoundCardSnapshot {
    pub card_id: String,
    /// Instance under test, parsed from the solve room's name
    /// (`swe--<instance>--<card8>`). Empty until the card's activity is minted.
    pub instance: String,
    /// Solver NAME once a run ledger names one; else the staged assignee's
    /// uuid. Empty = never staged to anyone (itself a finding).
    pub assignee: String,
    /// The solve activity's airc name — the navigable door. Empty until minted.
    pub solve_room_name: String,
    /// `unstarted` (no run artifact) | the run phase (`active`, `quiet`,
    /// `ungraded`, …) while working | the terminal state once settled.
    pub state: String,
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    /// Seconds since this card's newest work artifact. `None` = none ever.
    #[ts(optional, type = "number")]
    pub last_act_secs: Option<u64>,
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// BOARD truth (the durable record): the card's column right now —
    /// `open|claimed|in_progress|blocked|review|merged|closed`. Empty when the
    /// board could not be read.
    #[serde(default)]
    pub board_state: String,
    /// Who holds it on the board (display name, else short id). Empty = nobody.
    #[serde(default)]
    pub owner: String,
    /// Board timestamps — the experiment axes: time-to-claim and time-to-settle
    /// read from these, never from a process clock that a reboot resets.
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub created_at_ms: Option<u64>,
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub updated_at_ms: Option<u64>,
    /// When the verdict file was written (`SweVerdict::graded_at_ms`).
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub graded_at_ms: Option<u64>,
}

/// The run-ledger facts [`enrich_rounds`] merges into a card row — a minimal
/// projection of `commands::benchmark::BenchRunCard`, defined HERE so the
/// dependency points commands→cognition like everything else.
pub struct CardRunFacts {
    /// The card this run was fired for, when the run id carries it (`claim-<card>-…`).
    /// A run that names a card enriches ONLY that card; the instance+time match below is
    /// the fallback for legacy runs with no card id (card 8a0ee845, 2026-09-06: a fresh
    /// team-round card read "resolved" from the paused control round's run on the same
    /// instance, seconds after dispatch).
    pub card_id: Option<String>,
    pub instance: String,
    pub solver: Option<String>,
    pub phase: String,
    pub acts: Option<u32>,
    pub patch_bytes: Option<u32>,
    pub last_activity_ms: u64,
    pub resolved: Option<bool>,
}

/// An unsettled card silent longer than this, on a round where work HAS
/// started, marks the round `stalled`. Acts land every 2–6 min when a citizen
/// is actually driving a workspace; 20 min of silence is a verdict, not noise.
pub(super) const ROUND_STALL_AFTER_SECS: u64 = 1200;

/// `swe--<instance>--<card8>` → `<instance>`. Empty in → empty out.
pub(super) fn instance_of_room_name(name: &str) -> &str {
    name.strip_prefix("swe--")
        .and_then(|rest| rest.rsplit_once("--").map(|(inst, _)| inst))
        .unwrap_or("")
}

/// Merge run-ledger facts into round snapshots and pronounce each round's
/// verdict. ONE fold, called by both consumers (the `benchmark/rounds`
/// command and the positron bench emitter) so a mind and a screen can never
/// disagree about whether a round is grinding.
///
/// Matching is by INSTANCE, newest run wins — two rounds sampling the same
/// instance share history, and the newest artifact is the one that answers
/// "is anyone on this now".
pub fn enrich_rounds(rounds: &mut [RoundSnapshot], runs: &[CardRunFacts], now_ms: u64) {
    for round in rounds.iter_mut() {
        let mut newest_unsettled_act: Option<u64> = None;
        let mut any_started = false;
        for card in round.cards.iter_mut() {
            let settled = !matches!(card.state.as_str(), "" | "unstarted");
            // A run counts for this card only if it is at least as new as the
            // round: run facts are keyed by instance and an older round on the same
            // instance must not lend a fresh card its verdict.
            let run = runs
                .iter()
                .filter(|r| match r.card_id.as_deref() {
                    // A run that names its card belongs to that card and no other.
                    Some(id) => id == card.card_id,
                    // Legacy run with no card id: instance + not older than the round.
                    None => {
                        !card.instance.is_empty()
                            && r.instance == card.instance
                            && r.last_activity_ms >= round.opened_at_ms
                    }
                })
                .max_by_key(|r| r.last_activity_ms);
            let Some(run) = run else {
                // No detached-solve ledger entry — but a CITIZEN-driven card
                // leaves no ledger, so fall back to the held-work freshness the
                // turn boundary stamps. A recent held-work turn makes the card
                // read "working" (not "unstarted") and feeds the round's idle
                // clock, so the standing autopilot sees the round is being worked
                // and holds instead of piling up duplicates.
                if !settled {
                    let held_act = card
                        .card_id
                        .parse::<Uuid>()
                        .ok()
                        .and_then(card_last_act_ms);
                    if let Some(act_ms) = held_act {
                        any_started = true;
                        let act_age = now_ms.saturating_sub(act_ms) / 1000;
                        card.last_act_secs = Some(act_age);
                        card.state = "working".to_string();
                        newest_unsettled_act =
                            Some(newest_unsettled_act.map_or(act_age, |a: u64| a.min(act_age)));
                    } else {
                        card.state = "unstarted".to_string();
                    }
                }
                continue;
            };
            any_started = true;
            if let Some(solver) = &run.solver {
                card.assignee = solver.clone();
            }
            card.acts = run.acts;
            card.patch_bytes = run.patch_bytes;
            card.resolved = run.resolved;
            let act_age = now_ms.saturating_sub(run.last_activity_ms) / 1000;
            card.last_act_secs = Some(act_age);
            if !settled {
                card.state = run.phase.clone();
                newest_unsettled_act =
                    Some(newest_unsettled_act.map_or(act_age, |a: u64| a.min(act_age)));
            }
        }
        round.idle_secs = newest_unsettled_act;
        round.verdict = match round.stage.as_str() {
            "paused" => "paused".to_string(),
            "done" => "done".to_string(),
            _ => match newest_unsettled_act {
                None if !any_started => "unstarted".to_string(),
                // Every card WITH a run is settled; unsettled ones never started.
                None => "unstarted".to_string(),
                Some(age) if age > ROUND_STALL_AFTER_SECS => "stalled".to_string(),
                Some(_) => "grinding".to_string(),
            },
        };
    }
}

/// Every round this core is tracking, in a stable order.
///
/// Sorted by round id so two calls a second apart cannot reorder rows under a reader —
/// a projection whose row order depends on `HashMap` iteration teaches its consumers to
/// distrust it.
/// Every live round's ACTIVITY rooms — `(solve_room, airc_name)` for each
/// card that has one, skipping rows whose name predates `room_name` (empty).
/// The presence emitter merges these with the registry rooms so per-run
/// rosters and transcripts reach the interface (#2606 / #2632: the academy's
/// children are rooms, and a room without presence renders blank).
pub fn activity_rooms() -> Vec<(Uuid, String)> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    rounds
        .values()
        .filter(|r| r.stage != RoundStage::Done)
        .flat_map(|r| {
            let run = (!r.run_room_name.is_empty())
                .then(|| (r.round_id, r.run_room_name.clone()));
            run.into_iter().chain(
                r.card_activities
                    .values()
                    .filter(|a| !a.room_name.is_empty())
                    .map(|a| (a.solve_room, a.room_name.clone()))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Cards a citizen is actively on across every WORKING citizen-driven round —
/// claimed or in progress on the board. A card in `review` is not counted: its
/// owner is done and the slot it held is what lets a peer pull its review card.
/// This is the number WIP-as-lanes compares to the served lane count.
pub fn in_flight_citizen_cards() -> usize {
    live_rounds()
        .iter()
        .filter(|r| r.stage.eq_ignore_ascii_case("working") && r.driver.to_ascii_lowercase().contains("citizen"))
        .flat_map(|r| r.cards.iter())
        .filter(|c| !c.owner.is_empty())
        .filter(|c| {
            let s = c.board_state.to_ascii_lowercase();
            s == "claimed" || s == "in_progress"
        })
        .count()
}

/// Is the round holding this card still WORKING? `true` for a card no round
/// knows (ordinary work). A paused/done round's card must not be re-claimed by
/// a lapsed-hold recovery: the board keeps the last owner on a reopened card,
/// and on 2026-09-05 the recovery pulled ten citizens back onto a paused round.
pub fn card_round_is_working(card_id: Uuid) -> bool {
    let rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    match rounds.values().find(|r| r.cards.contains_key(&card_id)) {
        Some(r) => r.stage == RoundStage::Working,
        None => true,
    }
}

pub fn live_rounds() -> Vec<RoundSnapshot> {
    let rounds = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: Vec<RoundSnapshot> = rounds
        .values()
        .map(|r| {
            let mut cards: Vec<RoundCardSnapshot> = r
                .cards
                .iter()
                .map(|(card_id, settled)| {
                    let activity = r.card_activities.get(card_id);
                    let assignee = activity
                        .map(|a| a.assignee)
                        .or_else(|| r.card_assignees.get(card_id).copied())
                        .map(|u| u.to_string())
                        .unwrap_or_default();
                    let room_name = activity
                        .map(|a| a.room_name.clone())
                        .unwrap_or_default();
                    // Activity room name is the authority once minted; the
                    // staging-time record covers every card before that.
                    let instance = match instance_of_room_name(&room_name) {
                        "" => r.card_instances.get(card_id).cloned().unwrap_or_default(),
                        parsed => parsed.to_string(),
                    };
                    RoundCardSnapshot {
                        card_id: card_id.to_string(),
                        instance,
                        assignee,
                        solve_room_name: room_name,
                        // Terminal state from the tracker; open cards report
                        // "unstarted" until enrich_rounds sees a run artifact.
                        state: settled.clone().unwrap_or_else(|| "unstarted".to_string()),
                        acts: None,
                        patch_bytes: None,
                        last_act_secs: None,
                        resolved: None,
                        board_state: String::new(),
                        owner: String::new(),
                        created_at_ms: None,
                        updated_at_ms: None,
                        graded_at_ms: None,
                    }
                })
                .collect();
            cards.sort_by(|a, b| a.instance.cmp(&b.instance).then(a.card_id.cmp(&b.card_id)));
            RoundSnapshot {
                opened_at_ms: r.opened_at_ms,
                round_id: r.round_id.to_string(),
                run_room_name: r.run_room_name.clone(),
                benchmark: r.benchmark.clone(),
                stage: match r.stage {
                    RoundStage::Working => "working",
                    RoundStage::Paused => "paused",
                    RoundStage::Done => "done",
                }
                .to_string(),
                dispatched: r.dispatched(),
                settled: r.dispatched().saturating_sub(r.remaining()),
                remaining: r.remaining(),
                driver: match r.driver {
                    WorkDriver::Citizen => "citizen",
                    WorkDriver::DetachedSolve => "detached_solve",
                }
                .to_string(),
                cards,
                // Honest defaults until enrich_rounds runs with ledger facts.
                verdict: String::new(),
                idle_secs: None,
            }
        })
        .collect();
    out.sort_by(|a, b| a.round_id.cmp(&b.round_id));
    out
}
