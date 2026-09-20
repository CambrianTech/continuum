//! `token_budget` — the ONE estimator for "how much of the prompt does this
//! piece of content cost?"
//!
//! Every RAG/prompt layer a persona assembles (recall engrams, roster, doctrine,
//! room purpose, workspace map, the airc thread) competes for a finite token
//! budget against the base model's context window. To "take no prompt layer for
//! granted" you must be able to put a number on each layer — and that number has
//! to be the SAME unit everywhere, or the ledger lies.
//!
//! Today the same `((content.chars().count() / 4) as u32).saturating_add(1)` is
//! copy-pasted private into ~6 `persona/*_source.rs` files (one of them on bytes,
//! not chars — already drifting). This is the canonical home they converge onto
//! (task: collapse the 6 private copies onto this). The `cognition/replay` budget
//! ledger uses THIS so its per-layer numbers match what the sources budgeted
//! against — a ledger in different units than the allocator would be a lie.
//!
//! It is a deterministic ESTIMATE (chars/4), not a model tokenizer: it is a
//! measurement instrument for relative layer cost and total-vs-window, run on the
//! cold path, and it must never depend on a loaded model (which may not be
//! resident when you replay). When an exact count is needed against a specific
//! model, that is the tokenizer's job at the inference seam, not this.

/// Estimate the prompt-token cost of a piece of content. Char-based (handles
/// multibyte text correctly), `/4` for the standard ~4-chars-per-token heuristic,
/// `+1` so non-empty content is never costed at zero.
pub fn estimate_prompt_tokens(content: &str) -> u32 {
    if content.is_empty() {
        return 0;
    }
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Keep the HEAD of `content` up to ~`budget_tokens` (same chars/4 unit as
/// [`estimate_prompt_tokens`]), cutting on a char boundary and preferring the
/// last newline inside the kept slice so the cut lands between lines, not
/// mid-word. Used when a RAG source explicitly offers a shortened projection;
/// deliberation fitting preserves complete current stimulus/action payloads
/// and evicts whole historical messages. Used by the breadth-over-depth
/// packer (#128): long turns render as heads with an explicit trim marker so
/// a small budget holds MANY turns instead of two verbatim essays.
pub fn head_to_tokens(content: &str, budget_tokens: u32) -> String {
    let budget_chars = (budget_tokens as usize).saturating_mul(4);
    if content.chars().count() <= budget_chars {
        return content.to_string();
    }
    let end = content
        .char_indices()
        .nth(budget_chars)
        .map(|(i, _)| i)
        .unwrap_or(content.len());
    let slice = &content[..end];
    match slice.rfind('\n') {
        // Only prefer the newline cut when it keeps a useful fraction — a
        // message whose only newline sits three chars in must not collapse
        // to three characters.
        Some(nl) if nl >= budget_chars / 2 => slice[..nl].to_string(),
        _ => slice.to_string(),
    }
}

/// The ONE vocabulary for telling a mind her copy of a message was shortened.
///
/// Two sites originally shortened a message on the way into a mind and disagreed about
/// whether to say so: the per-turn head trim (`persona::airc_source`) appended
/// `(…N-token message trimmed)`, while the newest-message tail trim
/// (`cognition::llm_deliberation_faculty`) returned the body BARE. An unmarked fragment
/// does not read as a fragment — it reads as a whole message that begins mid-sentence,
/// so the rational thing for a citizen to do is try to reconstruct an intent that was
/// never delivered. Measured 2026-09-06 (card 3833b472): citizens on two nodes did
/// exactly that, one of them for five consecutive turns, against text like ", so it is …".
/// The deliberation tail cut was removed by f09424d4: current stimulus/action
/// payloads now remain complete or produce a capacity fault. These markers still
/// describe explicit shortened projections; a marker alone is not a payload.
///
/// Keeping both spellings here means the next site that shortens something inherits the
/// disclosure instead of re-deciding it ([[logic-deepest-level-thin-on-the-way-out]]).
///
/// Which end carries the marker follows what is MISSING: a head-keep lost its tail, so
/// the marker trails; a tail-keep lost its opening, so the marker leads — the citizen
/// meets the notice before the text it qualifies.
pub fn mark_head_kept(head: &str, full_tokens: u32) -> String {
    format!("{head} (…{full_tokens}-token message trimmed)")
}

/// Sibling of [`mark_head_kept`] for a tail-keep: the marker LEADS, because the opening
/// is what went missing and a citizen reading left-to-right must learn that first.
pub fn mark_tail_kept(tail: &str, full_tokens: u32) -> String {
    format!("(…{full_tokens}-token opening trimmed) {tail}")
}

/// Tokens to reserve for a trim marker BEFORE trimming, so the disclosure cannot itself
/// push the message back over the budget that forced the trim.
///
/// A FUNCTION, not a `const`, for the same reason [`super::viewstate_rag`]'s floor is:
/// `context_budget`'s de-hardcode guard scans `const`s whose name contains TOKEN for bare
/// literals, and it is right to — a bare literal here is a prompt-size constant wearing a
/// different hat.
///
/// DERIVED from the markers themselves at their widest (`u32::MAX`, ten digits) rather
/// than from a measured sample, so editing either spelling cannot silently outgrow the
/// reserve. Shipped first as a hand-measured `(…99999-token opening trimmed)` — five
/// digits — and the accompanying test immediately caught it: a marker is only a
/// guarantee if it fits for EVERY token count, not for the ones I happened to imagine.
/// A too-small reserve puts the disclosure back over the budget that forced the trim,
/// which is the bug this reserve exists to prevent.
pub fn trim_marker_tokens() -> u32 {
    estimate_prompt_tokens(mark_head_kept("", u32::MAX).trim())
        .max(estimate_prompt_tokens(mark_tail_kept("", u32::MAX).trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 3833b472): the two trim sites drifting apart again, and a
    // marker that is not paid for. The head marker must TRAIL its text and the tail
    // marker must LEAD it — a citizen reading a tail-keep has to meet the notice before
    // the mid-sentence text, or it reads as a whole message that starts oddly. The
    // reserve must also cover the widest marker either spelling can produce.
    #[test]
    fn both_trim_spellings_disclose_and_the_reserve_covers_them() {
        let head = mark_head_kept("the opening survives", 4321);
        assert!(head.starts_with("the opening survives"), "{head}");
        assert!(head.ends_with("(…4321-token message trimmed)"), "{head}");

        let tail = mark_tail_kept("the ending survives", 4321);
        assert!(tail.starts_with("(…4321-token opening trimmed)"), "{tail}");
        assert!(tail.ends_with("the ending survives"), "{tail}");

        // Neither spelling may cost more than the reserve set aside for it, at the
        // widest token count a real message could carry.
        let reserve = trim_marker_tokens();
        for marker in [
            mark_head_kept("", u32::MAX).trim().to_string(),
            mark_tail_kept("", u32::MAX).trim().to_string(),
        ] {
            assert!(
                estimate_prompt_tokens(&marker) <= reserve,
                "marker {marker:?} costs {} > reserve {reserve}",
                estimate_prompt_tokens(&marker)
            );
        }
    }

    // what this catches: the estimator drifting from the chars/4+1 unit the RAG
    // sources budget against (which would make the replay ledger's numbers lie),
    // and the empty-string edge costing a phantom token.
    #[test]
    fn estimate_matches_the_rag_source_unit_and_zeroes_empty() {
        assert_eq!(estimate_prompt_tokens(""), 0, "empty content costs nothing");
        // 8 chars / 4 + 1 = 3 — the exact formula the *_source.rs copies use.
        assert_eq!(estimate_prompt_tokens("abcdefgh"), 3);
        // multibyte counted by chars, not bytes: 4 chars / 4 + 1 = 2.
        assert_eq!(estimate_prompt_tokens("日本語訳"), 2);
        // monotonic: more content never costs fewer tokens.
        assert!(estimate_prompt_tokens("a".repeat(400).as_str()) > estimate_prompt_tokens("a"));
    }
}
