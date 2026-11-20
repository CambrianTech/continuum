//! Which turn the next frame advances (CONTINUUM-ENGINE 4.13/4.15). A frame is one step: one prefill
//! chunk or one decoded token, never a whole turn, so a mind with a long cold prompt never holds the
//! engine while a mind with a short tail waits behind her (the M5, 2026-11-18, build 88074d9300:
//! turns served whole, back to back, waited 300-360 s behind one another's prefills).
//!
//! Eligible turns share a one-second service budget. The bound mind advances until another
//! turn is due, then the oldest unserved turn gets the stream at the next landed frame.
const SERVICE_BUDGET_MS: u64 = 1000;

pub(super) fn share_ms(ready: usize) -> u64 {
    (SERVICE_BUDGET_MS / ready.max(1) as u64).max(1)
}

/// A turn as the frame sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// Her mind is the one bound to the decoding stream now.
    pub active: bool,
    /// Her prompt is prefilled and she is producing tokens.
    pub decoding: bool,
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

/// The index of the turn the next frame advances, or `None` for no turns.
pub(super) fn pick(turns: &[Candidate], bound_ms: u64) -> Option<usize> {
    let share_ms = share_ms(turns.len());
    if let Some((index, other)) = turns.iter().enumerate().filter(|(_, t)| !t.active).max_by_key(|(_, t)| t.waited_ms) {
        if other.waited_ms >= SERVICE_BUDGET_MS || (other.waited_ms >= share_ms && bound_ms >= share_ms) {
            return Some(index);
        }
    }
    if let Some(decoding) = turns.iter().position(|turn| turn.active && (turn.decoding || (bound_ms < share_ms && turns.iter().any(|other| other.decoding)))) {
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
    fn a_short_tail_never_waits_behind_a_long_prompt_and_an_overdue_turn_can_preempt_decode() {
        assert_eq!(pick(&[], 0), None);
        let long_active = turn(true, false, 60_000, 0);
        let short = turn(false, false, 300, 0);
        assert_eq!(pick(&[long_active, short], 0), Some(1), "the short tail switches in at the chunk boundary");

        let answering = turn(true, true, 0, 0);
        assert_eq!(pick(&[turn(false, false, 1, 90_000), answering], 500), Some(0), "an overdue turn interrupts an answer");

        let aged = turn(false, false, 60_000, 600_000);
        assert_eq!(pick(&[turn(false, false, 1_000, 0), aged], 0), Some(1), "ten minutes of waiting brings a long prompt forward");

        assert_eq!(pick(&[turn(false, false, 500, 0), turn(true, false, 500, 0)], 0), Some(1), "a tie keeps the bound mind");
    }
}
