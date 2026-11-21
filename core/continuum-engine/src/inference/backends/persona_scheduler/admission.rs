//! The Core's context capacity: the one owner of "how big can her turn be" (object model 4.11).
//!
//! Two questions, both answered here and nowhere else:
//! - **Can her turn start at all?** Only if her prompt leaves at least one position for her reply.
//!   A prompt the whole context cannot hold is `Infeasible`, with the numbers, before any prefill is
//!   spent on it (section 3.8: a need exceeding the whole context; her caller refits or is placed on a
//!   larger Core). The M5, 2026-11-21: a 26,633-token replay prompt was admitted to a 24,576-token
//!   stream, prefilled for ~31 s, and refused at the boundary chunk.
//! - **Does the context hold her next position?** Her reply grows into the context token by token;
//!   when it reaches the end, the Core ends her turn there as `CapacityReached`, keeping everything she
//!   wrote. `max_tokens` is never reserved up front: reserving it refused turns that would have
//!   finished well inside the context (4.11 invariant 4).

use llama::Pos;

/// The most positions the Core's one context holds, whole to the mind in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Capacity(usize);

/// A prompt the whole context cannot hold, with the numbers: her caller refits to `capacity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("her prompt is {prompt_tokens} tokens and the context holds {capacity}: refit within it")]
pub(super) struct Infeasible {
    pub prompt_tokens: usize,
    pub capacity: usize,
}

impl Capacity {
    pub(crate) const fn new(tokens: usize) -> Self {
        Self(tokens)
    }

    /// Her turn may start if her prompt leaves at least one position for her reply.
    pub(super) const fn admit(self, prompt_tokens: usize) -> Result<(), Infeasible> {
        if prompt_tokens < self.0 {
            Ok(())
        } else {
            Err(Infeasible { prompt_tokens, capacity: self.0 })
        }
    }

    /// Whether the context holds a token at `pos`.
    pub(crate) fn holds(self, pos: Pos) -> bool {
        usize::from(pos) < self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a prompt the context cannot hold admitted and failing deep in prefill (the
    // 2026-11-21 replay); a turn refused for a reply it might never write (`max_tokens` reserved up
    // front, c0f87b4503); and her reply decoded past the context's last position instead of ending
    // there.
    #[test]
    fn a_turn_starts_when_her_prompt_fits_and_her_reply_ends_at_the_context() {
        let context = Capacity::new(122_368);
        assert_eq!(context.admit(122_367), Ok(()), "one position left for her reply is enough to start");
        assert_eq!(context.admit(122_368), Err(Infeasible { prompt_tokens: 122_368, capacity: 122_368 }), "no room to reply at all");
        let at = |count: usize| Pos::try_from(count).map(|pos| context.holds(pos));
        assert_eq!(at(122_367), Ok(true), "the last position is hers");
        assert_eq!(at(122_368), Ok(false), "past it, her turn ends as capacity reached");
    }
}
