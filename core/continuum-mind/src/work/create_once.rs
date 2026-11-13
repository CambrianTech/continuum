//! A CARD IS CREATED ONCE: the one owner every card creation in the core goes through
//! (`work/create`, the gated finisher's review card, the submission's review hand-off).
//!
//! Live, the M5 2026-11-13 (residents' board, room 11c1a7ac): Aiko re-issued one create after
//! a view turnover dropped its result and filed "Review bracket dialect parsing fixes" three
//! times in three minutes (149f1391, c250ca1d, 4e788c87); one review card stood four times
//! (175d9d66, 235fe88c, bebef7da, f56c5ccf). airc's create mints a fresh id per call, so a
//! repeat was always a new card. Here a repeat returns the card it repeats, as
//! [`CardCreation::Exists`], so her result says the card is already on the board.
//!
//! What makes two creates the same card:
//! - a filed card: the same room (the board read), the same author (her airc identity) and
//!   the same title up to case and spacing ([`title_key`]), while that card is unfinished;
//! - a review of a submission: the same parent and submission (airc's deterministic
//!   `submission_review_card_id`, which `ensure_submission_review_in` already honours);
//! - a review with no submission bound: the same parent, while a review of it is unfinished.
//!
//! A DISTINCT title is always a new card: near matches are information for her
//! (`work/create`'s `similar`), never a refusal here.

use std::sync::Arc;

use airc_core::PeerId;
use airc_lib::{Airc, AircError, CardState, CreateWorkCard, Room, WorkCard, WorkCardId};
use airc_work::{BoardSnapshot, SubmissionId};
use continuum_base::time::EpochMs;

use crate::persona::work_board::board_index;

/// The card a create asks for, typed by what makes it unique.
pub enum NewCard {
    /// A card a mind files on a room's board: once per (room, author, title).
    Filed(CreateWorkCard),
    /// A review of `parent`: once per (parent, submission). `None` = a review not bound to a
    /// submission (a bench card's, or a room card with nothing submitted).
    Review {
        parent: WorkCardId,
        submission: Option<SubmissionId>,
        card: CreateWorkCard,
    },
}

/// What a create did: the card is new, or the card it repeats already stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardCreation {
    Created(WorkCardId),
    Exists(WorkCardId),
}

impl CardCreation {
    pub fn card_id(self) -> WorkCardId {
        match self {
            Self::Created(id) | Self::Exists(id) => id,
        }
    }
}

/// A create's outcome with the board it was decided against (the room's cards BEFORE the
/// create): what `work/create` ranks near matches in, with no second read.
pub struct Created {
    pub card: CardCreation,
    pub board: Arc<BoardSnapshot>,
}

/// Create `new` on `room`'s board unless the card it repeats already stands there.
pub async fn create_once(airc: &Airc, room: &Room, new: NewCard) -> Result<Created, AircError> {
    let now_ms = EpochMs::now().millis(); // pre-1970 reads 0: leases read as not yet crossed
    let board = board_index::for_peer(airc.peer_id()).board(airc, room, now_ms).await?;
    let made = match new {
        NewCard::Review { parent, submission: Some(submission), card } => {
            let id = airc_work::submission_review_card_id(parent, submission);
            if board.cards.iter().any(|c| c.card_id == id) {
                CardCreation::Exists(id)
            } else {
                CardCreation::Created(airc.ensure_submission_review_in(room, parent, submission, card).await?)
            }
        }
        NewCard::Review { parent, submission: None, card } => match open_review_of(&board.cards, parent) {
            Some(id) => CardCreation::Exists(id),
            None => CardCreation::Created(airc.create_work_card_in(room, card.reviewing(parent)).await?),
        },
        NewCard::Filed(card) => match filed_twin(&board.cards, airc.peer_id(), &card.title) {
            Some(id) => CardCreation::Exists(id),
            None => CardCreation::Created(airc.create_work_card_in(room, card).await?),
        },
    };
    match made {
        CardCreation::Created(_) => board_index::wrote(airc, room),
        CardCreation::Exists(id) => continuum_base::probe!(
            class = "work.create.exists",
            room = %room.channel,
            card = %airc_core::shown_id::shown_form(id.as_uuid()),
            "a create repeated a card that already stands: the card is returned, none is minted"
        ),
    }
    Ok(Created { card: made, board })
}

/// PURE: a title as the board compares it, case and spacing aside ("Review  bracket" and
/// "review bracket" are one title; a different word is a different title).
fn title_key(title: &str) -> String {
    title.split_whitespace().map(str::to_lowercase).collect::<Vec<_>>().join(" ")
}

/// PURE: a card is unfinished until it merges or closes; a card in review still stands.
fn unfinished(card: &WorkCard) -> bool {
    !matches!(card.state, CardState::Merged | CardState::Closed)
}

/// PURE: her unfinished card with this title, if she already filed it.
fn filed_twin(cards: &[WorkCard], author: PeerId, title: &str) -> Option<WorkCardId> {
    let key = title_key(title);
    cards
        .iter()
        .find(|c| c.created_by == author && unfinished(c) && title_key(&c.title) == key)
        .map(|c| c.card_id)
}

/// PURE: an unfinished review of `parent`, if one stands.
fn open_review_of(cards: &[WorkCard], parent: WorkCardId) -> Option<WorkCardId> {
    cards.iter().find(|c| c.reviews == Some(parent) && unfinished(c)).map(|c| c.card_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_lib::{ClaimWorkCard, Priority, RepoId};

    async fn scope(home: &std::path::Path) -> (Arc<Airc>, Room, RepoId) {
        let airc = Arc::new(
            Airc::open_with_wire_root_for_test(home, home)
                .await
                .expect("a local airc scope opens without a daemon"),
        );
        let room = airc.join("residents").await.expect("join the residents' room");
        let repo = RepoId::new("github.com/CambrianTech/continuum").expect("repo id");
        (airc, room, repo)
    }

    async fn cards_titled(airc: &Airc, room: &Room, title: &str) -> usize {
        board_index::BoardFetch::fetch(airc, room)
            .await
            .expect("the board reads")
            .cards
            .iter()
            .filter(|c| title_key(&c.title) == title_key(title))
            .count()
    }

    // what this catches (card ac50de29; Aiko on the M5 2026-11-13, three cards for one title in
    // three minutes): a repeated create minting a second card. The repeat, even respaced and
    // recased, returns the first card as Exists; a distinct title is still a new card.
    #[tokio::test]
    async fn a_repeated_create_returns_her_card_and_a_distinct_title_is_new() {
        let home = tempfile::tempdir().expect("temp airc home");
        let (airc, room, repo) = scope(home.path()).await;
        let title = "Review bracket dialect parsing fixes";
        let filed = |t: &str| NewCard::Filed(CreateWorkCard::new(repo.clone(), t, Priority::P2));

        let first = create_once(&airc, &room, filed(title)).await.expect("first create").card;
        assert!(matches!(first, CardCreation::Created(_)), "the first create mints the card: {first:?}");
        let id = first.card_id();
        let again = create_once(&airc, &room, filed("  review bracket   dialect parsing FIXES "))
            .await
            .expect("repeat create")
            .card;
        assert_eq!(again, CardCreation::Exists(id), "the repeat names the card she already filed");
        assert_eq!(cards_titled(&airc, &room, title).await, 1, "one card on the board");

        let other = create_once(&airc, &room, filed("Review bracket dialect tests")).await.expect("distinct create").card;
        assert!(matches!(other, CardCreation::Created(o) if o != id), "a distinct title is a new card: {other:?}");
    }

    // what this catches (the M5 2026-11-13: one review card standing four times): a second
    // review spawn for the same submission, or a second unbound review of a parent whose review
    // is still open, minting another review card.
    #[tokio::test]
    async fn a_second_review_spawn_returns_the_first_review_card() {
        let home = tempfile::tempdir().expect("temp airc home");
        let (airc, room, repo) = scope(home.path()).await;
        let review = |title: &str| CreateWorkCard::new(repo.clone(), title, Priority::P1);

        // Bound to a submission.
        let parent = airc
            .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a fix", Priority::P1))
            .await
            .expect("parent card created");
        let claim = airc
            .claim_work_card(ClaimWorkCard { card_id: parent, ttl_ms: 600_000 })
            .await
            .expect("the work is claimed");
        let submitted = airc
            .submit_work_in(
                &room,
                airc_lib::SubmitWork {
                    submission_id: SubmissionId::new(),
                    card_id: parent,
                    claim_id: claim,
                    instance: "ordinary".into(),
                    base_sha: airc_work::GitObjectId::new("a".repeat(40)).expect("hex"), // expect: a literal 40-hex id
                    head: airc_work::GitObjectId::new("c".repeat(40)).expect("hex"), // expect: a literal 40-hex id
                    artifact: airc_work::SubmissionArtifact {
                        hash: airc_blobs::ContentHash::from_bytes(b"patch"),
                        size_bytes: 5,
                        mime: Some("text/x-patch".into()),
                    },
                },
            )
            .await
            .expect("submitted");
        let spawn = |title: &str| NewCard::Review { parent, submission: Some(submitted.submission_id), card: review(title) };
        let first = create_once(&airc, &room, spawn("review: a fix")).await.expect("first spawn").card;
        assert!(matches!(first, CardCreation::Created(_)), "the first spawn opens the review: {first:?}");
        let id = first.card_id();
        let again = create_once(&airc, &room, spawn("review: a fix")).await.expect("second spawn").card;
        assert_eq!(again, CardCreation::Exists(id));
        assert_eq!(cards_titled(&airc, &room, "review: a fix").await, 1, "one review card per submission");

        // Unbound: one open review per parent.
        let bench = airc
            .create_work_card_in(&room, CreateWorkCard::new(repo.clone(), "a bench card", Priority::P1))
            .await
            .expect("bench card created");
        let unbound = || NewCard::Review { parent: bench, submission: None, card: review("review: a bench card") };
        let first = create_once(&airc, &room, unbound()).await.expect("first unbound").card;
        let again = create_once(&airc, &room, unbound()).await.expect("second unbound").card;
        assert_eq!(again, CardCreation::Exists(first.card_id()), "{first:?} then {again:?}");
        assert_eq!(cards_titled(&airc, &room, "review: a bench card").await, 1);
    }
}
