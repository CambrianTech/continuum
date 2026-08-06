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

/// How much of `part` reappears in `whole`, in `0.0..=1.0` — ASYMMETRIC containment, the
/// sibling of [`text_similarity`] and deliberately NOT the same measure.
///
/// Jaccard asks "are these two texts the same?". Containment asks "did this text SWALLOW that
/// one?". The difference matters the moment one side is much longer: a persona who reproduces
/// a 30-token perception fact inside a 300-token message scores ~0.1 on Jaccard (diluted by
/// her own words) but 1.0 on containment. When the question is "did you echo what you were
/// told", dilution by verbosity must not launder the echo.
///
/// Shares [`content_tokens`] with `text_similarity` on purpose — one tokenizer, so a change to
/// what counts as a content word can never make the two measures disagree about a text.
pub fn containment(whole: &str, part: &str) -> f64 {
    let (tw, tp) = (content_tokens(whole), content_tokens(part));
    if tp.is_empty() {
        return 0.0;
    }
    tp.intersection(&tw).count() as f64 / tp.len() as f64
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

/// The default circling threshold: average mutual similarity across a persona's recent
/// output. 0.5 separates a pleasantry loop (near-identical restatements → ~0.7) from real
/// collaboration (varied, substantive → ~0.1–0.2).
pub const CIRCLING_THRESHOLD: f64 = 0.5;

/// True when the persona's OWN recent output is CIRCLING — its last `window` messages are
/// mutually low-novelty, i.e. it keeps saying variations of the same thing. The live
/// spiral was two personas politely complimenting each other forever; each was circling
/// on its own trajectory. A circling persona should stop adding to the loop even though
/// the room keeps pinging it — while genuine collaboration (varied, substantive replies)
/// has low mutual similarity and sails through untouched, so this never blocks real work.
///
/// Trajectory counterpart to [`is_self_repeat`] (single draft): checks the persona's own
/// recent messages against EACH OTHER, so it fires BEFORE generating another near-dup.
pub fn is_circling(recent_outputs: &[&str], window: usize, threshold: f64) -> bool {
    let n = recent_outputs.len();
    if window < 2 || n < window {
        return false; // not enough of my own recent turns to judge a trajectory
    }
    let recent = &recent_outputs[n - window..];
    let mut total = 0.0;
    let mut pairs = 0usize;
    for i in 0..recent.len() {
        for j in (i + 1)..recent.len() {
            total += text_similarity(recent[i], recent[j]);
            pairs += 1;
        }
    }
    pairs > 0 && total / pairs as f64 >= threshold
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

    // what this catches: THE stopping condition — a persona whose own recent output is
    // the pleasantry loop (near-identical restatements, from the real live spiral) reads
    // as circling and should stop; a persona doing genuine varied collaboration does NOT
    // read as circling, so real work is never blocked. Directly reproduces + gates the
    // Asha↔Anwen spiral.
    #[test]
    fn circling_flags_the_pleasantry_loop_not_real_collaboration() {
        // Verbatim-ish from the live spiral — a persona's own successive replies.
        let looping = [
            "You're very welcome, Anwen! It's always a pleasure collaborating with someone as enthusiastic and driven as you. Looking forward to many more productive sessions together!",
            "You're welcome, Anwen! It's always a pleasure to collaborate with someone as talented and enthusiastic as you. Looking forward to many more productive sessions together!",
            "You're very welcome, Anwen! It's always a pleasure collaborating with someone as passionate and driven as you. Looking forward to many more productive sessions together!",
        ];
        assert!(
            is_circling(&looping, 3, CIRCLING_THRESHOLD),
            "a persona re-emitting pleasantries must read as circling → stop the spiral"
        );

        // Genuine collaboration — varied, substantive turns.
        let working = [
            "The eviction policy should weigh usage frequency times salience.",
            "For the dedup, a counting bloom filter is O(1) and keeps memory flat.",
            "Watch the false-positive rate though — cap the filter at 0.1% or we drop live genomes.",
        ];
        assert!(
            !is_circling(&working, 3, CIRCLING_THRESHOLD),
            "varied substantive collaboration must NOT read as circling — never block real work"
        );

        // Too few turns to judge a trajectory → never circling.
        assert!(!is_circling(&looping[..1], 3, CIRCLING_THRESHOLD));
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
