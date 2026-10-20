use super::*;
/// THE VERDICT LAW (card 52842311): cards the tracker still counts as unsettled whose
/// instance already HAS a verdict (`lookup`), with that verdict.
pub fn unsettled_cards_with<V>(lookup: impl Fn(&str) -> Option<V>) -> Vec<(Uuid, String, V)> {
    let rounds = rounds();
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

/// A BOARD REOPEN REOPENS THE TRACKER: the board's column is the state, the tracker
/// follows it. True when the tracker held the card settled.
pub fn reopen_card(card: Uuid) -> bool {
    let mut rounds = rounds();
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return false;
    };
    if !round.reopen_card(card) {
        return false;
    }
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

/// Advance the round a card-state event names. Non-terminal and malformed payloads are
/// not ours.
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
    let mut rounds = rounds();
    let Some(round) = rounds.values_mut().find(|r| r.cards.contains_key(&card)) else {
        return;
    };
    let round_id = round.round_id;
    match round.settle_card(card, state) {
        SettleOutcome::NotOurs | SettleOutcome::AlreadySettled => {}
        SettleOutcome::Settled { remaining } => {
            crate::modules::citizen_health::note_settle();
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

/// Every live round's room and its cards' activity rooms, as `(room, name)`, skipping
/// rows with no name. The presence emitter merges these with the registry rooms.