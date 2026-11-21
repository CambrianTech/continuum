//! Which turn the Core advances next (CONTINUUM-ENGINE 4.13/4.15): one mind at a time, and a turn
//! in progress runs to its end. Joel, 2026-11-21: "you don't cut it up, run single lane; context
//! switching is faster."
//!
//! The policy this replaces gave every ready turn a slice of a one-second service budget, so a turn
//! was cut into frames shared with every other ready mind. On the M5 (build 56913f640b, 2026-11-21
//! 04:14-04:29 UTC) that meant 20 to 520 switches per turn at 12 to 25 ms each, up to 8.5 s of
//! switching inside one turn, for no gain to the turn being cut. Its reason was cold prompts (the
//! M5, 2026-11-18: whole turns back to back, short tails waiting 300-360 s behind cold prefills);
//! the same 2026-11-21 read showed most turns reusing 71 to 98% of their prompt. Whether a cold
//! prompt still needs its own handling is measured, not assumed.
//!
//! So: the bound mind's turn, once started, keeps the Core until it ends. When no turn is in
//! progress, the next is the one with the least remaining work discounted by how long it has
//! waited, so a long wait brings any prompt forward and no mind starves; a tie keeps the mind
//! already bound, which saves a switch.

/// A turn as the pick sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// Her mind is the one bound to the decoding stream now.
    pub active: bool,
    /// Her turn has begun: prefilling or decoding.
    pub started: bool,
    /// Prompt tokens still to prefill (0 once decoding).
    pub remaining: usize,
    /// Milliseconds since admission or her most recent completed frame.
    pub waited_ms: u64,
}

impl Candidate {
    /// Remaining work discounted by age: tokens per second waited, plus one so a fresh turn is its
    /// own size. Ballpark by design; only the order matters.
    fn cost(&self) -> u128 {
        (self.remaining as u128 * 1_000) / (1_000 + u128::from(self.waited_ms))
    }
}

/// The index of the turn the Core advances next, or `None` for no turns.
pub(super) fn pick(turns: &[Candidate]) -> Option<usize> {
    if let Some(running) = turns.iter().position(|turn| turn.active && turn.started) {
        return Some(running);
    }
    turns.iter().enumerate().min_by_key(|(_, turn)| (turn.cost(), !turn.active)).map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(active: bool, started: bool, remaining: usize, waited_ms: u64) -> Candidate {
        Candidate { active, started, remaining, waited_ms }
    }

    // what this catches: a turn in progress cut up to serve other minds (the 2026-11-21 read: 20 to
    // 520 switches per turn), a turn waiting forever behind fresh short ones, and a needless switch
    // on a tie.
    #[test]
    fn a_turn_in_progress_runs_to_its_end_and_a_long_wait_still_comes_forward() {
        assert_eq!(pick(&[]), None);
        let prefilling = turn(true, true, 60_000, 0);
        assert_eq!(pick(&[turn(false, false, 300, 90_000), prefilling]), Some(1), "an overdue turn waits for the running one to end");
        let answering = turn(true, true, 0, 0);
        assert_eq!(pick(&[turn(false, false, 1, 90_000), answering]), Some(1), "an answer in progress is never cut");
        let aged = turn(false, false, 60_000, 600_000);
        assert_eq!(pick(&[turn(false, false, 1_000, 0), aged]), Some(1), "ten minutes of waiting brings a long prompt forward");
        assert_eq!(pick(&[turn(false, false, 500, 0), turn(true, false, 500, 0)]), Some(1), "a tie keeps the bound mind");
    }
}
