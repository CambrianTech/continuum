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
/// mid-word. Sibling of `deliberation_budget::tail_to_tokens` — head-keep is
/// the right shape for a chat MESSAGE (the opening carries the point; the
/// tail is elaboration), where tail-keep is right for a transcript window
/// (the latest lines carry the present). Used by the breadth-over-depth
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

#[cfg(test)]
mod tests {
    use super::*;

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
