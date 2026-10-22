//! The prompt-token estimates: the typical one ([`estimate_prompt_tokens`]) and the
//! conservative window guard ([`estimate_guard_tokens`]). They live in the floor crate so every layer (the `ai` adapters,
//! the mind's RAG and budget layers) prices content in the SAME unit without importing a layer
//! above it; `rag` reaching `ai` for it put `rag` in the core's dependency cycle.

/// Estimate the prompt-token cost of a piece of content. Char-based (handles
/// multibyte text correctly), `/4` for the standard ~4-chars-per-token heuristic,
/// `+1` so non-empty content is never costed at zero.
pub fn estimate_prompt_tokens(content: &str) -> u32 {
    if content.is_empty() {
        return 0;
    }
    ((content.chars().count() / 4) as u32).saturating_add(1)
}

/// Chars-per-token divisor for the conservative window estimate. English is ~4
/// chars/token, but the deliberation prompt carries UUID-dense rosters, structured
/// engram observations, and code, which tokenize far denser — so we OVER-count
/// tokens (divide by 3, not 4) to stay safely under `n_ctx`. The completion reserve
/// absorbs the remaining slack.
// context-budget-exempt: a chars-per-token UNIT CONVERSION, not a budget — it is the basis ContextBudget itself computes against
pub const GUARD_CHARS_PER_TOKEN: usize = 3;

/// Conservative token estimate for the window guard (the deliberation fit and the
/// engine's served-window guard size a prompt the same way) (see [`GUARD_CHARS_PER_TOKEN`]).
pub fn estimate_guard_tokens(s: &str) -> usize {
    s.len() / GUARD_CHARS_PER_TOKEN
}
