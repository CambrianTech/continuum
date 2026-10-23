//! Reading a card's submissions: the latest candidate, and what its holder does next.

use airc_lib::WorkCardId;
use uuid::Uuid;

use super::facts::short8;

/// Latest accepted candidate in the board's transcript order. AIRC projects submissions
/// newest first; publisher wall clocks do not define recency. Keep handoff and review
/// consumers on this contract so a resumed citizen sees the candidate being reviewed.
pub fn latest_submission(card: &airc_lib::WorkCard) -> Option<&airc_work::WorkSubmission> {
    card.submissions.first()
}

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
pub fn next_after_submission(latest: Option<Uuid>, outcomes: &[airc_work::WorkReviewOutcome], graded_at_submit: bool, review: Option<WorkCardId>) -> Option<String> {
    let submission = short8(latest?);
    Some(if outcomes.contains(&airc_work::WorkReviewOutcome::Passed) {
        format!("Submission {submission} passed review: the card is done. Nothing more to do on it.")
    } else if outcomes.contains(&airc_work::WorkReviewOutcome::Failed) {
        format!("A reviewer sent submission {submission} back (see reviews). Revise the work and work/submit again.")
    } else if graded_at_submit {
        format!(
            "Submitted as {submission}. The test oracle grades it now and posts the verdict in the room \
             (marking the card done runs the same grade). A failed grade: revise and work/submit again. \
             A passed grade: nothing more to do."
        )
    } else if let Some(review) = review {
        format!(
            "Submitted as {submission}. Review card {review} is available to its reviewer. Await the review: a pass \
             completes it; a send-back appears in its reviews. Do not submit again unless it is sent back."
        )
    } else {
        format!("Submission {submission} is retained, but no review handoff is recorded. Retry work/submit with this same submission_id to establish review; do not publish a new candidate just to retry handoff.")
    })
}

pub fn submission_review_handoff(board: &airc_work::WorkBoardProjection, parent: WorkCardId, submission: Option<airc_work::SubmissionId>) -> Option<WorkCardId> {
    let id = airc_work::submission_review_card_id(parent, submission?);
    board.review_request(id).map(|_| id)
}

