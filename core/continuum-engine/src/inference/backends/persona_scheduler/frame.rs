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
//! progress, the most urgent waiting turn goes next (object model 5.7, Need: each mind says how much
//! she needs her turn: Urgent when addressed, Want with work in hand, Content otherwise); within one
//! urgency, turns waiting at least half a second go oldest first. Below that threshold,
//! remaining prefill cost and age decide. Affinity breaks ties.

/// A turn as the pick sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    /// How much her mind needs this turn.
    pub urgency: continuum_sage::component::Urgency,
    /// Whether her mind is bound to the decoding stream now, and whether her turn has begun.
    pub place: Place,
    /// Prompt tokens still to prefill (0 once decoding).
    pub remaining: usize,
    /// Milliseconds since admission or her most recent completed frame.
    pub waited_ms: u64,
}

/// Where a turn stands against the decoding stream: the four combinations of bound and begun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Place {
    /// Not bound, not begun.
    Waiting,
    /// Bound to the stream, her turn not begun.
    Bound,
    /// Bound, her turn begun: prefilling or decoding.
    Running,
    /// Begun, then swapped out for a more urgent turn.
    Paused,
}

impl Place {
    pub(super) fn of(active: bool, started: bool) -> Self {
        match (active, started) {
            (false, false) => Self::Waiting,
            (true, false) => Self::Bound,
            (true, true) => Self::Running,
            (false, true) => Self::Paused,
        }
    }

    fn active(self) -> bool {
        matches!(self, Self::Bound | Self::Running)
    }
}

impl Candidate {
    fn before(&self, other: &Self) -> std::cmp::Ordering {
        // derived-or-floor: a half-second queue wait meets the interactive stall threshold.
        // At the next free boundary, age takes precedence over estimated prefill cost.
        const WAIT_BUDGET_MS: u64 = 500;
        if self.waited_ms.max(other.waited_ms) >= WAIT_BUDGET_MS {
            return other.waited_ms.cmp(&self.waited_ms)
                .then(self.remaining.cmp(&other.remaining))
                .then(other.place.active().cmp(&self.place.active()));
        }
        let discounted = |turn: &Self, by: &Self| turn.remaining as u128 * (1_000 + u128::from(by.waited_ms));
        discounted(self, other)
            .cmp(&discounted(other, self))
            .then(other.waited_ms.cmp(&self.waited_ms))
            .then(other.place.active().cmp(&self.place.active()))
    }
}

/// The index of the turn the Core advances next, or `None` for no turns.
///
/// Need decides at every frame, the running turn included (Joel, 2026-11-22: "Don't enforce arbitrary
/// rules"; "switching contexts is so damn fast anyway": 26 ms p50 on the M5, aa734cc01c). A running turn
/// keeps the Core while nothing needs it more, because at equal need a switch gains nothing; that is why
/// turns still run whole (the 11-21 point: slicing equal turns bought only switches). A more urgent turn
/// takes the next frame, and the running one resumes from her cached state when it is next most needed.
pub(super) fn pick(turns: &[Candidate]) -> Option<usize> {
    let most = turns.iter().map(|turn| turn.urgency).max()?;
    if let Some(running) = turns.iter().position(|turn| turn.place == Place::Running && turn.urgency == most) {
        return Some(running);
    }
    turns
        .iter()
        .enumerate()
        .filter(|(_, turn)| turn.urgency == most)
        .min_by(|(_, a), (_, b)| a.before(b))
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_sage::component::Urgency;

    fn turn(active: bool, started: bool, remaining: usize, waited_ms: u64) -> Candidate {
        Candidate { urgency: Urgency::Want, place: Place::of(active, started), remaining, waited_ms }
    }

    #[test]
    fn a_cold_turn_gets_the_next_boundary_after_its_wait_budget() {
        let warm = turn(true, false, 0, 0);
        assert_eq!(pick(&[warm, turn(false, false, 60_000, 499)]), Some(0));
        let overdue = turn(false, false, 60_000, 500);
        assert_eq!(pick(&[warm, overdue]), Some(1));
        assert_eq!(pick(&[turn(true, false, 1, 501), overdue]), Some(0));
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

    // what this catches (Codex on #6208 at 162fbe5fa7): an aged and a fresh one-token tail rounding to
    // the same cost, affinity winning every such tie, and the bound mind's repeated arrivals starving
    // an older mind.
    #[test]
    fn a_bound_mind_that_keeps_arriving_never_starves_an_older_one() {
        assert_eq!(pick(&[turn(true, false, 1, 5), turn(false, false, 1, 90_000)]), Some(1), "the aged one-token tail goes first");
        assert_eq!(pick(&[turn(true, false, 0, 0), turn(false, false, 0, 1)]), Some(1), "equal cost goes to the longer wait");

        // The bound mind answers and arrives again, fully cached, round after round; the older mind,
        // also fully cached, waits. It is served on the first round it is ready, not never.
        let mut older = turn(false, false, 0, 0);
        let served = (1..=5u64).find(|round| {
            older.waited_ms = round * 2_000;
            pick(&[turn(true, false, 0, 0), older]) == Some(1)
        });
        assert_eq!(served, Some(1), "the older mind is served on its first round");
    }

    // what this catches: an addressed mind waiting behind minds that only want to continue, including
    // one mid-turn (the M5, 2026-11-22: Aris decoded 19,540 tokens for 10.4 min with 13 turns queued
    // behind her). The most urgent turn takes the next frame, however long the others waited and
    // whether or not one is running; at equal need the running turn keeps the Core.
    #[test]
    fn the_most_urgent_turn_takes_the_next_frame_even_from_a_running_one() {
        let with = |urgency, active, started, waited_ms| Candidate { urgency, place: Place::of(active, started), remaining: 100, waited_ms };
        let addressed = with(Urgency::Urgent, false, false, 0);
        let continuing = with(Urgency::Want, false, false, 90_000);
        let musing = with(Urgency::Content, true, false, 120_000);
        assert_eq!(pick(&[continuing, musing, addressed]), Some(2), "urgency before age and affinity");
        assert_eq!(pick(&[musing, continuing]), Some(1), "Want before Content");
        let running = with(Urgency::Content, true, true, 0);
        assert_eq!(pick(&[addressed, running]), Some(0), "the addressed mind takes the next frame from a musing one");
        let working = with(Urgency::Want, true, true, 0);
        assert_eq!(pick(&[continuing, working]), Some(1), "at equal need the running turn keeps the Core");
        assert_eq!(pick(&[musing, working]), Some(1), "a less needed turn never takes it");
    }
}
