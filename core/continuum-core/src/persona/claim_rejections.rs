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
//! the persona's airc peer uuid, the same key `ActiveWorkSource` is
//! bound to); the source renders the recent entries as `[work]` facts
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

/// Ring cap per persona — a claim-storm must not flood perception; the
/// newest few rejections carry all the signal.
const MAX_PER_PERSONA: usize = 4;

struct Entry {
    at: Instant,
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
    entries.push(Entry { at: now, line });
    if entries.len() > MAX_PER_PERSONA {
        let overflow = entries.len() - MAX_PER_PERSONA;
        entries.drain(..overflow);
    }
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
    // (no eternal nag), and the per-persona cap drops oldest-first so a
    // claim-storm cannot flood perception.
    #[test]
    fn rejections_render_per_persona_expire_and_cap() {
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

        // Cap: 6 recorded → the newest MAX_PER_PERSONA survive.
        for i in 0..6 {
            record_at(a, &format!("card{i}"), "held", t0);
        }
        let facts = recent_at(a, t0 + Duration::from_secs(1));
        assert_eq!(facts.len(), MAX_PER_PERSONA);
        assert!(
            facts.last().is_some_and(|f| f.contains("card5")),
            "newest kept"
        );
        assert!(!facts.iter().any(|f| f.contains("card0")), "oldest dropped");
    }
}
