//! Whether a turn fits the Core at all, decided at the door (object model §3.8: the Core answers
//! `Infeasible` when a need exceeds its cap, with the numbers).
//!
//! The M5, 2026-11-21: a 26,633-token replay prompt was admitted to a 24,576-token stream, prefilled
//! for ~31 s, and refused by the engine at the boundary chunk ("no KV slot"). Then seven live turns
//! (07:31-07:57Z) were admitted with prompts of 23,791 to 24,277 tokens to a 24,320-token stream,
//! leaving 43 to 529 tokens for answers asked to run longer: each decoded until the stream ended
//! (`from=24320 count=1`) and failed mid-answer. A turn's need is her prompt and the reply she asked
//! for; one that cannot fit is refused before any work is spent on it, and never cut mid-answer.

/// The most tokens one mind's stream holds: the applied geometry's `n_ctx_seq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StreamCapacity(usize);

/// A turn that cannot fit this Core, with the numbers: her caller refits to `capacity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("her prompt is {prompt_tokens} tokens and she asked for up to {reply_tokens} more; one stream holds {capacity}: refit within it")]
pub(super) struct Infeasible {
    pub prompt_tokens: usize,
    pub reply_tokens: usize,
    pub capacity: usize,
}

impl StreamCapacity {
    pub(super) const fn new(tokens: usize) -> Self {
        Self(tokens)
    }

    /// Admit a turn only if her prompt and the reply she asked for both fit one stream. The last
    /// token she is given is never decoded back in, so `prompt + reply` positions is the most a turn
    /// can occupy.
    pub(super) const fn admit(self, prompt_tokens: usize, reply_tokens: usize) -> Result<(), Infeasible> {
        if prompt_tokens.saturating_add(reply_tokens) <= self.0 {
            Ok(())
        } else {
            Err(Infeasible { prompt_tokens, reply_tokens, capacity: self.0 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a prompt that cannot fit the stream admitted and failing deep in prefill
    // (the 2026-11-21 replay); a prompt that fits but leaves no room for the reply she asked for,
    // admitted and failing mid-answer at the stream's end (seven live turns, 2026-11-21); and a turn
    // that fits exactly refused at the edge.
    #[test]
    fn a_turn_is_admitted_only_when_her_prompt_and_reply_both_fit() {
        let stream = StreamCapacity::new(24_320);
        assert_eq!(stream.admit(23_296, 1_024), Ok(()), "prompt and reply fill the stream exactly");
        assert_eq!(stream.admit(24_202, 1_024), Err(Infeasible { prompt_tokens: 24_202, reply_tokens: 1_024, capacity: 24_320 }), "the prompt fits, her reply does not");
        assert_eq!(stream.admit(26_633, 256), Err(Infeasible { prompt_tokens: 26_633, reply_tokens: 256, capacity: 24_320 }), "the prompt alone overflows");
    }
}
