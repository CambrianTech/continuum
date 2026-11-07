//! The learning half of the health line: what became of her reviewed work this hour, counted at
//! the one place each fact is decided, typed, never a string.
//!
//! Why its own counters: until 2026-11-07 the line read `settles` (benchmark rounds only, 0 by
//! construction) and `credits settled` (the benchmark verdict only), so the ordinary reviewed path
//! never counted. Aiko's credit really settled on the M5 (acceptance c88bf229, 2026-11-06
//! 10:33:14Z) and the line said 0. And `credits staged` counted one per work turn while a settle
//! accepts one revision per submission, ~1,300:1 even when healthy, so the ratio meant nothing.
//!
//! What is counted, and where (each at its one owner in continuum-mind's
//! `training_producer::reviewed`):
//! - [`CreditOutcome`]: a review's decision over her credit (`consume_review_credit`), and
//!   `Accepted` only at the `CreditReviewAcceptance` write (`settle_passed`), so a replay of an
//!   accepted credit never counts it twice;
//! - submissions with staged credit: a board submission bound to her staged revision
//!   (`bind_snapshot`), one per submission, the unit a settle accepts;
//! - review boundaries processed: a dream consolidation boundary finished for an accepted credit
//!   (`ReviewBoundaryStore::finish`).
//!
//! The review queue (submissions awaiting a reviewer, linked PRs never submitted) is a measure of
//! the boards at the tick, not a counter: [`ReviewQueue`].

use std::sync::atomic::{AtomicU64, Ordering};

use super::{Read, LEDGER, MINDS};

/// What a review decided about her credit, as the health line counts it. A projection of the
/// credit owner's state (`ReviewedCreditState`, which maps onto it exhaustively), so a new state
/// fails to compile until it is counted somewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreditOutcome {
    /// The training destination accepted her reviewed revision: learning that actually settled.
    Accepted,
    /// A reviewer judged the submission failed.
    FailedReview,
    /// A reviewer recorded no judgement either way.
    UnknownReview,
    /// A pass, decided, waiting on the training destination's acceptance.
    AwaitingAcceptance,
    /// Her own review, in a room that requires another reviewer.
    IndependentReviewRequired,
    /// The credit cannot be reached yet: no binding, or its provenance does not bind.
    Held,
    /// The bound evidence cannot be submitted for training.
    Ineligible,
    /// Nothing decided now (policy unreadable, another review selected, not resident).
    Undecided,
}

impl CreditOutcome {
    /// Every outcome, in the order the line reads them.
    pub const ALL: [CreditOutcome; 8] = [
        CreditOutcome::Accepted,
        CreditOutcome::FailedReview,
        CreditOutcome::UnknownReview,
        CreditOutcome::AwaitingAcceptance,
        CreditOutcome::IndependentReviewRequired,
        CreditOutcome::Held,
        CreditOutcome::Ineligible,
        CreditOutcome::Undecided,
    ];

    const fn index(self) -> usize {
        match self {
            CreditOutcome::Accepted => 0,
            CreditOutcome::FailedReview => 1,
            CreditOutcome::UnknownReview => 2,
            CreditOutcome::AwaitingAcceptance => 3,
            CreditOutcome::IndependentReviewRequired => 4,
            CreditOutcome::Held => 5,
            CreditOutcome::Ineligible => 6,
            CreditOutcome::Undecided => 7,
        }
    }

    /// The outcome's name on the line and in the probe.
    pub const fn as_str(self) -> &'static str {
        match self {
            CreditOutcome::Accepted => "accepted",
            CreditOutcome::FailedReview => "failed review",
            CreditOutcome::UnknownReview => "unknown review",
            CreditOutcome::AwaitingAcceptance => "awaiting acceptance",
            CreditOutcome::IndependentReviewRequired => "independent review required",
            CreditOutcome::Held => "held",
            CreditOutcome::Ineligible => "ineligible",
            CreditOutcome::Undecided => "undecided",
        }
    }
}

/// One count per [`CreditOutcome`], indexed by it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreditOutcomeCounts([u64; CreditOutcome::ALL.len()]);

impl CreditOutcomeCounts {
    pub fn of(&self, outcome: CreditOutcome) -> u64 {
        self.0[outcome.index()]
    }

    pub fn note(&mut self, outcome: CreditOutcome) {
        self.0[outcome.index()] += 1;
    }
}

/// The learning counters, fed only by the `note_*` functions below.
pub(crate) struct LearningCounters {
    outcomes: [AtomicU64; CreditOutcome::ALL.len()],
    submissions_staged: AtomicU64,
    boundaries_processed: AtomicU64,
}

impl Default for LearningCounters {
    fn default() -> Self {
        Self::new()
    }
}

impl LearningCounters {
    pub(crate) const fn new() -> Self {
        Self {
            outcomes: [const { AtomicU64::new(0) }; CreditOutcome::ALL.len()],
            submissions_staged: AtomicU64::new(0),
            boundaries_processed: AtomicU64::new(0),
        }
    }

    pub(crate) fn read(&self, read: Read) -> LearningHour {
        let mut outcomes = CreditOutcomeCounts::default();
        for outcome in CreditOutcome::ALL {
            outcomes.0[outcome.index()] = read.of(&self.outcomes[outcome.index()]);
        }
        LearningHour {
            outcomes,
            submissions_staged: read.of(&self.submissions_staged),
            boundaries_processed: read.of(&self.boundaries_processed),
        }
    }
}

/// The hour's learning, as read at the tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LearningHour {
    pub outcomes: CreditOutcomeCounts,
    /// Board submissions bound to staged credit: one per submission, the unit a settle accepts.
    pub submissions_staged: u64,
    /// Dream consolidation boundaries finished for accepted credit.
    pub boundaries_processed: u64,
}

/// The boards' review queue at the tick: what no reviewer can judge yet. Measured, never counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReviewQueue {
    /// Live cards whose newest submission no review names.
    pub awaiting_reviewer: u64,
    /// Code cards with a pull request linked and no submission: no reviewer can file a verdict.
    pub unsubmitted_pull_requests: u64,
}

/// A review decided `outcome` for `persona`'s credit. `Accepted` is noted only where the
/// acceptance is written, so it counts each settled credit once.
pub fn note_credit_outcome_of(persona: uuid::Uuid, outcome: CreditOutcome) {
    LEDGER.learning.outcomes[outcome.index()].fetch_add(1, Ordering::Relaxed);
    if outcome == CreditOutcome::Accepted {
        MINDS.entry(persona).or_default().credits_accepted += 1;
    }
}

/// Her credit accepted this hour: the per-mind read of [`note_credit_outcome_of`].
pub fn credits_accepted_of(persona: uuid::Uuid) -> u64 {
    MINDS.get(&persona).map_or(0, |m| m.credits_accepted) // map_or: no row this hour = nothing accepted, the truth
}

/// A board submission was bound to her staged credit (a fresh binding, never a replay).
pub fn note_submission_staged() {
    LEDGER.learning.submissions_staged.fetch_add(1, Ordering::Relaxed);
}

/// A review boundary's dream consolidation finished (a fresh receipt, never a replay).
pub fn note_boundary_processed() {
    LEDGER.learning.boundaries_processed.fetch_add(1, Ordering::Relaxed);
}

/// The line's learning terms: every credit outcome side by side, never summed, then the
/// submissions, the boundaries and, when the boards were read, the review queue.
pub fn learning_terms(hour: &LearningHour, queue: Option<&ReviewQueue>) -> String {
    let outcomes: Vec<String> =
        CreditOutcome::ALL.iter().map(|o| format!("{} {}", o.as_str(), hour.outcomes.of(*o))).collect();
    let queue = match queue {
        Some(q) => format!(
            " · awaiting a reviewer {} submissions / {} linked PRs never submitted",
            q.awaiting_reviewer, q.unsubmitted_pull_requests
        ),
        None => String::new(),
    };
    format!(
        "learning: {} submissions with staged credit · credit {} · review boundaries processed {}{queue}",
        hour.submissions_staged,
        outcomes.join(" / "),
        hour.boundaries_processed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an outcome counted under another or not at all, and the window not
    // resetting. Each outcome lands in its own counter; a peek leaves the hour standing; a close
    // returns it and starts the next at zero.
    #[test]
    fn every_outcome_is_counted_in_its_own_counter_and_the_window_resets() {
        let counters = LearningCounters::new();
        for outcome in CreditOutcome::ALL {
            counters.outcomes[outcome.index()].fetch_add(outcome.index() as u64 + 1, Ordering::Relaxed);
        }
        counters.submissions_staged.fetch_add(2, Ordering::Relaxed);
        let peek = counters.read(Read::Peek);
        for outcome in CreditOutcome::ALL {
            assert_eq!(peek.outcomes.of(outcome), outcome.index() as u64 + 1, "{outcome:?}");
        }
        assert_eq!(counters.read(Read::Close).submissions_staged, 2);
        assert_eq!(counters.read(Read::Peek), LearningHour::default());
    }

    // what this catches: the line folding outcomes into one number, naming the deleted benchmark
    // terms (`settles`, credits `settled`), or reading an unread board as an empty queue.
    #[test]
    fn the_line_names_each_outcome_and_the_queue_and_no_benchmark_term() {
        let mut outcomes = CreditOutcomeCounts::default();
        outcomes.note(CreditOutcome::Accepted);
        outcomes.note(CreditOutcome::FailedReview);
        outcomes.note(CreditOutcome::FailedReview);
        let hour = LearningHour { outcomes, submissions_staged: 3, boundaries_processed: 1 };
        let line = learning_terms(&hour, Some(&ReviewQueue { awaiting_reviewer: 4, unsubmitted_pull_requests: 1 }));
        for wanted in ["3 submissions with staged credit", "accepted 1", "failed review 2", "boundaries processed 1", "awaiting a reviewer 4", "1 linked PRs"] {
            assert!(line.contains(wanted), "{wanted}: {line}");
        }
        assert!(!line.contains("settle"), "{line}");
        assert!(!learning_terms(&hour, None).contains("awaiting a reviewer"), "an unread board is not an empty queue");
    }
}
