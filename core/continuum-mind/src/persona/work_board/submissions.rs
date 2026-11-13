//! Reading a card's submissions: what its holder does next. The latest candidate is
//! airc's `WorkCard::latest_submission`, the one owner of "newest".

use airc_lib::WorkCardId;
use uuid::Uuid;

use crate::cognition::card_facts::short8;

/// PURE: what happens next for the holder of a card whose latest submission is `latest`, given
/// the outcomes its reviews have reached and whether it is a bench card. The ONE place this is
/// said, read by `work/submit`'s receipt, `work/get` and `work/submission`. `None` before anything
/// is submitted.
///
/// What this exists for (card aff659aa; the 5090 2026-10-11): Kimi spent ~5 h of work turns on an
/// already-submitted card asking which state transition triggers grading. The answer differs by
/// card, and this says the right one:
/// - a card graded at submit (`recipe::benchmark_catalog::graded_at_submit`: a known SWE bench card) is graded
///   by its test oracle the moment it is submitted (`work/submit` spawns `grade_submission`, which
///   grades by the same predicate), and marking it done (`work/state <id> done`) also
///   runs the grade (grade-on-done). The verdict posts to the room. The bench card's own text says
///   the same, so this must never contradict it (#4954: the first wording said "no state you set
///   triggers it", which is false here);
/// - any other card is ordinary work, never graded: it is done when a peer's review passes it.
/// A `Failed` review sends either kind back. An oracle FAIL is not yet a WorkReview (card
/// 2c756670), so for a bench card the room's grade line is named as the verdict to read.
///
/// A passed code card is NOT done on the pass: it lands when its pull request merges
/// (`pull_request`, the card's link). With none linked she is told the move that opens and links
/// it, never that the work is done (the finishing chain's break 2, 2026-11-08).
pub fn next_after_submission(latest: Option<Uuid>, outcomes: &[airc_work::WorkReviewOutcome], graded_at_submit: bool, review: Option<WorkCardId>, pull_request: Option<&airc_work::PullRequestRef>) -> Option<String> {
    let submission = short8(latest?);
    let pr = pull_request.map(|pr| format!("{}#{}", pr.repo, pr.number));
    Some(if outcomes.contains(&airc_work::WorkReviewOutcome::Passed) {
        match (graded_at_submit, &pr) {
            (true, _) => format!("Submission {submission} passed review: the card is done. Nothing more to do on it."),
            (false, Some(pr)) => format!(
                "Submission {submission} passed review. Pull request {pr} merges through the room's merger, and the card \
                 finishes as Merged; nothing more to do unless a refused merge comes back to you."
            ),
            (false, None) => format!(
                "Submission {submission} passed review, but no pull request is linked, so nothing can merge yet. \
                 work/submit again (the same patch): it opens the pull request for your branch and links it to the card."
            ),
        }
    } else if outcomes.contains(&airc_work::WorkReviewOutcome::Failed) {
        format!("A reviewer sent submission {submission} back (see reviews). Revise the work and work/submit again.")
    } else if graded_at_submit {
        format!(
            "Submitted as {submission}. The test oracle grades it now and posts the verdict in the room \
             (marking the card done runs the same grade). A failed grade: revise and work/submit again. \
             A passed grade: nothing more to do."
        )
    } else if let Some(review) = review {
        let landing = match &pr {
            Some(pr) => format!("Pull request {pr} carries it; a pass lets the merger land it"),
            None => "No pull request is linked yet; work/submit again (the same patch) opens and links it".to_string(),
        };
        format!(
            "Submitted as {submission}. Review card {review} is available to its reviewer. {landing}. Await the review: \
             a send-back appears in its reviews. Do not submit again unless it is sent back."
        )
    } else {
        format!("Submission {submission} is retained, but no review handoff is recorded. Retry work/submit with this same submission_id to establish review; do not publish a new candidate just to retry handoff.")
    })
}

pub fn submission_review_handoff(board: &airc_work::WorkBoardProjection, parent: WorkCardId, submission: Option<airc_work::SubmissionId>) -> Option<WorkCardId> {
    let id = airc_work::submission_review_card_id(parent, submission?);
    board.review_request(id).map(|_| id)
}

