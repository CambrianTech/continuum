//! The one prompt-token estimate. Lives in the engine ring so `ai` adapters and the
//! mind's RAG/budget layers price content in the SAME unit; `cognition::token_budget`
//! re-exports it as its documented home.

/// Estimate the prompt-token cost of a piece of content. Char-based (handles
/// multibyte text correctly), `/4` for the standard ~4-chars-per-token heuristic,
/// `+1` so non-empty content is never costed at zero.
pub fn estimate_prompt_tokens(content: &str) -> u32 {
    if content.is_empty() {
        return 0;
    }
    ((content.chars().count() / 4) as u32).saturating_add(1)
}
