//! `BenchRound` — the round LIFECYCLE spine (#371): a dispatched benchmark round is an
//! ENTITY with a stage, not a pile of cards whose fate is probe archaeology.
//!
//! Before this, `benchmark/dispatch` posted cards + kickoffs and then NOTHING owned the
//! round: no stage announcements, no END — "random and directed by agent, not an
//! ecosystem" (Joel 2026-08-16). This module is the smallest true fix: dispatch registers
//! the round (its id IS the run room's id — dispatch already mints one per run, #329a;
//! never a second id), and the EXISTING `work.card.state_changed` bus event settles cards
//! one by one until the round is Done.
//!
//! Event-driven, never polling ([[the-whole-system-is-event-based-not-polling]]): there is
//! no tick here and no new bus receiver either — the one live consumer of
//! `WORK_CARD_STATE_CHANGED` (`modules::benchmark_grade`'s receiver task, the #450
//! subscriber) forwards every payload to [`observe_card_event`]. One subscription, two
//! reactions (grade the card, advance the round).
//!
//! Probes fire at TRANSITIONS ONLY:
//! - `bench.round.staged`   — at dispatch: round id, benchmark, card count.
//! - `bench.round.card_settled` — a card in the round reached a terminal state.
//! - `bench.round.done`     — the END, exactly once: every card settled.
//!
//! ## The round also owns WHO DRIVES its work ([`WorkDriver`])
//!
//! A benchmark card can be worked two ways, and the difference decides whether the round
//! teaches anybody anything. The round is the only thing that knows which, because the
//! decision is made once at dispatch and read later at claim time — in a different verb,
//! on a different task, with nothing threaded between them.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use uuid::Uuid;

/// Who actually does the work on this round's cards.
///
/// - [`DetachedSolve`](WorkDriver::DetachedSolve) — a `work/claim` (or a directed
///   dispatch) fires `agent/solve` for a FORKED copy of the citizen. Proven: it reaches
///   the repo, it grades, it produced our one SWE pass (#366). It also produces no room
///   turn, so `training_producer::produce` never runs and the round teaches nobody
///   (#456: the L2 producer is PATH-gated, and this is the path it excludes).
/// - [`Citizen`](WorkDriver::Citizen) — nothing detached fires. She claims the card in
///   her own service loop and works it on the held-work turn, which roots her hands at
///   the staged checkout and feeds the training producer. This is the path the learning
///   half of the objective requires — and the one that has never once been observed
///   end to end, because on the default path the detached solve always wins the claim.
///
/// `DetachedSolve` is the default and today's behaviour: an operator opts INTO the
/// citizen path per round. Both are real drivers, not a flag and a fallback — which is
/// why the choice is named on the round rather than hidden behind a `skip_solve` bool.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/WorkDriver.ts")]
pub enum WorkDriver {
    #[default]
    DetachedSolve,
    Citizen,
}

/// Where a round is in its life. `Working` from the moment dispatch returns (cards are
/// posted and kickoffs sent); `Done` when every card in the round's set has reached a
/// terminal card state. Two stages only — the smallest true lifecycle; claim/review
/// granularity already lives on the cards themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundStage {
    Working,
    Done,
}

/// Terminal card states — "this card's work is finished". The vocabulary is the serde
/// snake_case form of `airc_lib::CardState` as carried by the `work.card.state_changed`
/// payload: `work/state` maps done|closed → Closed and accepts merged. ONE truth for
/// "terminal": `modules::benchmark_grade::is_terminal` delegates here (compression
/// principle — the grader and the round tracker must never disagree about doneness).
pub(crate) fn is_terminal_card_state(state: &str) -> bool {
    matches!(
        state.to_ascii_lowercase().as_str(),
        "closed" | "done" | "merged"
    )
}

/// What one observed card-settle did to a round. Returned by [`BenchRound::settle_card`]
/// so the probe emission (and the tests) read the transition off the type instead of
/// re-deriving it from counters.
#[derive(Debug, PartialEq, Eq)]
enum SettleOutcome {
    /// The card is not in this round's set — not ours, ignore.
    NotOurs,
    /// The card was already settled (duplicate terminal event) — no state change.
    AlreadySettled,
    /// The card settled; `remaining` cards are still working.
    Settled { remaining: usize },
    /// This settle was the LAST one — the round just transitioned Working → Done.
    /// Fires exactly once per round by construction (the transition consumes it).
    RoundDone,
}

/// One benchmark round: the card set a single `benchmark/dispatch` posted, tracked from
/// dispatch (Working) to all-cards-terminal (Done). Identity is the run ROOM's uuid —
/// the id dispatch already mints per run; a round IS its room's activity.
pub struct BenchRound {
    round_id: Uuid,
    benchmark: String,
    /// Card uuid → the terminal state it settled with (`None` = still working).
    cards: HashMap<Uuid, Option<String>>,
    stage: RoundStage,
    /// Who works this round's cards — read at CLAIM time, decided at dispatch.
    driver: WorkDriver,
}

impl BenchRound {
    pub fn new(round_id: Uuid, benchmark: &str, card_ids: &[Uuid], driver: WorkDriver) -> Self {
        Self {
            round_id,
            benchmark: benchmark.to_string(),
            cards: card_ids.iter().map(|c| (*c, None)).collect(),
            stage: RoundStage::Working,
            driver,
        }
    }

    pub fn stage(&self) -> RoundStage {
        self.stage
    }

    /// Cards not yet settled.
    pub fn remaining(&self) -> usize {
        self.cards.values().filter(|s| s.is_none()).count()
    }

    /// Total cards this round dispatched.
    pub fn dispatched(&self) -> usize {
        self.cards.len()
    }

    /// Record that `card` reached terminal `state`. Pure state machine — the caller
    /// ([`observe_card_event`]) turns the outcome into probes. A card outside the set is
    /// `NotOurs`; a duplicate settle is `AlreadySettled` (never double-counted); the
    /// settle that empties the set transitions the round to `Done` exactly once.
    fn settle_card(&mut self, card: Uuid, state: &str) -> SettleOutcome {
        match self.cards.get_mut(&card) {
            None => SettleOutcome::NotOurs,
            Some(Some(_)) => SettleOutcome::AlreadySettled,
            Some(slot) => {
                *slot = Some(state.to_string());
                let remaining = self.remaining();
                if remaining == 0 && self.stage == RoundStage::Working {
                    self.stage = RoundStage::Done;
                    SettleOutcome::RoundDone
                } else {
                    SettleOutcome::Settled { remaining }
                }
            }
        }
    }
}

/// The live rounds this core is tracking, keyed by round (= run room) id. Process-global
/// for the same reason `modules::work::WORK_EVENT_BUS` is: the bus and the dispatch verb
/// are process-global, and the event consumer holds no path to a threaded-through field.
/// Sync `Mutex`, never held across an await (every touch is a short pure mutation).
/// A round is REMOVED the moment it completes, so the map only ever holds in-flight
/// rounds — no unbounded growth, and "done fires once" is structural.
static ROUNDS: LazyLock<Mutex<HashMap<Uuid, BenchRound>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Open a round BEFORE its first card is posted, so the driver is readable from the
/// instant a card can be claimed.
///
/// The ordering is load-bearing, not tidiness. `benchmark/dispatch` sends a kickoff
/// inside its card loop, so a citizen can claim card 1 while card 2 is still being
/// posted. If the round were only registered after the loop (as it was), that claim
/// would find no round, [`driver_for_card`] would answer with the default, and a
/// `Citizen`-driven round would silently fire the detached solver for its first card —
/// defeating exactly the thing the round was configured to do, with no error anywhere.
///
/// Idempotent by round id: re-opening a live round leaves it untouched.
pub fn open_round(round_id: Uuid, benchmark: &str, driver: WorkDriver) {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .entry(round_id)
        .or_insert_with(|| BenchRound::new(round_id, benchmark, &[], driver));
}

/// Add a freshly posted card to an open round. Unknown round = no-op (dispatch always
/// opens first; a card that arrives without one is not part of a tracked round).
pub fn add_card(round_id: Uuid, card_id: Uuid) {
    if let Some(r) = ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get_mut(&round_id)
    {
        r.cards.entry(card_id).or_insert(None);
    }
}

/// Close the dispatch phase and announce the round (`bench.round.staged`). Called by
/// `benchmark/dispatch` after its card loop. A round that posted zero cards (everything
/// skipped / already on board) stages and immediately ends — an honest empty round,
/// never a map entry that no event can ever settle.
pub fn seal_round(round_id: Uuid) {
    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    let Some(round) = rounds.get(&round_id) else {
        return;
    };
    let (benchmark, dispatched, driver) =
        (round.benchmark.clone(), round.dispatched(), round.driver);
    crate::probe!(
        class = "bench.round.staged",
        round_id = %round_id,
        benchmark = %benchmark,
        cards = dispatched,
        driver = ?driver,
        "benchmark round staged — cards posted, kickoffs sent, round is Working"
    );
    if dispatched == 0 {
        rounds.remove(&round_id);
        crate::probe!(
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
pub fn driver_for_card(card_id: Uuid) -> WorkDriver {
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
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

    let mut rounds = ROUNDS.lock().unwrap_or_else(|p| p.into_inner());
    // A card belongs to at most one round: dispatch's idempotence gate refuses a second
    // live card per task, and card uuids are minted per card. First match is THE match.
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return;
    };
    let round_id = round.round_id;
    match round.settle_card(card, state) {
        SettleOutcome::NotOurs | SettleOutcome::AlreadySettled => {}
        SettleOutcome::Settled { remaining } => {
            crate::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = remaining,
                "round card settled"
            );
        }
        SettleOutcome::RoundDone => {
            let (dispatched, benchmark) = (round.dispatched(), round.benchmark.clone());
            crate::probe!(
                class = "bench.round.card_settled",
                round_id = %round_id,
                card_id = %card,
                state = %state,
                remaining = 0usize,
                "round card settled"
            );
            crate::probe!(
                class = "bench.round.done",
                round_id = %round_id,
                benchmark = %benchmark,
                dispatched = dispatched,
                settled = dispatched,
                "benchmark round END — every card reached a terminal state"
            );
            rounds.remove(&round_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cards(n: usize) -> Vec<Uuid> {
        (0..n).map(|_| Uuid::new_v4()).collect()
    }

    /// Open a round and add its cards — the dispatch sequence, for tests that only care
    /// about the finished set.
    fn register_round(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) {
        open_round(round_id, benchmark, WorkDriver::default());
        for c in card_ids {
            add_card(round_id, *c);
        }
        seal_round(round_id);
    }

    // what this catches: the window between "card posted" and "round registered". Dispatch
    // sends a kickoff inside its card loop, so a citizen can claim card 1 while card 2 is
    // still being posted — and `work/claim` asks `driver_for_card` whether to fire the
    // detached solver. If the driver were only readable after the whole loop, a
    // Citizen-driven round would fire the solver on its first card and defeat itself
    // silently. The driver must be right from the first `add_card`, before `seal_round`.
    #[test]
    fn the_driver_is_readable_the_instant_a_card_can_be_claimed() {
        let round_id = Uuid::new_v4();
        let first = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::Citizen);
        add_card(round_id, first);
        // Mid-loop: the round is NOT sealed, a second card is not posted yet.
        assert_eq!(
            driver_for_card(first),
            WorkDriver::Citizen,
            "a claim landing mid-dispatch must see the round's real driver"
        );
        seal_round(round_id);
        assert_eq!(driver_for_card(first), WorkDriver::Citizen);
        ROUNDS.lock().unwrap().remove(&round_id);
    }

    // what this catches: the default biting the wrong way. A card belonging to no live
    // round — human-claimed, undirected, or a leftover claimed after its round ended —
    // must answer DetachedSolve, the proven path. Defaulting to Citizen would silently
    // stop firing solves for every ordinary claim on the box.
    #[test]
    fn a_card_in_no_round_drives_by_detached_solve() {
        assert_eq!(driver_for_card(Uuid::new_v4()), WorkDriver::DetachedSolve);
    }

    // what this catches: a round that dispatched nothing must END, not sit in the map
    // forever waiting for an event that can never arrive (no cards = no card events).
    #[test]
    fn an_empty_round_seals_straight_to_done_and_leaves_no_entry() {
        let round_id = Uuid::new_v4();
        open_round(round_id, "swe-bench-lite", WorkDriver::DetachedSolve);
        seal_round(round_id);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "an empty round must not remain tracked"
        );
    }

    // what this catches: the END transition firing more than once. All-settled must yield
    // RoundDone exactly once — a re-delivered terminal event after Done must be a no-op,
    // or the room would get N "round over" announcements for one round.
    #[test]
    fn all_settled_transitions_to_done_exactly_once() {
        let ids = cards(3);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.stage(), RoundStage::Working);
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 2 });
        assert_eq!(r.settle_card(ids[1], "merged"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::RoundDone);
        assert_eq!(r.stage(), RoundStage::Done);
        // Any further event on an already-settled card is inert — never a second Done.
        assert_eq!(r.settle_card(ids[2], "closed"), SettleOutcome::AlreadySettled);
        assert_eq!(r.stage(), RoundStage::Done);
    }

    // what this catches: a duplicate terminal event double-counting a card. The bridge
    // dedupes by event id, but a card can legitimately transition closed→merged; the
    // second settle must not decrement `remaining` a second time (a round declared Done
    // with cards still working is a lying END).
    #[test]
    fn a_card_settling_twice_does_not_double_count() {
        let ids = cards(2);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(ids[0], "closed"), SettleOutcome::Settled { remaining: 1 });
        assert_eq!(r.settle_card(ids[0], "merged"), SettleOutcome::AlreadySettled);
        assert_eq!(r.remaining(), 1, "the duplicate must not consume the other card's slot");
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: a card from OUTSIDE the round's dispatched set (another round's
    // card, a hand-written work card) advancing this round. Membership is the set
    // dispatch posted, nothing else.
    #[test]
    fn a_card_outside_the_set_is_ignored() {
        let ids = cards(1);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids, WorkDriver::default());
        assert_eq!(r.settle_card(Uuid::new_v4(), "closed"), SettleOutcome::NotOurs);
        assert_eq!(r.remaining(), 1);
        assert_eq!(r.stage(), RoundStage::Working);
    }

    // what this catches: the wire seam. `observe_card_event` must parse the #450 bridge
    // payload contract ({card_id: hyphenated uuid, state: serde CardState}) into the
    // state machine — and a non-terminal transition (in_progress) must not settle. Uses
    // the REAL global registry with a unique round, so it also pins register→observe→
    // remove-on-done end to end.
    #[test]
    fn observe_parses_the_bridge_payload_and_removes_the_round_on_done() {
        let ids = cards(1);
        let round_id = Uuid::new_v4();
        register_round(round_id, "humaneval-rs", &ids);
        let in_progress = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "in_progress",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&in_progress);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_some_and(|r| r.remaining() == 1),
            "a non-terminal transition must not settle the card"
        );
        let closed = serde_json::json!({
            "card_id": ids[0].to_string(),
            "state": "closed",
            "room_id": round_id.to_string(),
        });
        observe_card_event(&closed);
        assert!(
            ROUNDS.lock().unwrap().get(&round_id).is_none(),
            "the round completed and must be removed — done can never fire twice"
        );
    }
}
