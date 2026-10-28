//! Adaptive speculation: the MTP draft runs only while a turn decodes alone.
//!
//! Measured on the M5, 2026-10-28 02:21Z (engine-c 741b0175b, Qwen3.6-35B-A3B Q4_K_M, our
//! llama-server alone, 256-token decodes, t/s per stream / aggregate):
//!
//! | streams | draft (MTP n_max 4) | no draft |
//! |---|---|---|
//! | 1 | 80 / 80 | 57 / 57 |
//! | 2 | 42 / 84 | 44 / 89 |
//! | 4 | 23 / 91 | 31 / 122 |
//!
//! Stock llama.cpp batches as well as our server without a draft (llama-batched-bench: 59, 90,
//! 124 aggregate), so nothing serializes: the draft adds tokens every stream verifies, and on
//! this MoE model a bigger batch reads more experts, so drafting under concurrency costs more
//! than it saves. Set per request (`speculative.n_max`), never by relaunching the engine. The
//! crossover is this node's; a node with another model or backend measures its own.

/// The draft limit for a turn admitted while `others` other turns hold the engine: `None`
/// keeps the engine's own draft (this turn decodes alone), `Some(0)` drafts nothing.
pub(crate) fn draft_limit(others: usize) -> Option<u32> {
    (others > 0).then_some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: drafting while other minds decode (the M5: 91 against 122 t/s at four
    // streams), or losing the draft when a turn decodes alone (80 against 57 t/s).
    #[test]
    fn a_turn_drafts_alone_and_never_beside_another() {
        assert_eq!(draft_limit(0), None);
        assert_eq!(draft_limit(1), Some(0));
        assert_eq!(draft_limit(3), Some(0));
    }
}
