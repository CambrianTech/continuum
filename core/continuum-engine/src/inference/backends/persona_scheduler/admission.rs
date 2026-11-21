//! Whether a turn fits the Core at all, decided at the door (object model §3.8: the Core answers
//! `Infeasible` when a need exceeds its cap, with the numbers).
//!
//! The M5, 2026-11-21: a 26,633-token replay prompt was admitted to a 24,576-token stream, prefilled
//! for ~31 s, and refused by the engine at the boundary chunk ("no KV slot"). A turn whose prompt
//! leaves no room for even one generated token is refused before any work is spent on it.

/// The most tokens one mind's stream holds: the applied geometry's `n_ctx_seq`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StreamCapacity(usize);

/// A turn that cannot fit this Core, with the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("her prompt is {prompt_tokens} tokens and one stream holds {capacity}: no room to answer on this engine")]
pub(super) struct Infeasible {
    pub prompt_tokens: usize,
    pub capacity: usize,
}

impl StreamCapacity {
    pub(super) const fn new(tokens: usize) -> Self {
        Self(tokens)
    }

    /// Admit a prompt only if it leaves room for at least one generated token.
    pub(super) const fn admit(self, prompt_tokens: usize) -> Result<(), Infeasible> {
        if prompt_tokens < self.0 {
            Ok(())
        } else {
            Err(Infeasible { prompt_tokens, capacity: self.0 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a prompt that cannot fit the stream admitted and failing deep in prefill
    // (the 2026-11-21 replay), and a prompt that fits refused at the edge.
    #[test]
    fn a_prompt_is_admitted_only_with_room_for_one_token() {
        let stream = StreamCapacity::new(24_576);
        assert_eq!(stream.admit(24_575), Ok(()), "one token of room is enough to start");
        assert_eq!(stream.admit(24_576), Err(Infeasible { prompt_tokens: 24_576, capacity: 24_576 }));
        assert_eq!(stream.admit(26_633), Err(Infeasible { prompt_tokens: 26_633, capacity: 24_576 }));
    }
}
