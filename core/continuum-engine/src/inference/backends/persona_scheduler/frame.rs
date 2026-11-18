//! Which turn the next frame advances (CONTINUUM-ENGINE 4.13/4.15). A frame is one step: one prefill
//! chunk or one decoded token, never a whole turn, so a mind with a long cold prompt never holds the
//! engine while a mind with a short tail waits behind her (the M5, 2026-11-18, build 88074d9300:
//! turns served whole, back to back, waited 300-360 s behind one another's prefills).
//!
//! The rule, in order:
//! 1. the active mind's turn that is decoding continues: one mind decodes until she stops, and a
//!    switch mid-answer would only add a switch;
//! 2. otherwise the turn with the least prefill left goes first, its remaining tokens discounted by
//!    how long it has waited, so a short tail never waits behind a long prompt and a long prompt
//!    still ages to the front;
//! 3. ties go to the active mind (no switch).
//!
//! The policy is this one pure function, so the owner of goal 9's scheduler replaces it here and
//! nowhere else.

/// A turn as the frame sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// Her mind is the one bound to the decoding stream now.
    pub active: bool,
    /// Her prompt is prefilled and she is producing tokens.
    pub decoding: bool,
    /// Prompt tokens still to prefill (0 once decoding).
    pub remaining: usize,
    /// Milliseconds since her turn was submitted.
    pub waited_ms: u64,
}

impl Candidate {
    /// Remaining work discounted by age: tokens per second waited, plus one so a fresh turn is its
    /// own size. Ballpark by design; only the order matters.
    fn cost(&self) -> u128 {
        (self.remaining as u128 * 1_000) / (1_000 + u128::from(self.waited_ms))
    }
}

/// The index of the turn the next frame advances, or `None` for no turns.
pub(super) fn pick(turns: &[Candidate]) -> Option<usize> {
    if let Some(decoding) = turns.iter().position(|turn| turn.active && turn.decoding) {
        return Some(decoding);
    }
    turns
        .iter()
        .enumerate()
        .min_by_key(|(_, turn)| (turn.cost(), !turn.active))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(active: bool, decoding: bool, remaining: usize, waited_ms: u64) -> Candidate {
        Candidate { active, decoding, remaining, waited_ms }
    }

    // what this catches: a mind's cold prompt holding the engine while a short tail waits (the M5,
    // 2026-11-18: whole turns back to back, 300-360 s waits). The short tail goes first; the active
    // mind's answer in progress is never cut for a switch; a long prompt still reaches the front by
    // waiting; equal costs keep the mind already bound.
    #[test]
    fn a_short_tail_never_waits_behind_a_long_prompt_and_an_answer_is_never_cut() {
        assert_eq!(pick(&[]), None);
        let long_active = turn(true, false, 60_000, 0);
        let short = turn(false, false, 300, 0);
        assert_eq!(pick(&[long_active, short]), Some(1), "the short tail switches in at the chunk boundary");

        let answering = turn(true, true, 0, 0);
        assert_eq!(pick(&[turn(false, false, 1, 90_000), answering]), Some(1), "her answer finishes first");

        let aged = turn(false, false, 60_000, 600_000);
        assert_eq!(pick(&[turn(false, false, 1_000, 0), aged]), Some(1), "ten minutes of waiting brings a long prompt forward");

        assert_eq!(pick(&[turn(false, false, 500, 0), turn(true, false, 500, 0)]), Some(1), "a tie keeps the bound mind");
    }
}
