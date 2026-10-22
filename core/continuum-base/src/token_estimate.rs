//! The one prompt-token estimate. Lives in the floor crate so every layer (the `ai` adapters,
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
