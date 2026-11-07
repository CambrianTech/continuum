//! What on a board no reviewer can judge yet, from the board's typed facts: a live card's newest
//! submission that no review names, and a code card with a pull request linked and no submission.
//! The second is a hole, not a wait: a reviewer's verdict names a submission, so a PR that never
//! became one can never be reviewed or reach her credit. On the M5, 2026-11-07, Benchy's #5791
//! (card b51f4b59) sat in Review with no submission and a reviewer's verdict was refused ("review
//! refers to no accepted submission on the parent card").
//!
//! Her surface turns the hole into her next move (`work_preconditions::next_move`, rendered on her
//! held-work item), and the health line and `work/list` count both ([`awaiting_reviewer`],
//! [`unsubmitted_pull_requests`]).

use airc_work::{BoardSnapshot, CardState, WorkCard, WorkSubmission};

use crate::work_preconditions::unsubmitted_pull_request;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_preconditions::{fixture_card, fixture_pull_request, fixture_submission};
    use airc_work::WorkCardId;

    fn her() -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    // what this catches: the board's review queue counting a card that is over, or missing a
    // linked PR that was never submitted (it has no submission, so it is not "awaiting").
    #[test]
    fn the_queue_counts_newest_unreviewed_submissions_and_unsubmitted_prs() {
        let mut waiting = fixture_card(CardState::Review, Some(her()), None, "a");
        waiting.submissions = vec![fixture_submission(&waiting)];
        let mut unsubmitted = fixture_card(CardState::Review, Some(her()), None, "b");
        unsubmitted.card_id = WorkCardId::from_u128(2);
        unsubmitted.pull_request = Some(fixture_pull_request());
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
