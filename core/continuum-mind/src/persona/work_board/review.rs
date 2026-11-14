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
//! The state is `review_verdict::derive` (pure, pinned by its table in continuum-rooms). The read
//! is one board read plus one wall read.

use airc_lib::{Airc, WorkCardId};
use uuid::Uuid;

use crate::cognition::review_verdict::{derive, MindOf, PeerIsMind, ReviewState};
use crate::experience::binding::ReviewPolicy;

/// The mind behind a board peer: the resident whose airc identity it is, else the peer itself.
/// The one identity owner the self-review rule reads (`review_verdict::MindOf`), so a mind is never
/// compared as two raw ids from two namespaces.
impl MindOf for crate::persona::PersonaAircRuntimeRegistry {
    fn mind_of(&self, peer: airc_core::PeerId) -> Uuid {
        self.iter()
            .find(|runtime| runtime.airc().peer_id() == peer)
            .map(|runtime| runtime.persona_id())
            .unwrap_or_else(|| peer.as_uuid()) // unwrap_or_else: a peer no resident is, is its own mind
    }
}

/// The room's review state for a card: its declared policy and the state its latest submission's
/// recorded verdicts derive (`review_verdict::derive`, the one derivation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoomReview {
    pub policy: ReviewPolicy,
    pub state: ReviewState,
}

impl RoomReview {
    /// Must the card still wait for review before it finishes?
    pub fn outstanding(&self) -> bool {
        !self.state.passed()
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

impl PolicyRead {
    /// The policy a derivation counts under: the declared one, else the default (one independent
    /// pass, no self-review). The default is the floor, never looser than a declaration.
    pub fn counted(&self) -> ReviewPolicy {
        match self {
            PolicyRead::Declared(policy) => policy.clone(),
            PolicyRead::Absent | PolicyRead::Unavailable(_) => ReviewPolicy::default(),
        }
    }

    /// The declared policy, or `None` when the room declares none or it could not be read.
    pub fn declared(&self) -> Option<&ReviewPolicy> {
        match self {
            PolicyRead::Declared(policy) => Some(policy),
            PolicyRead::Absent | PolicyRead::Unavailable(_) => None,
        }
    }
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
    let state = match card.latest_submission() {
        Some(latest) => derive(latest, board.submission_reviews_for(latest.submission_id), &policy, &minds()),
        None => ReviewState::Awaiting { passes: 0, required: policy.required },
    };
    Some(RoomReview { policy, state })
}

/// The identity owner this node has: its resident registry, or none (every peer its own mind).
pub(crate) fn minds() -> MindsHere {
    MindsHere(crate::persona::PersonaAircRuntimeRegistry::try_global())
}

/// [`MindOf`] over the node's registry when one is installed.
pub(crate) struct MindsHere(Option<crate::persona::PersonaAircRuntimeRegistry>);

impl MindOf for MindsHere {
    fn mind_of(&self, peer: airc_core::PeerId) -> Uuid {
        match &self.0 {
            Some(registry) => registry.mind_of(peer),
            None => PeerIsMind.mind_of(peer),
        }
    }
}
