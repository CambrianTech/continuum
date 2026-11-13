//! Deterministic self-repeat guard — suppress a draft that near-duplicates the persona's
//! OWN recent output, BEFORE it reaches the room.
//!
//! The live run exposed the gap: with two personas over airc, Asha re-emitted her eviction
//! policy almost verbatim on successive cognition cycles. The only redundancy mechanism then
//! in tree was unwired and routed to a cloud model (deleted since). So there was no gate at all; the
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

/// How many CONTENT tokens a text carries, by the one tokenizer all these measures share.
///
/// Exposed so a caller can ask whether a text is long enough for a containment score to MEAN
/// anything before it acts on one — see [`crate::cognition::parroted_perception`]. Sharing
/// `content_tokens` is the point: a guard that counted tokens differently from the measure it
/// guards would disagree with it about some text, which is the whole failure mode.
pub fn content_token_count(s: &str) -> usize {
    content_tokens(s).len()
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: similarity is bounded, symmetric, and 1.0 for identical text.
    #[test]
    fn similarity_is_bounded_and_identical_is_one() {
        assert_eq!(
            text_similarity("hello there world", "hello there world"),
            1.0
        );
        assert_eq!(text_similarity("", "anything"), 0.0);
        let s = text_similarity("the quick brown fox", "the lazy brown dog");
        assert!((0.0..=1.0).contains(&s) && s > 0.0 && s < 1.0);
    }
}
