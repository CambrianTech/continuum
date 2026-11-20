//! A turn's prefill, from what her blocks already hold (MEMORY: never process twice). Her token
//! history is the engine's: a turn whose prompt extends it under the same basis prefills only the new
//! tail; anything else (another model or adapter set, another epoch, other media behind the same
//! token ids, a rewritten conversation) starts a newer epoch, so stale blocks are never decoded
//! against.

#[cfg(test)]
mod tests;

/// A token history and its basis: what her blocks were computed under beside the token ids (the
/// model, the adapter set and its scales, her epoch, the identity of any native media spliced into
/// the prompt). Two equal token sequences under different bases are different histories (Codex on the
/// serving slice).
#[derive(Debug, Clone, Copy)]
pub struct History<'a, B> {
    pub basis: &'a B,
    pub tokens: &'a [i32],
}

/// What a turn must decode before she can sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefill {
    /// Her blocks hold the prompt's first `from` tokens under the turn's basis; decode the rest from
    /// there.
    Append { from: usize },
    /// Her blocks do not hold a prefix of this prompt under this basis: start a newer epoch and
    /// decode all of it.
    Restart,
}

/// Plan a turn's prefill from `held` (what her blocks hold) and `turn` (the whole prompt this turn
/// renders, and its basis).
///
/// Sampling needs the logits of the prompt's last token, which a held position does not keep. So
/// when the prompt adds nothing past what she holds, the last token is re-decoded
/// (`Append { from: len - 1 }`, after the tier drops that one position). An empty prompt cannot be
/// sampled from and restarts, as does a mind with nothing held.
pub fn plan<B: PartialEq>(held: History<'_, B>, turn: History<'_, B>) -> Prefill {
    held.compare(&turn).prefill
}

/// One comparison of the native token vectors, used for planning and accounting.
pub struct Compared {
    pub shared: usize,
    pub prefill: Prefill,
}

pub fn common_prefix(a: &[i32], b: &[i32]) -> usize {
    a.iter().zip(b).take_while(|(left, right)| left == right).count()
}

impl<B: PartialEq> History<'_, B> {
    pub fn compare(&self, next: &History<'_, B>) -> Compared {
        let shared = common_prefix(self.tokens, next.tokens);
        let prefill = if shared > 0 && shared == self.tokens.len() && self.basis == next.basis {
            Prefill::Append { from: shared.min(next.tokens.len() - 1) }
        } else {
            Prefill::Restart
        };
        Compared { shared, prefill }
    }
}
