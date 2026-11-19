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

impl Reuse {
    /// The prompt tokens she does not prefill.
    pub(super) fn reused(&self) -> usize {
        match self {
            Reuse::Append { from } | Reuse::Rewind { from } => *from,
            Reuse::Restart(_) => 0,
        }
    }
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
pub(super) fn common_prefix(a: &[i32], b: &[i32]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Her checkpoint as reuse sees it: the tokens it holds through (exclusive end) and its basis.
pub(super) struct Kept<'a, B> {
    pub end: usize,
    pub basis: &'a B,
}

/// Where `prompt` (under `basis`) starts, given what she holds and her checkpoint.
pub(super) fn choose<B: PartialEq>(held: History<'_, B>, kept: Option<Kept<'_, B>>, next: History<'_, B>) -> Reuse {
    let (held_tokens, prompt, basis) = (held.tokens, next.tokens, next.basis);
    // The prompt extends her whole held history (her own generated tokens included): append.
    if let continuum_persona_engine::turn::Prefill::Append { from } = continuum_persona_engine::turn::plan(held, next) {
        return Reuse::Append { from };
    }
    let held = held_tokens;
    let Some(kept) = kept else {
        return Reuse::Restart(if held.is_empty() { Miss::Cold } else { Miss::NoCheckpoint });
    };
    if kept.basis != basis {
        return Reuse::Restart(Miss::Basis);
    }
    // Her checkpoint's end must be held unchanged by both, and leave a token to sample from.
    if kept.end == 0 || kept.end >= prompt.len() || common_prefix(held, prompt) < kept.end {
        return Reuse::Restart(Miss::Diverged);
    }
    Reuse::Rewind { from: kept.end }
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
        let kept = || Some(Kept { end: 80, basis: &genes });
        assert_eq!(choose(h(&held, &genes), kept(), h(&next, &genes)), Reuse::Rewind { from: 80 });

        let mut early: Vec<i32> = (0..40).collect();
        early.push(777); // her view itself changed (a new epoch): before her checkpoint
        early.extend(41..90);
        assert_eq!(choose(h(&held, &genes), kept(), h(&early, &genes)), Reuse::Restart(Miss::Diverged));

        assert_eq!(choose(h(&held, &genes), kept(), h(&next, &"tuned")), Reuse::Restart(Miss::Basis));
        assert_eq!(choose(h(&held, &genes), None, h(&next, &genes)), Reuse::Restart(Miss::NoCheckpoint));
        assert_eq!(choose(h(&[], &genes), None, h(&next, &genes)), Reuse::Restart(Miss::Cold));

        let mut extends = held.clone();
        extends.extend([5, 6]);
        assert_eq!(choose(h(&held, &genes), kept(), h(&extends, &genes)), Reuse::Append { from: 100 });
    }
}
