//! What a turn's prefill starts from (CONTINUUM-ENGINE 4.15). Her prompt is append-only up to her
//! view's end; past it (the clock, the ask, her newest act) it changes every act. The engine reuses
//! what she holds as far as the new prompt still matches it, and the hybrid's recurrent state can
//! only go back to a checkpoint, so the furthest legal start is her checkpoint at her last view's
//! end, when the new prompt still holds her tokens through it under the same genes. Before this a
//! changed trailer re-prefilled her whole prompt every act (the M5, 2026-11-18, b762170061: reused
//! 0 of 11,834-30,588 tokens on every act).

pub(super) use continuum_persona_engine::turn::History;

/// Where a turn's prefill starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reuse {
    /// The prompt extends what she holds: prefill from `from` (`turn::plan`).
    Append { from: usize },
    /// Rewind her to her checkpoint and prefill from `from`, its end.
    Rewind { from: usize },
    /// Prefill her whole prompt, for the reason named.
    Restart(Miss),
}

/// Why nothing she holds could be reused: each is a loud probe reason, never a silent re-prefill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Miss {
    /// She holds nothing and has no checkpoint (a first turn, or a restart dropped her).
    Cold,
    /// The prompt diverges from her before her checkpoint's end (her view changed: a new epoch).
    Diverged,
    /// Her checkpoint was taken under other genes.
    Basis,
    /// No checkpoint is kept for her.
    NoCheckpoint,
}

impl Miss {
    pub(super) fn reason(self) -> &'static str {
        match self {
            Miss::Cold => "cold",
            Miss::Diverged => "diverged_before_checkpoint",
            Miss::Basis => "genes_changed",
            Miss::NoCheckpoint => "no_checkpoint",
        }
    }
}

/// The tokens two sequences share from their start.
pub(super) use continuum_persona_engine::turn::common_prefix;

/// One of her checkpoints as reuse sees it: the tokens it holds through (exclusive end) and its basis.
#[derive(Clone, Copy)]
pub(super) struct Kept<'a, B> {
    pub end: usize,
    pub basis: &'a B,
}

/// Where `prompt` (under `basis`) starts, given what she holds and her checkpoints, in any order:
/// the furthest one both histories hold unchanged under the same genes. Her head's checkpoint
/// serves a turnover, which changes her view but not her head (the M5, 2026-11-19, 48a1dc80cd: 17
/// of 30 turns re-prefilled from 0 behind an unchanged head, ~180k of 422,847 tokens).
pub(super) struct Decision {
    pub shared: usize,
    pub action: Reuse,
}

pub(super) fn choose<'a, B: PartialEq + 'a>(held: History<'_, B>, kept: impl IntoIterator<Item = Kept<'a, B>>, next: History<'_, B>) -> Decision {
    let comparison = held.compare(&next);
    let shared = comparison.shared;
    let decide = |action| Decision { shared, action };
    let (held, prompt, basis) = (held.tokens, next.tokens, next.basis);
    if let continuum_persona_engine::turn::Prefill::Append { from } = comparison.prefill {
        return decide(Reuse::Append { from });
    }
    let mut kept = kept.into_iter().peekable();
    if kept.peek().is_none() {
        return decide(Reuse::Restart(if held.is_empty() { Miss::Cold } else { Miss::NoCheckpoint }));
    }
    let mut same_genes = kept.filter(|kept| kept.basis == basis).peekable();
    if same_genes.peek().is_none() {
        return decide(Reuse::Restart(Miss::Basis));
    }
    // A checkpoint's end must be held unchanged by both, and leave a token to sample from.
    decide(same_genes
        .map(|kept| kept.end)
        .filter(|end| *end > 0 && *end < prompt.len() && *end <= shared)
        .max()
        .map_or(Reuse::Restart(Miss::Diverged), |from| Reuse::Rewind { from }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h<'a, B>(tokens: &'a [i32], basis: &'a B) -> History<'a, B> {
        History { basis, tokens }
    }

    // what this catches (the M5, 2026-11-18, b762170061): every act re-prefilling her whole prompt
    // because the clock trailer or her own generated tokens no longer matched. A new prompt that
    // keeps her view and changes only what follows it rewinds to her checkpoint; one that diverges
    // before it, or runs under other genes, restarts with its reason; a prompt that extends what she
    // holds still appends (no rewind needed).
    #[test]
    fn a_changed_trailer_rewinds_to_her_checkpoint_and_nothing_else_does() {
        let genes = "base";
        let held: Vec<i32> = (0..100).collect(); // her view (0..80), the old trailer, her generated tokens
        let mut next: Vec<i32> = (0..80).collect();
        next.extend([900, 901, 902]); // a new trailer and her call, rendered differently
        let kept = [Kept { end: 80, basis: &genes }];
        assert_eq!(choose(h(&held, &genes), kept, h(&next, &genes)).action, Reuse::Rewind { from: 80 });

        let mut early: Vec<i32> = (0..40).collect();
        early.push(777); // her view itself changed (a new epoch): before her checkpoint
        early.extend(41..90);
        assert_eq!(choose(h(&held, &genes), kept, h(&early, &genes)).action, Reuse::Restart(Miss::Diverged));

        assert_eq!(choose(h(&held, &genes), kept, h(&next, &"tuned")).action, Reuse::Restart(Miss::Basis));
        assert_eq!(choose(h(&held, &genes), [], h(&next, &genes)).action, Reuse::Restart(Miss::NoCheckpoint));
        assert_eq!(choose(h(&[], &genes), [], h(&next, &genes)).action, Reuse::Restart(Miss::Cold));

        let mut extends = held.clone();
        extends.extend([5, 6]);
        assert_eq!(choose(h(&held, &genes), kept, h(&extends, &genes)).action, Reuse::Append { from: 100 });
    }

    // what this catches (the M5, 2026-11-19, 48a1dc80cd: 17 of 30 turns re-prefilled from 0 behind
    // an unchanged head): a turnover changes her view after her head, so the prompt diverges
    // before her view's checkpoint but after her head's, and it rewinds to her head instead of
    // starting over. The furthest checkpoint both hold wins, whatever order they are kept in.
    #[test]
    fn a_turnover_rewinds_to_her_head_and_the_furthest_held_checkpoint_wins() {
        let genes = "base";
        let held: Vec<i32> = (0..100).collect(); // her head (0..20), her view (20..80), the trailer
        let kept = [Kept { end: 80, basis: &genes }, Kept { end: 20, basis: &genes }];
        let mut turned: Vec<i32> = (0..20).collect();
        turned.extend(500..560); // a new epoch: her view re-rendered behind the same head
        assert_eq!(choose(h(&held, &genes), kept, h(&turned, &genes)).action, Reuse::Rewind { from: 20 });

        let mut trailer: Vec<i32> = (0..80).collect();
        trailer.push(900);
        assert_eq!(choose(h(&held, &genes), kept, h(&trailer, &genes)).action, Reuse::Rewind { from: 80 });

        let mut head_changed: Vec<i32> = vec![777];
        head_changed.extend(1..90);
        assert_eq!(choose(h(&held, &genes), kept, h(&head_changed, &genes)).action, Reuse::Restart(Miss::Diverged));

        let mixed = [Kept { end: 80, basis: &"tuned" }, Kept { end: 20, basis: &genes }];
        assert_eq!(choose(h(&held, &genes), mixed, h(&trailer, &genes)).action, Reuse::Rewind { from: 20 }, "only her checkpoints under these genes count");
    }
}
