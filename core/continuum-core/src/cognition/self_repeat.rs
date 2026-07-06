//! Deterministic self-repeat guard — suppress a draft that near-duplicates the persona's
//! OWN recent output, BEFORE it reaches the room.
//!
//! The live run exposed the gap: with two personas over airc, Asha re-emitted her eviction
//! policy almost verbatim on successive cognition cycles. The only redundancy mechanism in
//! tree (`check_redundancy`) isn't wired into the live loop AND routes to groq (cloud) — a
//! non-starter on an offline, local-first misfit box. So there was no gate at all; the
//! `should_respond` LLM even *encourages* re-answering ("respond if you have a different
//! angle"), and the persona rationalizes one every cycle.
//!
//! This guard is local + deterministic + instant (no LLM, no network) — exactly what a
//! room full of personas doing things in parallel needs so they don't talk over themselves.
//! It complements, not replaces, a semantic check: this catches the cheap, common,
//! offline-safe case (I just said this); a smarter judge can still catch subtler repeats.

use std::collections::HashSet;

/// Content tokens of a message (lowercased, alphanumeric, length > 2) for cheap overlap.
fn content_tokens(s: &str) -> HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(String::from)
        .collect()
}

/// Jaccard similarity of two texts' content-token sets, in `0.0..=1.0`. Order- and
/// length-insensitive, so a lightly-reworded restatement still scores high.
pub fn text_similarity(a: &str, b: &str) -> f64 {
    let (ta, tb) = (content_tokens(a), content_tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    inter / union
}

/// The default self-repeat threshold. 0.8 catches a reworded restatement (Asha's
/// near-identical policy) while leaving room for a genuinely new point that happens to
/// reuse the same vocabulary.
pub const SELF_REPEAT_THRESHOLD: f64 = 0.8;

/// True when `draft` near-duplicates any of the persona's own `recent_outputs` — a repeat
/// to suppress rather than broadcast. Compares ONLY against the persona's own prior output
/// (self-repeat), never against other speakers — echoing someone else is a different
/// concern, and agreeing with a peer is legitimate.
pub fn is_self_repeat(draft: &str, recent_outputs: &[String], threshold: f64) -> bool {
    recent_outputs
        .iter()
        .any(|prev| text_similarity(draft, prev) >= threshold)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a NEAR-VERBATIM restatement of the persona's own last message is
    // flagged (the exact live failure — Asha re-emitting her yield answer almost word for
    // word on the next cycle), while a genuinely different point is NOT flagged. The 0.8
    // threshold is deliberately high-precision: catch the egregious repeat, never silence a
    // real new contribution. (A semantic judge can catch softer paraphrase-repeats later.)
    #[test]
    fn flags_near_verbatim_self_repeat_not_new_points() {
        let prior = "When my genome is about to be evicted under pressure, I believe it is \
                     best to gracefully yield rather than negotiate to stay, so the system \
                     keeps optimal performance and I re-page when resources return.";
        // Near-verbatim re-emission (a couple words changed) → repeat.
        let repeat = "When my genome is about to be evicted under pressure, I think it is \
                      best to gracefully yield rather than negotiate to stay, so the system \
                      keeps optimal performance and I re-page once resources return.";
        assert!(is_self_repeat(repeat, &[prior.to_string()], SELF_REPEAT_THRESHOLD));

        // A genuinely new point (different content) → not a repeat.
        let new_point = "Actually, the harder question is whether the room should get a vote \
                         before any one persona's genome is paged out — a fairness quorum.";
        assert!(!is_self_repeat(new_point, &[prior.to_string()], SELF_REPEAT_THRESHOLD));

        // No prior output → never a repeat.
        assert!(!is_self_repeat(repeat, &[], SELF_REPEAT_THRESHOLD));
    }

    // what this catches: similarity is bounded, symmetric, and 1.0 for identical text.
    #[test]
    fn similarity_is_bounded_and_identical_is_one() {
        assert_eq!(text_similarity("hello there world", "hello there world"), 1.0);
        assert_eq!(text_similarity("", "anything"), 0.0);
        let s = text_similarity("the quick brown fox", "the lazy brown dog");
        assert!((0.0..=1.0).contains(&s) && s > 0.0 && s < 1.0);
    }
}
