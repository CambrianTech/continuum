//! What a card's holder can do next, in each state: the one owner of the work board's moves
//! (docs/architecture/STATE-DRIVEN-SURFACE.md).
//!
//! airc-work has states (`CardState`) and no transition table: `apply_card_state_changed`
//! accepts any state from any state. The board's verbs each check their own preconditions,
//! so the only way a resident learned which verb applied was to call it and read the refusal.
//! On the M5, 2026-11-04, Benchy held a REVIEW card (32d53734) and called `commands/help` 4
//! times and `work/submit` 5 times ("nothing to submit") because `work/review` was not on her
//! surface; Atlas called `work/list` 6 times to find where he stood. This table is what the
//! substrate knew and did not show her. Her tool surface (`cognition::tool_surface`) and her
//! held-work item (`context_streams::held_work`) both read it, so what she is told she can do
//! and what she has in hand are one decision.
//!
//! Owned here, beside the board's other facts (`card_facts`, `card_holder`), until it moves
//! down into airc-work as the board's own transition table (the follow-up the design doc names).
//! Each row was read off the verb it names (`continuum-mind/src/work/`); the doc records where
//! a verb's requirements differ from what one would guess.

use airc_core::PeerId;
use airc_work::{CardState, WorkCard, WorkCardId};

/// What a held card asks of its holder: her own work, or a verdict on someone else's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardRole {
    /// Her own implementation: she codes it and submits it.
    Code,
    /// A review card: she reads `parent`'s submission and records a verdict on it.
    Review { parent: WorkCardId },
}

impl CardRole {
    /// The card's role, from its typed link: a card that reviews another names it in `reviews`.
    pub fn of(card: &WorkCard) -> Self {
        match card.reviews {
            Some(parent) => CardRole::Review { parent },
            None => CardRole::Code,
        }
    }
}

/// One move a holder can make, named by the command (or command subtree) that makes it.
/// Declared in canonical order (by command name), so a set of moves renders and selects the
/// same bytes whatever order the table lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Move {
    /// Her hands on the checkout: every `code/` verb.
    Code,
    /// Renew her claim's lease during long work.
    Heartbeat,
    /// Write the card's ledger: what she knows, what she will test next.
    Note,
    /// Hand the card back.
    Release,
    /// Record a verdict on the parent's submission, under her review claim.
    Review,
    /// Move the card through its lifecycle (start, block, resume a review for revision).
    State,
    /// Read a submission: the parent's, for a reviewer; her own and its verdict, for an author.
    Submission,
    /// Publish her own work for review, derived from her checkout.
    Submit,
}

impl Move {
    /// Every move, in canonical order: the exhaustive list a test walks.
    pub const ALL: [Move; 8] = [
        Move::Code,
        Move::Heartbeat,
        Move::Note,
        Move::Release,
        Move::Review,
        Move::State,
        Move::Submission,
        Move::Submit,
    ];

    /// The registry's name for the move: a command, or a subtree (`code/**`) a surface
    /// selects whole. The one place these names are spelled for the board's moves; the
    /// mind's work commands pin their `NAME` against it (`work::submission` tests).
    pub const fn command(self) -> &'static str {
        match self {
            Move::Code => "code/**",
            Move::Heartbeat => "work/heartbeat",
            Move::Note => "work/note",
            Move::Release => "work/release",
            Move::Review => "work/review",
            Move::State => "work/state",
            Move::Submission => "work/submission",
            Move::Submit => "work/submit",
        }
    }
}

/// The moves a holder has, in canonical order, each once.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Moves(Vec<Move>);

impl Moves {
    fn of(moves: &[Move]) -> Self {
        let mut moves = moves.to_vec();
        moves.sort_unstable();
        moves.dedup();
        Self(moves)
    }

    pub fn none() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, mv: Move) -> bool {
        self.0.contains(&mv)
    }

    pub fn iter(&self) -> impl Iterator<Item = Move> + '_ {
        self.0.iter().copied()
    }

    /// The command names (and subtrees) a tool surface selects for these moves.
    pub fn commands(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.iter().map(Move::command)
    }
}

/// The union of several cards' moves, each once, in canonical order.
impl FromIterator<Move> for Moves {
    fn from_iter<I: IntoIterator<Item = Move>>(moves: I) -> Self {
        Self::of(&moves.into_iter().collect::<Vec<_>>())
    }
}

/// THE TABLE: what a holder of a card in `role` can do while it is in `state`. Exhaustive over
/// both enums with no wildcard arm, so a new state or role is a compile error here, never a
/// silent empty surface.
///
/// - A code card she is working (Claimed, InProgress): her hands, submit, the ledger, the
///   lifecycle, hand back, renew the lease.
/// - Blocked: the ledger, the lifecycle (to resume), hand back.
/// - Review (submitted, awaiting a verdict): read the submission and its verdict, the ledger, and
///   `work/state` — the verb that resumes her own review to in_progress for a revision
///   (`resumes_held_review`), which brings her hands and `work/submit` back. `work/submit` itself
///   would accept a corrected candidate in Review (airc `validate_for_card` refuses only Merged
///   and Closed), but without a change it is refused as the same patch, the loop Benchy ran.
/// - A review card she is working (Claimed, InProgress): read the parent's submission, record the
///   verdict, the ledger, hand back, renew the lease.
/// - A review card Blocked: the ledger, the lifecycle, hand back. In Review (her verdict is
///   recorded): read the submission, the ledger.
/// - Open, Merged, Closed: nothing is held, so nothing is in hand.
pub fn moves_in(role: CardRole, state: CardState) -> Moves {
    use Move::*;
    let moves: &[Move] = match (role, state) {
        (CardRole::Code, CardState::Claimed | CardState::InProgress) => &[Code, Submit, Note, State, Release, Heartbeat],
        (CardRole::Code, CardState::Blocked) => &[Note, State, Release],
        (CardRole::Code, CardState::Review) => &[Submission, Note, State],
        (CardRole::Review { .. }, CardState::Claimed | CardState::InProgress) => &[Submission, Review, Note, Release, Heartbeat],
        (CardRole::Review { .. }, CardState::Blocked) => &[Note, State, Release],
        (CardRole::Review { .. }, CardState::Review) => &[Submission, Note],
        (CardRole::Code | CardRole::Review { .. }, CardState::Open | CardState::Merged | CardState::Closed) => &[],
    };
    Moves::of(moves)
}

/// The moves `holder` has on `card`: none unless she owns it, else the table's row for its role
/// and state.
pub fn held_moves(card: &WorkCard, holder: PeerId) -> Moves {
    if card.owner != Some(holder) {
        return Moves::none();
    }
    moves_in(CardRole::of(card), card.state)
}

/// A card she holds, with its role and the moves its state gives her: what her surface and her
/// held-work item both read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub card: WorkCard,
    pub role: CardRole,
    pub moves: Moves,
}

impl Held {
    /// `card` as `holder` holds it; `None` when she does not own it.
    pub fn of(card: WorkCard, holder: PeerId) -> Option<Self> {
        (card.owner == Some(holder)).then(|| Self { role: CardRole::of(&card), moves: held_moves(&card, holder), card })
    }
}

/// A card for tests: `title`, in `state`, owned by `owner`, reviewing `reviews` when it is a
/// review card. The one held-card fixture, shared by every crate's tests (`test-fixtures`).
#[cfg(any(test, feature = "test-fixtures"))]
pub fn fixture_card(state: CardState, owner: Option<PeerId>, reviews: Option<WorkCardId>, title: &str) -> WorkCard {
    WorkCard {
        card_id: WorkCardId::from_u128(0x32d5_3734),
        repo: airc_work::RepoId::new("acme/continuum").expect("a fixed, valid repo id"), // expect: a literal that RepoId accepts
        title: title.to_string(),
        body: None,
        priority: airc_work::Priority::P2,
        lane_id: None,
        state,
        owner,
        claim_provenance: None,
        claim_id: None,
        claim_expires_at_ms: None,
        last_heartbeat_at_ms: None,
        pull_request: None,
        created_by: PeerId::from_uuid(uuid::Uuid::from_u128(9)),
        created_at_ms: 1,
        updated_at_ms: 1,
        reviews,
        submissions: Vec::new(),
        last_submission_rejection: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATES: [CardState; 7] = [
        CardState::Open,
        CardState::Claimed,
        CardState::InProgress,
        CardState::Blocked,
        CardState::Review,
        CardState::Merged,
        CardState::Closed,
    ];

    fn card(state: CardState, owner: Option<PeerId>, reviews: Option<WorkCardId>) -> WorkCard {
        fixture_card(state, owner, reviews, "review the parent")
    }

    // what this catches: Benchy on the M5, 2026-11-04, holding review card 32d53734 in Claimed,
    // called work/submit 5 times ("nothing to submit") because the verdict verb was not in her
    // hands. A held review card's moves are the reviewer's: read the parent's submission, record
    // the verdict; never submit.
    #[test]
    fn a_held_review_card_gives_the_reviewer_moves_never_submit() {
        let her = PeerId::from_uuid(uuid::Uuid::from_u128(1));
        let parent = WorkCardId::from_u128(7);
        let held = Held::of(card(CardState::Claimed, Some(her), Some(parent)), her).expect("she owns it");
        assert_eq!(held.role, CardRole::Review { parent });
        assert!(held.moves.contains(Move::Review) && held.moves.contains(Move::Submission), "{:?}", held.moves);
        assert!(!held.moves.contains(Move::Submit) && !held.moves.contains(Move::Code), "{:?}", held.moves);
        let code = held_moves(&card(CardState::InProgress, Some(her), None), her);
        assert!(code.contains(Move::Submit) && code.contains(Move::Code), "{code:?}");
        let other = PeerId::from_uuid(uuid::Uuid::from_u128(2));
        assert!(held_moves(&card(CardState::InProgress, Some(other), None), her).is_empty(), "another's card gives her nothing");
        assert!(Held::of(card(CardState::InProgress, Some(other), None), her).is_none());
    }

    // what this catches: a state the table does not answer (a new CardState would fail to
    // compile in `moves_in`; this walks every pair so a row that silently empties is seen), a
    // held state with no way to record progress, or moves out of canonical order (the surface's
    // bytes would move between acts with nothing changed).
    #[test]
    fn every_state_and_role_has_a_defined_row_in_canonical_order() {
        for role in [CardRole::Code, CardRole::Review { parent: WorkCardId::from_u128(7) }] {
            for state in STATES {
                let moves = moves_in(role, state);
                let listed: Vec<Move> = moves.iter().collect();
                let mut sorted = listed.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(listed, sorted, "{role:?} {state:?}");
                let held = matches!(state, CardState::Claimed | CardState::InProgress | CardState::Blocked | CardState::Review);
                assert_eq!(moves.contains(Move::Note), held, "{role:?} {state:?}: a held card always takes a ledger note");
                assert_eq!(moves_in(role, state), moves, "pure: the same inputs give the same moves");
            }
        }
        assert_eq!(Move::ALL.iter().map(|m| m.command()).collect::<std::collections::BTreeSet<_>>().len(), Move::ALL.len(), "one command per move");
    }
}
