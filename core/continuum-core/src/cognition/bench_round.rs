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

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;
use uuid::Uuid;

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
}

impl BenchRound {
    pub fn new(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) -> Self {
        Self {
            round_id,
            benchmark: benchmark.to_string(),
            cards: card_ids.iter().map(|c| (*c, None)).collect(),
            stage: RoundStage::Working,
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

/// Register a freshly dispatched round and announce it (`bench.round.staged`). Called by
/// `benchmark/dispatch` after its card loop, with the run room's uuid as the round id and
/// the FULL card uuids it posted. A dispatch that posted zero cards (everything skipped /
/// already on board) stages and immediately ends — an honest empty round, never a map
/// entry that no event can ever settle.
pub fn register_round(round_id: Uuid, benchmark: &str, card_ids: &[Uuid]) {
    crate::probe!(
        class = "bench.round.staged",
        round_id = %round_id,
        benchmark = %benchmark,
        cards = card_ids.len(),
        "benchmark round staged — cards posted, kickoffs sent, round is Working"
    );
    if card_ids.is_empty() {
        crate::probe!(
            class = "bench.round.done",
            round_id = %round_id,
            benchmark = %benchmark,
            dispatched = 0usize,
            settled = 0usize,
            "benchmark round END — nothing was dispatched"
        );
        return;
    }
    ROUNDS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(round_id, BenchRound::new(round_id, benchmark, card_ids));
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

    // what this catches: the END transition firing more than once. All-settled must yield
    // RoundDone exactly once — a re-delivered terminal event after Done must be a no-op,
    // or the room would get N "round over" announcements for one round.
    #[test]
    fn all_settled_transitions_to_done_exactly_once() {
        let ids = cards(3);
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids);
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
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids);
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
        let mut r = BenchRound::new(Uuid::new_v4(), "humaneval-rs", &ids);
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
