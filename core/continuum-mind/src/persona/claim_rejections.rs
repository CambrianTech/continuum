//! Claim-rejection ring — durable perception for REJECTED claim attempts
//! (the #159-family sibling of the lost-claim transition fact).
//!
//! Glass-boxed live 2026-08-02: e5f4141d's claim on card 44ebaa41 was
//! rejected (held by 90e758b2) and she reported the rejection accurately
//! for three turns — then, once the raw action receipt scrolled out of
//! her 2–6 turn window, re-narrated "I've already claimed the task" and
//! planned work on a card she never held. The board's arbitration keeps
//! the system safe; the persona's TIME is what burns. The receipt's
//! lifetime was the bug: a rejection is a WORK-STATE fact, not a
//! transient tool result, so it must outlive the context squeeze.
//!
//! Mechanism: `work/claim` records each rejection here (identity =
//! the persona's airc peer uuid, the same key her held work is
//! read under, `persona::held_work`); her view renders the recent entries as `[work]` facts
//! for [`REJECTION_FACT_TTL`] — long enough to outlive the receipt's
//! window, short enough not to nag past relevance. Perception fact,
//! never a gate ([[no-hardcoded-heuristics-to-steer-cognition]]).
//!
//! Process-wide singleton, same shape as `install_tracked_dirs` /
//! `serving_active_artifacts`: the command layer and the RAG source
//! have no construction-time path to share state, and both already key
//! the same peer-uuid space.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use uuid::Uuid;

/// How long a rejection stays renderable. ~10 min ≈ several persona
/// polls — the receipt's 2–6 turn window plus margin, well short of
/// "nagging about last hour's board state".
const REJECTION_FACT_TTL: Duration = Duration::from_secs(10 * 60);

/// One rejection per card she named, the newest: a claim-storm on one card is one fact, and
/// how many cards' facts she sees is her view's budget (each is a keyed item there), never a
/// count decided here. The ring is bounded by the cards she tried within the TTL.
struct Entry {
    at: Instant,
    card_ref: String,
    line: String,
}

static RING: OnceLock<Mutex<HashMap<Uuid, Vec<Entry>>>> = OnceLock::new();

fn ring() -> &'static Mutex<HashMap<Uuid, Vec<Entry>>> {
    RING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record a rejected claim attempt for `persona`. `card_ref` is whatever
/// the persona passed (short id or UUID — echo HER handle back);
/// `error` is the substrate's rejection verbatim (no parsing, no
/// paraphrase — honest provenance).
pub fn record(persona: Uuid, card_ref: &str, error: &str) {
    record_at(persona, card_ref, error, Instant::now());
}

fn record_at(persona: Uuid, card_ref: &str, error: &str, now: Instant) {
    let line = format!(
        "[work] Your claim on card {card_ref} was REJECTED: {error}. The card is \
         NOT yours — do not plan or narrate work on it; check the board and pick \
         other work."
    );
    let Ok(mut map) = ring().lock() else { return };
    let entries = map.entry(persona).or_default();
    entries.retain(|e| e.card_ref != card_ref && now.duration_since(e.at) < REJECTION_FACT_TTL);
    entries.push(Entry { at: now, card_ref: card_ref.to_string(), line });
}

/// The persona's still-live rejection facts, newest last. Prunes expired
/// entries as it reads (no background sweeper needed for a bounded ring).
pub fn recent(persona: Uuid) -> Vec<String> {
    recent_at(persona, Instant::now())
}

fn recent_at(persona: Uuid, now: Instant) -> Vec<String> {
    let Ok(mut map) = ring().lock() else {
        return Vec::new();
    };
    let Some(entries) = map.get_mut(&persona) else {
        return Vec::new();
    };
    entries.retain(|e| now.duration_since(e.at) < REJECTION_FACT_TTL);
    let lines = entries.iter().map(|e| e.line.clone()).collect();
    if entries.is_empty() {
        map.remove(&persona);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the whole ring contract — a recorded rejection
    // renders for its TTL and ONLY for its persona, expires afterward
    // (no eternal nag), and a claim-storm on one card is ONE fact (its
    // newest), so repetition cannot flood perception.
    #[test]
    fn rejections_render_per_persona_expire_and_keep_one_per_card() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let t0 = Instant::now();

        record_at(a, "44ebaa41", "already claimed by another peer", t0);
        let facts = recent_at(a, t0 + Duration::from_secs(60));
        assert_eq!(facts.len(), 1);
        assert!(facts[0].contains("44ebaa41"));
        assert!(facts[0].contains("REJECTED"));
        assert!(facts[0].contains("already claimed by another peer"));
        assert!(recent_at(b, t0).is_empty(), "other persona sees nothing");

        // Past TTL: gone, and the persona's slot is cleaned up.
        assert!(recent_at(a, t0 + REJECTION_FACT_TTL + Duration::from_secs(1)).is_empty());

        // A storm on one card: six rejections are one fact, the newest, and it moves last.
        record_at(a, "card0", "held", t0);
        for i in 0..6 {
            record_at(a, "card1", &format!("held, attempt {i}"), t0);
        }
        let facts = recent_at(a, t0 + Duration::from_secs(1));
        assert_eq!(facts.len(), 2, "one fact per card: {facts:?}");
        assert!(facts[0].contains("card0"), "the other card's fact is kept");
        assert!(
            facts.last().is_some_and(|f| f.contains("card1") && f.contains("attempt 5")),
            "the storm's newest rejection is the one kept, newest last"
        );
    }
}
