//! Her held work this tick, typed: the cards she holds with what each one's preconditions put in
//! her hands (`work_preconditions`),
//! the claims that ended since her last good read, and her live rejected-claim facts. Read once
//! per tick by the cycle (`extensions::held_work`) and carried on the [`Workspace`](super::Workspace),
//! so her tool surface and her held-work item read the same snapshot
//! (docs/architecture/STATE-DRIVEN-SURFACE.md). Nothing here parses rendered text.

use airc_work::{CardState, WorkCardId};

use crate::cognition::work_preconditions::Held;

/// What her board says she holds now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeldWork {
    /// Every card she holds, in the board's order.
    pub holds: Vec<Held>,
    /// The claim her work turns are bound to when she holds more than one
    /// (`work_focus::focus_workspace_card`); `None` with one card or none.
    pub selected: Option<WorkCardId>,
    /// Claims that were hers at the last successful read and are not now: a lease that lapsed
    /// or a card released. Delivered once, the read it is seen; a degraded read never adds one.
    pub ended: Vec<EndedClaim>,
    /// Her rejected claim attempts still worth knowing, worded by the ring that records them
    /// (`persona::claim_rejections`).
    pub rejections: Vec<String>,
    /// Cards she sent back whose author has not been answered (card 7f8c248e), read from the
    /// board's typed reviews, submissions and review cards. Informational: never a tool
    /// restriction, never an assignment.
    pub awaiting: Vec<AwaitingResubmission>,
    /// Her latest submission on each of her own cards: the files it changes against its base and
    /// the newest verdict on it, side by side (card 1e9774cc). A review that names a file then
    /// points at a path she can see, and she reads the reviewer's words, not her paraphrase.
    pub submissions: Vec<SubmissionChanges>,
    /// Teammates' submissions awaiting a reviewer on every board she stands in, the longest waiting
    /// first (`board_nav`'s review requests). Her board view reads only the turn's room; on the M5,
    /// 2026-11-14, 26 of the 28 readable requests stood on a board none of her turns were in.
    /// Informational: never a tool restriction, never an assignment.
    pub review_requests: Vec<ReviewRequest>,
}

/// A review card a teammate's submission opened, hers to take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewRequest {
    pub review_card: WorkCardId,
    pub title: String,
    /// When the review card was opened: how long the submission has waited.
    pub opened_at_ms: u64,
}

/// What one submission changes against its recorded base, and the newest verdict on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionChanges {
    pub card: WorkCardId,
    pub title: String,
    pub submission: airc_work::SubmissionId,
    pub base: String,
    pub head: String,
    /// Read from her card's checkout; `Err` says why it could not be, so she is told what to do
    /// rather than shown nothing.
    pub files: Result<Vec<ChangedFile>, ChangesUnread>,
    pub verdict: Option<SubmissionVerdict>,
}

/// One file a submission changes. `added`/`deleted` are `None` for a binary file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub change: FileChange,
    pub added: Option<u64>,
    pub deleted: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileChange {
    Added,
    Deleted,
    Modified,
    Renamed { from: String },
    /// A copy: its source still exists.
    Copied { from: String },
}

/// Why a submission's changed files could not be read, typed so her view says what she can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangesUnread {
    /// Her card has no checkout on this node.
    NoCheckout,
    /// The submission's commits are not in her checkout (fetch them).
    CommitsAbsent { base: String, head: String },
    /// Git refused the diff, in its own words.
    GitRefused { detail: String },
    /// Git answered in a shape the reader does not understand.
    Unparsable { detail: String },
}

/// The newest verdict on a submission, with the reviewer's own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionVerdict {
    pub reviewer: airc_core::PeerId,
    pub outcome: airc_work::WorkReviewOutcome,
    pub evidence: Option<String>,
}

/// A card whose newest submission she failed. Until its author resubmits there is nothing for her
/// to do on it; when they do, the board opens a review card for that exact submission, offered to
/// her first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwaitingResubmission {
    pub card: WorkCardId,
    pub title: String,
    /// The submission her send-back judged.
    pub reviewed: airc_work::SubmissionId,
    /// The review card of the author's newer submission, open and nobody else's.
    pub resubmitted: Option<WorkCardId>,
}

/// A claim that ended: which card, and what it was called, so she can tell which one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndedClaim {
    pub card: WorkCardId,
    pub title: String,
}

impl HeldWork {
    /// The command names (and subtrees) her held cards put in her hands (each card's verbs whose
    /// precondition holds, with what they grant), each once, in canonical order: what her tool
    /// surface adds to the room's declared set. Stable while her cards' facts are.
    pub fn move_commands(&self) -> Vec<String> {
        let commands: std::collections::BTreeSet<&'static str> = self.holds.iter().flat_map(|held| held.in_hand.iter().copied()).collect();
        commands.into_iter().map(str::to_owned).collect()
    }

    /// She holds a card she is working on now (InProgress): the working-presence contract's key.
    pub fn holds_in_progress(&self) -> bool {
        self.holds.iter().any(|held| held.card.state == CardState::InProgress)
    }

    /// She holds a card in either working state (Claimed or InProgress): the lane gate's key.
    pub fn holds_a_working_card(&self) -> bool {
        self.holds.iter().any(|held| matches!(held.card.state, CardState::Claimed | CardState::InProgress))
    }
}
