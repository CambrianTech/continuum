//! What on a board no reviewer can judge yet, from the board's typed facts: a live card's newest
//! submission that no review names, and a code card with a pull request linked and no submission.
//! The second is a hole, not a wait: a reviewer's verdict names a submission, so a PR that never
//! became one can never be reviewed or reach her credit. On the M5, 2026-11-07, Benchy's #5791
//! (card b51f4b59) sat in Review with no submission and a reviewer's verdict was refused ("review
//! refers to no accepted submission on the parent card").
//!
//! Her surface turns the hole into her next move ([`next_move`], rendered on her held-work item),
//! and the health line and `work/list` count both ([`awaiting_reviewer`],
//! [`unsubmitted_pull_requests`]).

use airc_core::PeerId;
use airc_work::{BoardSnapshot, CardState, PullRequestRef, WorkCard, WorkSubmission};

use crate::work_preconditions::{check, CardRole, WorkVerb};

/// A code card that is not over, with a pull request linked and no submission: her work, which no
/// reviewer can judge until it is submitted.
pub fn unsubmitted_pull_request(card: &WorkCard) -> Option<&PullRequestRef> {
    let live = !matches!(card.state, CardState::Merged | CardState::Closed);
    let code = CardRole::of(card) == CardRole::Code;
    card.pull_request.as_ref().filter(|_| live && code && card.submissions.is_empty())
}

/// The newest submission of each live card that no review on the board names.
pub fn awaiting_reviewer(board: &BoardSnapshot) -> impl Iterator<Item = &WorkSubmission> {
    board
        .cards
        .iter()
        .filter(|card| !matches!(card.state, CardState::Merged | CardState::Closed))
        .filter_map(|card| card.submissions.first()) // newest accepted first (airc-work `WorkCard::submissions`)
        .filter(|newest| !board.submission_reviews.iter().any(|review| review.submission_id == newest.submission_id))
}

/// Every card on the board whose linked pull request was never submitted.
pub fn unsubmitted_pull_requests(board: &BoardSnapshot) -> impl Iterator<Item = &WorkCard> {
    board.cards.iter().filter(|card| unsubmitted_pull_request(card).is_some())
}

/// The move her held card owes before anything else can happen to it, when there is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NextMove {
    /// Her pull request is linked to the card but was never submitted: submitting it is what lets
    /// a reviewer judge it and her credit settle.
    SubmitLinkedPullRequest { pull_request: PullRequestRef },
}

impl NextMove {
    /// The verb the move calls.
    pub const fn verb(&self) -> WorkVerb {
        match self {
            NextMove::SubmitLinkedPullRequest { .. } => WorkVerb::Submit,
        }
    }
}

/// `actor`'s next move on `card` at `now_ms`: offered only when its verb's precondition holds, the
/// same predicate the verb executes under (`work_preconditions::check`).
pub fn next_move(card: &WorkCard, actor: PeerId, now_ms: u64) -> Option<NextMove> {
    let pull_request = unsubmitted_pull_request(card)?;
    let next = NextMove::SubmitLinkedPullRequest { pull_request: pull_request.clone() };
    check(next.verb(), card, actor, now_ms).is_ok().then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_preconditions::{fixture_card, FIXTURE_NOW_MS};
    use airc_work::{BranchName, RepoId, WorkCardId};

    fn her() -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    fn pr() -> PullRequestRef {
        PullRequestRef {
            repo: RepoId::new("acme/continuum").expect("a fixed, valid repo id"), // expect: a literal RepoId accepts
            number: 5791,
            head: BranchName::new("benchy/fix").expect("a valid branch"), // expect: a literal branch name
            base: BranchName::new("canary").expect("a valid branch"),     // expect: as above
        }
    }

    fn submission(card: &WorkCard) -> WorkSubmission {
        WorkSubmission {
            submission_id: airc_work::SubmissionId::from_u128(9),
            card_id: card.card_id,
            claim_id: card.claim_id.expect("an owned fixture has a claim"), // expect: the fixture claims an owned card
            instance: "i".into(),
            base_sha: airc_work::GitObjectId::new("abc123").expect("hex"), // expect: a literal hex id
            artifact: airc_blobs::MediaRef { hash: airc_blobs::ContentHash::from_bytes(b"patch"), size_bytes: 5, mime: None },
            publisher: her(),
            submitted_at_ms: 1,
        }
    }

    // what this catches: Benchy's #5791 on the M5, 2026-11-07: her held card in Review with a PR
    // linked and no submission, so no reviewer could file a verdict and nothing told her. Her
    // surface names work/submit for that card, only while the card is hers to submit, and not once
    // a submission exists, nor on a review card or a card that is over.
    #[test]
    fn a_held_card_with_a_linked_pr_and_no_submission_yields_the_submit_move() {
        let mut card = fixture_card(CardState::Review, Some(her()), None, "fix the thing");
        card.pull_request = Some(pr());
        let next = next_move(&card, her(), FIXTURE_NOW_MS).expect("the unsubmitted PR is her next move"); // expect: asserted
        assert_eq!(next, NextMove::SubmitLinkedPullRequest { pull_request: pr() });
        assert_eq!(next.verb(), WorkVerb::Submit);
        let other = PeerId::from_uuid(uuid::Uuid::from_u128(2));
        assert_eq!(next_move(&card, other, FIXTURE_NOW_MS), None, "not hers to submit");
        let mut submitted = card.clone();
        submitted.submissions = vec![submission(&card)];
        assert_eq!(next_move(&submitted, her(), FIXTURE_NOW_MS), None, "submitted: a reviewer can judge it");
        let mut review = fixture_card(CardState::Claimed, Some(her()), Some(WorkCardId::from_u128(7)), "r");
        review.pull_request = Some(pr());
        assert_eq!(next_move(&review, her(), FIXTURE_NOW_MS), None, "a review card's word is a verdict");
        let mut merged = card.clone();
        merged.state = CardState::Merged;
        assert_eq!(next_move(&merged, her(), FIXTURE_NOW_MS), None, "its work is over");
    }

    // what this catches: the board's review queue counting a card that is over, or missing a
    // linked PR that was never submitted (it has no submission, so it is not "awaiting").
    #[test]
    fn the_queue_counts_newest_unreviewed_submissions_and_unsubmitted_prs() {
        let mut waiting = fixture_card(CardState::Review, Some(her()), None, "a");
        waiting.submissions = vec![submission(&waiting)];
        let mut unsubmitted = fixture_card(CardState::Review, Some(her()), None, "b");
        unsubmitted.card_id = WorkCardId::from_u128(2);
        unsubmitted.pull_request = Some(pr());
        let mut over = waiting.clone();
        over.card_id = WorkCardId::from_u128(3);
        over.state = CardState::Closed;
        let board = BoardSnapshot {
            cards: vec![waiting, unsubmitted, over],
            submission_reviews: Vec::new(),
            lanes: Vec::new(),
            workspaces: Vec::new(),
            repo_tracking: Vec::new(),
            pull_requests: Vec::new(),
            manager_hats: Vec::new(),
            agent_availability: Vec::new(),
            hygiene_reports: Vec::new(),
        };
        assert_eq!(awaiting_reviewer(&board).count(), 1);
        assert_eq!(unsubmitted_pull_requests(&board).map(|c| c.card_id).collect::<Vec<_>>(), vec![WorkCardId::from_u128(2)]);
    }
}
