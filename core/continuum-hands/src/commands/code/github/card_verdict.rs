//! The board's recorded verdict on a pull request: when a card links the PR, a merge needs a
//! PASSED review of the card's latest submission by someone other than its author, and that
//! submission must end at the PR's head. "Passed" comes only from recorded verdicts (card 5a94e88e),
//! never from a card's state; the rule is the one airc's merger applies (card 950b3093,
//! `airc-cli` `merger::review_gate`), so a resident's merge and the merger's agree.
//!
//! The boards come through a seam ([`CardBoards`]): hands cannot read a caller's boards (her airc
//! identity lives in the mind), so the layer above binds the reader at boot. The judgement is pure,
//! over facts reduced from the boards ([`linked_cards`] then [`verdict_at_head`]).

use std::sync::Arc;

use airc_work::{BoardSnapshot, GitObjectId, WorkReviewOutcome};
use uuid::Uuid;

use crate::sdk_codegen::{CommandError, Ctx};

/// Why the caller's boards could not be read. A merge whose PR may carry a card is never judged
/// without them.
#[derive(Debug, thiserror::Error)]
pub enum BoardsUnread {
    #[error("this node has bound no board reader for the merge gate")]
    NotBound,
    #[error("this node has no live roster of identities to read boards as")]
    NoRoster,
    #[error("the caller acts as no airc identity on this node")]
    NoIdentity(#[source] CommandError),
    #[error("the boards could not be read")]
    Board(#[source] airc_lib::AircError),
}

/// The boards the caller can read: every room she is subscribed to. Bound at boot by the layer that
/// holds her airc identity.
#[async_trait::async_trait]
pub trait CardBoards: Send + Sync {
    async fn boards(&self, ctx: &Ctx) -> Result<Vec<Arc<BoardSnapshot>>, BoardsUnread>;
}

/// A review's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictOutcome {
    Passed,
    Failed,
    Unknown,
}

/// One recorded review of a submission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub reviewer: Uuid,
    pub outcome: VerdictOutcome,
    /// The reviewer's signed clock: the latest review is the one that stands.
    pub at_ms: u64,
}

/// A card's latest submission and its reviews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submitted {
    /// The commit its patch ends at; `None` for a submission made before heads were recorded.
    pub head: Option<GitObjectId>,
    pub author: Uuid,
    pub reviews: Vec<Verdict>,
}

/// A card that links the pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedCard {
    pub card: Uuid,
    /// Its newest submission; `None` when it has none.
    pub latest: Option<Submitted>,
}

/// Why a linked card's verdict does not cover the PR's head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewGap {
    NoSubmission,
    HeadUnrecorded,
    HeadMoved { reviewed: GitObjectId, pr: GitObjectId },
    NoIndependentReview,
    LatestReview(VerdictOutcome),
}

impl std::fmt::Display for ReviewGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSubmission => f.write_str("the card has no submission for a reviewer to judge"),
            Self::HeadUnrecorded => f.write_str("its latest submission predates recorded heads, so no verdict can be shown to cover the PR's head"),
            Self::HeadMoved { reviewed, pr } => write!(f, "the review covers {reviewed}, and the PR's head moved on to {pr}: the new head needs its own review"),
            Self::NoIndependentReview => f.write_str("no one but its author has reviewed the latest submission"),
            Self::LatestReview(outcome) => write!(f, "the latest independent review is {outcome:?}, not Passed"),
        }
    }
}

/// What the boards say about merging the PR at its head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkedReview {
    /// No card links it: CI and mergeability alone decide.
    NotLinked,
    /// Every linked card's latest submission ends at the head and passed an independent review.
    Passed { card: Uuid, reviewer: Uuid },
    /// A linked card's verdict does not cover the head.
    Gap { card: Uuid, gap: ReviewGap },
}

fn outcome(outcome: WorkReviewOutcome) -> VerdictOutcome {
    match outcome {
        WorkReviewOutcome::Passed => VerdictOutcome::Passed,
        WorkReviewOutcome::Failed => VerdictOutcome::Failed,
        WorkReviewOutcome::Unknown => VerdictOutcome::Unknown,
    }
}

/// The cards on `boards` that link pull request `number` of `repo` (`owner/name`, compared as
/// GitHub does, without case), each with its newest submission and that submission's reviews.
pub fn linked_cards(boards: &[Arc<BoardSnapshot>], repo: &str, number: u64) -> Vec<LinkedCard> {
    let mut linked: Vec<LinkedCard> = Vec::new();
    for board in boards {
        for card in &board.cards {
            let links = card.pull_request.as_ref().is_some_and(|pr| pr.number == number && pr.repo.as_str().eq_ignore_ascii_case(repo));
            if !links || linked.iter().any(|seen| seen.card == card.card_id.as_uuid()) {
                continue;
            }
            let latest = card.submissions.first().map(|newest| Submitted {
                head: newest.head.commit().cloned(),
                author: newest.publisher.as_uuid(),
                reviews: board
                    .submission_reviews
                    .iter()
                    .filter(|review| review.submission_id == newest.submission_id)
                    .map(|review| Verdict { reviewer: review.reviewer.as_uuid(), outcome: outcome(review.outcome), at_ms: review.reviewed_at_ms })
                    .collect(),
            });
            linked.push(LinkedCard { card: card.card_id.as_uuid(), latest });
        }
    }
    linked
}

/// One card's verdict at `head`: its latest independent review passed, at exactly `head`.
fn card_verdict(latest: Option<&Submitted>, head: &GitObjectId) -> Result<Uuid, ReviewGap> {
    let submitted = latest.ok_or(ReviewGap::NoSubmission)?;
    let reviewed = submitted.head.as_ref().ok_or(ReviewGap::HeadUnrecorded)?;
    let standing = submitted
        .reviews
        .iter()
        .filter(|review| review.reviewer != submitted.author)
        .max_by_key(|review| review.at_ms)
        .ok_or(ReviewGap::NoIndependentReview)?;
    match standing.outcome {
        VerdictOutcome::Passed if reviewed == head => Ok(standing.reviewer),
        VerdictOutcome::Passed => Err(ReviewGap::HeadMoved { reviewed: reviewed.clone(), pr: head.clone() }),
        other => Err(ReviewGap::LatestReview(other)),
    }
}

/// What the linked cards say about merging at `head`: the first card whose verdict does not cover
/// it, else the first card's passing review, else that no card links the PR.
pub fn verdict_at_head(cards: &[LinkedCard], head: &GitObjectId) -> LinkedReview {
    let mut passed = None;
    for linked in cards {
        match card_verdict(linked.latest.as_ref(), head) {
            Ok(reviewer) if passed.is_none() => passed = Some(LinkedReview::Passed { card: linked.card, reviewer }),
            Ok(_) => {}
            Err(gap) => return LinkedReview::Gap { card: linked.card, gap },
        }
    }
    passed.unwrap_or(LinkedReview::NotLinked) // unwrap_or: no card passed because none links the PR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(fill: char) -> GitObjectId {
        GitObjectId::new(std::iter::repeat(fill).take(40).collect::<String>()).expect("a commit id")
    }

    fn peer(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    const AUTHOR: u128 = 1;
    const REVIEWER: u128 = 2;

    fn card(head: Option<GitObjectId>, reviews: Vec<Verdict>) -> LinkedCard {
        LinkedCard { card: peer(100), latest: Some(Submitted { head, author: peer(AUTHOR), reviews }) }
    }

    fn review(by: u128, outcome: VerdictOutcome, at_ms: u64) -> Verdict {
        Verdict { reviewer: peer(by), outcome, at_ms }
    }

    // what this catches (card 5a94e88e): a merge allowed on anything but a recorded, independent
    // pass at the PR's exact head. Each gap the merger refuses on is refused here, the newest
    // independent review is the one that stands, and an unlinked PR is judged by CI alone.
    #[test]
    fn only_a_recorded_independent_pass_at_the_head_lets_a_linked_pr_merge() {
        let head = sha('c');
        assert_eq!(verdict_at_head(&[], &head), LinkedReview::NotLinked);
        let passed = card(Some(head.clone()), vec![review(REVIEWER, VerdictOutcome::Passed, 5)]);
        assert_eq!(verdict_at_head(&[passed], &head), LinkedReview::Passed { card: peer(100), reviewer: peer(REVIEWER) });

        let gap = |linked: LinkedCard| match verdict_at_head(&[linked], &head) {
            LinkedReview::Gap { gap, .. } => gap,
            other => panic!("expected a gap, got {other:?}"),
        };
        assert_eq!(gap(LinkedCard { card: peer(100), latest: None }), ReviewGap::NoSubmission);
        assert_eq!(gap(card(None, vec![review(REVIEWER, VerdictOutcome::Passed, 5)])), ReviewGap::HeadUnrecorded);
        assert_eq!(gap(card(Some(head.clone()), vec![review(AUTHOR, VerdictOutcome::Passed, 5)])), ReviewGap::NoIndependentReview, "her own pass is not a review");
        assert_eq!(
            gap(card(Some(sha('a')), vec![review(REVIEWER, VerdictOutcome::Passed, 5)])),
            ReviewGap::HeadMoved { reviewed: sha('a'), pr: head.clone() },
            "a pass covers the head it read, not a later push"
        );
        assert_eq!(
            gap(card(Some(head.clone()), vec![review(REVIEWER, VerdictOutcome::Passed, 5), review(3, VerdictOutcome::Failed, 9)])),
            ReviewGap::LatestReview(VerdictOutcome::Failed),
            "a newer failing review stands over an older pass"
        );
    }

    // what this catches: one passing card admitting a PR that another linked card has not passed.
    #[test]
    fn every_linked_card_must_pass() {
        let head = sha('c');
        let passed = card(Some(head.clone()), vec![review(REVIEWER, VerdictOutcome::Passed, 5)]);
        let unreviewed = LinkedCard { card: peer(200), latest: Some(Submitted { head: Some(head.clone()), author: peer(AUTHOR), reviews: Vec::new() }) };
        assert_eq!(verdict_at_head(&[passed, unreviewed], &head), LinkedReview::Gap { card: peer(200), gap: ReviewGap::NoIndependentReview });
    }
}
