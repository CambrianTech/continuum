//! The room's review gate (card fa4aaaaa, step 2): whether a card may finish is the
//! ROOM's policy, never a benchmark round's flag.
//!
//! Joel, 2026-10-05: review is the room's one verdict action, whoever gives it; like
//! GitHub a room can require several, by role. airc's own model says the same from the
//! other side: a submission review is an immutable judgement and "consumers apply their
//! activity's independence/acceptance policy". This module is that consumer. The policy
//! is the room binding's `params.review` ([`ReviewPolicy`]); the evidence is the board's
//! reviews of the card's latest submission.
//!
//! The count is pure and pinned by a table. The read is one board read plus one wall read.

use airc_lib::{Airc, WorkCardId};
use uuid::Uuid;

use crate::experience::binding::ReviewPolicy;

/// One review of a submission, reduced to what the policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReviewFact {
    pub reviewer: Uuid,
    pub passed: bool,
}

/// How many passing reviews count toward `policy.required`: passes only, one per
/// distinct reviewer, and the author's own pass only where the room allows self-review.
/// Roles are declared but not yet enforced (membership does not carry roles yet).
pub(crate) fn passing_reviewers(reviews: &[ReviewFact], author: Option<Uuid>, policy: &ReviewPolicy) -> usize {
    let mut counted: Vec<Uuid> = Vec::new();
    for r in reviews {
        if !r.passed || counted.contains(&r.reviewer) {
            continue;
        }
        if Some(r.reviewer) == author && !policy.self_review {
            continue;
        }
        counted.push(r.reviewer);
    }
    counted.len()
}

/// The room's review state for a card: its declared policy and the passes that count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoomReview {
    pub policy: ReviewPolicy,
    pub passed: usize,
}

impl RoomReview {
    /// Must the card still wait for review before it finishes?
    pub fn outstanding(&self) -> bool {
        (self.passed as u32) < self.policy.required
    }
}

/// What reading a room's review policy found. `Unavailable` is not `Absent`: a binding
/// this build cannot read, or a wall it cannot reach, says nothing about what the room
/// declares, so no caller may treat it as "no policy" (Codex root, review of #5017).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyRead {
    /// The room declares no review policy.
    Absent,
    /// The room's policy could not be read now; the reason is probed.
    Unavailable(String),
    Declared(ReviewPolicy),
}

/// The review policy `room` declares. The one place a policy is read.
pub async fn room_policy(airc: &Airc, room: &airc_lib::Room) -> PolicyRead {
    let posts = match airc
        .wall_posts_in(room, Some(crate::experience::binding::RECIPE_WALL_CATEGORY))
        .await
    {
        Ok(posts) => posts,
        Err(error) => return unavailable(room, error.to_string()),
    };
    match crate::experience::binding::project_binding(&posts) {
        Ok(Some(binding)) => match binding.declared_review() {
            Ok(Some(policy)) => PolicyRead::Declared(policy),
            Ok(None) => PolicyRead::Absent,
            Err(why) => unavailable(room, why.to_string()),
        },
        Ok(None) => PolicyRead::Absent,
        Err(why) => unavailable(room, why.to_string()),
    }
}

fn unavailable(room: &airc_lib::Room, why: String) -> PolicyRead {
    continuum_base::probe!(
        class = "work.review.policy_unreadable",
        room = %room.channel.as_uuid(),
        why = %why,
        "the room's review policy could not be read: no caller treats that as no policy"
    );
    PolicyRead::Unavailable(why)
}

/// The review state of `card` in the room whose board holds it, or `None` when that room
/// declares no review policy (the caller keeps its round fallback until every room's
/// recipe declares one). An unreadable binding is `None` here too, never a guessed gate:
/// this is the finish gate, unchanged by #5017 (credit no longer reads it as admission).
pub(crate) async fn room_review(airc: &std::sync::Arc<Airc>, card_id: WorkCardId) -> Option<RoomReview> {
    let (room, card) = super::horizon::card_in_subscribed_rooms(airc, card_id).await?;
    let PolicyRead::Declared(policy) = room_policy(airc, &room).await else {
        return None;
    };
    let board = airc.work_board_in(&room).await.ok()?;
    let latest = super::submissions::latest_submission(&card);
    let author = latest.map(|s| s.publisher.as_uuid()).or(card.owner.map(|o| o.as_uuid()));
    let reviews: Vec<ReviewFact> = match latest {
        Some(sub) => board
            .submission_reviews_for(sub.submission_id)
            .filter(|r| r.card_id == card.card_id)
            .map(|r| ReviewFact {
                reviewer: r.reviewer.as_uuid(),
                passed: r.outcome == airc_work::WorkReviewOutcome::Passed,
            })
            .collect(),
        None => Vec::new(),
    };
    let passed = passing_reviewers(&reviews, author, &policy);
    Some(RoomReview { policy, passed })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTHOR: Uuid = Uuid::from_u128(1);
    const PEER_A: Uuid = Uuid::from_u128(2);
    const PEER_B: Uuid = Uuid::from_u128(3);

    fn pass(r: Uuid) -> ReviewFact {
        ReviewFact { reviewer: r, passed: true }
    }

    // what this catches: the acceptance policy applied to a submission's reviews (card
    // fa4aaaaa). Passes only; one per distinct reviewer; the author's own pass only where
    // the room allows self-review; `required` decides whether the card still waits.
    #[test]
    fn passes_count_once_per_reviewer_and_self_only_where_allowed() {
        let one = ReviewPolicy::default();
        let fail = ReviewFact { reviewer: PEER_A, passed: false };
        assert_eq!(passing_reviewers(&[fail], Some(AUTHOR), &one), 0, "a failure is not a pass");
        assert_eq!(passing_reviewers(&[pass(PEER_A), pass(PEER_A)], Some(AUTHOR), &one), 1, "one reviewer, one pass");
        assert_eq!(passing_reviewers(&[pass(AUTHOR)], Some(AUTHOR), &one), 0, "no self-review by default");
        let selfish = ReviewPolicy { self_review: true, ..ReviewPolicy::default() };
        assert_eq!(passing_reviewers(&[pass(AUTHOR)], Some(AUTHOR), &selfish), 1, "Kimi on her own work, where allowed");

        let two = ReviewPolicy { required: 2, ..ReviewPolicy::default() };
        let after_one = RoomReview { policy: two.clone(), passed: passing_reviewers(&[pass(PEER_A)], Some(AUTHOR), &two) };
        assert!(after_one.outstanding(), "required: 2 waits for a second reviewer");
        let after_two = RoomReview { policy: two.clone(), passed: passing_reviewers(&[pass(PEER_A), pass(PEER_B)], Some(AUTHOR), &two) };
        assert!(!after_two.outstanding());
    }
}
