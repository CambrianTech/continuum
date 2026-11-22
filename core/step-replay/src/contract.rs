//! The causal contract over one mind's acts: each act's step reaches her next prompt.

use crate::capture::Act;

/// How one mind's acts kept (or broke) the contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    pub acts: u64,
    /// Calls her next prompt showed whole: the call and its outcome, bound by id.
    pub steps_seen: u64,
    /// Calls her next prompt did not show as hers.
    pub call_missing: u64,
    /// Yields her next prompt did not show: she ended her turn and her next one does not say she did,
    /// or how long ago. Not a dropped outcome; the time she cannot see passing (object model 5.7).
    pub yields_unseen: u64,
    /// Calls shown as hers whose outcome was missing.
    pub outcome_missing: u64,
    /// Acts where she wrote a call as text and made no native call.
    pub prose_calls: u64,
    /// "its whole is #n above" lines pointing at a result her prompt did not hold whole.
    pub dangling_above: u64,
    /// Runs of three or more consecutive acts asking the same calls, and the longest.
    pub repeat_runs: u64,
    pub longest_repeat: u64,
}

impl Tally {
    /// The contract broke: a step she could not see.
    pub fn broken(&self) -> u64 {
        self.call_missing + self.outcome_missing + self.dangling_above
    }

    pub fn add(&mut self, other: &Tally) {
        self.acts += other.acts;
        self.steps_seen += other.steps_seen;
        self.call_missing += other.call_missing;
        self.yields_unseen += other.yields_unseen;
        self.outcome_missing += other.outcome_missing;
        self.prose_calls += other.prose_calls;
        self.dangling_above += other.dangling_above;
        self.repeat_runs += other.repeat_runs;
        self.longest_repeat = self.longest_repeat.max(other.longest_repeat);
    }
}

/// Check one mind's acts, oldest first. Only consecutive captured acts are compared: a gap in the
/// captures (an act not recorded) is not counted against her view.
pub fn check(acts: &[Act]) -> Tally {
    let mut tally = Tally { acts: acts.len() as u64, ..Tally::default() };
    for act in acts {
        tally.prose_calls += u64::from(act.prose_call);
        tally.dangling_above += act.shown.says_above.iter().filter(|n| !act.shown.whole_results.contains(n)).count() as u64;
    }
    for pair in acts.windows(2) {
        let (before, next) = (&pair[0], &pair[1]);
        for call in &before.calls {
            match (next.shown.calls.contains(&call.id), next.shown.outcomes.contains(&call.id)) {
                (true, true) => tally.steps_seen += 1,
                (false, _) if call.asked.starts_with("yield_turn") => tally.yields_unseen += 1,
                (false, _) => tally.call_missing += 1,
                (true, false) => tally.outcome_missing += 1,
            }
        }
    }
    let mut run = 1u64;
    for pair in acts.windows(2) {
        let same = !pair[0].calls.is_empty()
            && pair[0].calls.iter().map(|c| &c.asked).eq(pair[1].calls.iter().map(|c| &c.asked));
        if same {
            run += 1;
            continue;
        }
        close_run(&mut tally, run);
        run = 1;
    }
    close_run(&mut tally, run);
    tally
}

fn close_run(tally: &mut Tally, run: u64) {
    if run >= 3 {
        tally.repeat_runs += 1;
        tally.longest_repeat = tally.longest_repeat.max(run);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::capture::{Call, Shown};

    fn act(at_ms: u64, calls: &[(&str, &str)], shown_calls: &[&str], outcomes: &[&str]) -> Act {
        Act {
            request_id: format!("r{at_ms}"),
            at_ms,
            shown: Shown {
                calls: shown_calls.iter().map(|s| s.to_string()).collect(),
                outcomes: outcomes.iter().map(|s| s.to_string()).collect(),
                whole_results: BTreeSet::new(),
                says_above: Vec::new(),
            },
            calls: calls.iter().map(|(id, asked)| Call { id: id.to_string(), asked: asked.to_string() }).collect(),
            prose_call: false,
            completed: true,
        }
    }

    // what this catches: the MacBookPro loop shape (2026-11-22): a call whose step never reaches
    // her next prompt, an outcome dropped while the call stays, a repeat line pointing at a result
    // that is gone, and the same call asked over and over. A step shown whole counts as kept.
    #[test]
    fn a_step_her_next_prompt_does_not_show_breaks_the_contract() {
        let mut acts = vec![
            act(1, &[("c1", "read a")], &[], &[]),
            act(2, &[("c2", "read a")], &["c1"], &["c1"]),
            act(3, &[("c3", "read a")], &["c2"], &[]),
            act(4, &[("c4", "read a")], &[], &[]),
            act(5, &[("y5", "yield_turn{}")], &[], &[]),
            act(6, &[], &[], &[]),
        ];
        acts[4].shown.says_above = vec![9];
        let t = check(&acts);
        assert_eq!((t.steps_seen, t.outcome_missing, t.call_missing, t.yields_unseen), (1, 1, 2, 1));
        assert_eq!(t.dangling_above, 1);
        assert_eq!((t.repeat_runs, t.longest_repeat), (1, 4));
        assert_eq!(t.broken(), 4);
    }
}
