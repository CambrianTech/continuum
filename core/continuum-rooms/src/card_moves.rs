//! What an actor can do next on a card, in each state: the one owner of the work board's moves
//! (docs/architecture/STATE-DRIVEN-SURFACE.md).
//!
//! airc-work has states (`CardState`) and no transition table: `apply_card_state_changed`
//! accepts any state from any state. The board's verbs each check their own preconditions,
//! so the only way a resident learned which verb applied was to call it and read the refusal.
//! On the M5, 2026-11-04, Benchy held a REVIEW card (32d53734) and called `commands/help` 4
//! times and `work/submit` 5 times ("nothing to submit") because `work/review` was not on her
//! surface; Atlas called `work/list` 6 times to find where he stood. This table is what the
//! substrate knew and did not show her. Her tool surface (`cognition::tool_surface`), her
//! held-work item (`context_streams::held_work`) and a refusal's next moves all read it, so what
//! she is told she can do and what she has in hand are one decision.
//!
//! The table is data: (role, state, stance) -> moves or a typed reason there are none
//! ([`moves_in`]). It lives here, beside the board's other facts (`card_facts`, `card_holder`),
//! until it moves down into airc-work as the board's own transition table (Cormac, after this
//! merges). Each row was read off the verb it names (`continuum-mind/src/work/`); the doc records
//! where a verb's requirements differ from what one would guess.

use airc_core::PeerId;
use airc_work::{CardState, WorkCard, WorkCardId};

/// What a card asks of its holder: her own work, or a verdict on someone else's.
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

/// Where the actor stands to the card: its holder, nobody's, or someone else's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stance {
    Holder,
    Unheld,
    HeldBy(PeerId),
}

impl Stance {
    /// `actor`'s stance to `card`, from its owner.
    pub fn of(card: &WorkCard, actor: PeerId) -> Self {
        match card.owner {
            Some(owner) if owner == actor => Stance::Holder,
            Some(owner) => Stance::HeldBy(owner),
            None => Stance::Unheld,
        }
    }
}

/// One move an actor can make, named by the command (or command subtree) that makes it.
/// Declared in canonical order (by command name), so a set of moves renders and selects the
/// same bytes whatever order the table lists them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Move {
    /// Her hands on the checkout: every `code/` verb.
    Code,
    /// Take the card.
    Claim,
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
    pub const ALL: [Move; 9] = [
        Move::Code,
        Move::Claim,
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
    /// mind's work commands pin their `NAME` against it (`work` tests).
    pub const fn command(self) -> &'static str {
        match self {
            Move::Code => "code/**",
            Move::Claim => "work/claim",
            Move::Heartbeat => "work/heartbeat",
            Move::Note => "work/note",
            Move::Release => "work/release",
            Move::Review => "work/review",
            Move::State => "work/state",
            Move::Submission => "work/submission",
            Move::Submit => "work/submit",
        }
    }

    /// Whether the move names the command `name`: equal to a command, or under a subtree.
    pub fn names(self, name: &str) -> bool {
        let pattern = self.command();
        match pattern.strip_suffix("/**") {
            Some(subtree) => name.strip_prefix(subtree).is_some_and(|rest| rest.starts_with('/')),
            None => pattern == name,
        }
    }
}

/// Why an actor has no move on a card. A refusal carries this or the moves, never neither.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoMove {
    /// Another actor holds it.
    Held { by: PeerId },
    /// Its lane is over for her: Merged or Closed, or in Review awaiting a verdict she does not
    /// hold the card for.
    Ended(CardState),
}

/// A non-empty set of moves in canonical order, each once. Built only by [`MoveSet::of`], so an
/// empty set cannot exist.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MoveSet(Vec<Move>);

impl MoveSet {
    /// The set of `moves`; `None` when there are none.
    pub fn of(moves: &[Move]) -> Option<Self> {
        let mut moves = moves.to_vec();
        moves.sort_unstable();
        moves.dedup();
        (!moves.is_empty()).then_some(Self(moves))
    }

    pub fn iter(&self) -> impl Iterator<Item = Move> + '_ {
        self.0.iter().copied()
    }
}

/// What an actor can do on a card: some moves, or a typed reason there are none. A bare empty
/// list is unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Moves {
    Some(MoveSet),
    None(NoMove),
}

impl Moves {
    /// The moves, in canonical order; none for [`Moves::None`].
    pub fn iter(&self) -> impl Iterator<Item = Move> + '_ {
        let set = match self {
            Moves::Some(set) => Some(set),
            Moves::None(_) => None,
        };
        set.into_iter().flat_map(|set| set.iter())
    }

    pub fn contains(&self, mv: Move) -> bool {
        self.iter().any(|m| m == mv)
    }

    /// The command names (and subtrees) a tool surface selects for these moves.
    pub fn commands(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.iter().map(Move::command)
    }
}

/// THE TABLE: what an actor in `stance` can do on a card in `role` while it is in `state`.
/// Exhaustive over all three with no wildcard, so a new state, role or stance is a compile error
/// here, never a silent empty surface.
///
/// - Open: anyone claims it (an owner left on an Open card is stale).
/// - Merged, Closed: ended for everyone.
/// - Her code card, Claimed / InProgress: her hands, submit, the ledger, the lifecycle, hand
///   back, renew the lease. Blocked: the ledger, the lifecycle (to resume), hand back. Review
///   (submitted, awaiting a verdict): read the submission and its verdict, the ledger, and
///   `work/state`, which resumes her own review to in_progress for a revision
///   (`resumes_held_review`) and brings her hands and `work/submit` back. `work/submit` itself
///   would accept a corrected candidate in Review (airc `validate_for_card` refuses only Merged
///   and Closed), but without a change it is refused as the same patch, the loop Benchy ran.
/// - Her review card, Claimed / InProgress: read the parent's submission, record the verdict,
///   the ledger, hand back, renew the lease. Blocked: the ledger, the lifecycle, hand back.
///   Review (her verdict recorded): read the submission, the ledger.
/// - Someone else's card, Claimed / InProgress / Blocked: held by them. Unheld in those states
///   (a column moved with no owner): claim it. Review, not hers: ended for her until its verdict.
pub fn moves_in(role: CardRole, state: CardState, stance: Stance) -> Moves {
    use CardState as S;
    use Move::*;
    let row: Result<&[Move], NoMove> = match (stance, state, role) {
        (Stance::Holder | Stance::Unheld | Stance::HeldBy(_), S::Open, CardRole::Code | CardRole::Review { .. }) => Ok(&[Claim]),
        (Stance::Holder | Stance::Unheld | Stance::HeldBy(_), S::Merged | S::Closed, CardRole::Code | CardRole::Review { .. }) => {
            Err(NoMove::Ended(state))
        }
        (Stance::Holder, S::Claimed | S::InProgress, CardRole::Code) => Ok(&[Code, Submit, Note, State, Release, Heartbeat]),
        (Stance::Holder, S::Blocked, CardRole::Code) => Ok(&[Note, State, Release]),
        (Stance::Holder, S::Review, CardRole::Code) => Ok(&[Submission, Note, State]),
        (Stance::Holder, S::Claimed | S::InProgress, CardRole::Review { .. }) => Ok(&[Submission, Review, Note, Release, Heartbeat]),
        (Stance::Holder, S::Blocked, CardRole::Review { .. }) => Ok(&[Note, State, Release]),
        (Stance::Holder, S::Review, CardRole::Review { .. }) => Ok(&[Submission, Note]),
        (Stance::HeldBy(by), S::Claimed | S::InProgress | S::Blocked, CardRole::Code | CardRole::Review { .. }) => Err(NoMove::Held { by }),
        (Stance::Unheld, S::Claimed | S::InProgress | S::Blocked, CardRole::Code | CardRole::Review { .. }) => Ok(&[Claim]),
        (Stance::Unheld | Stance::HeldBy(_), S::Review, CardRole::Code | CardRole::Review { .. }) => Err(NoMove::Ended(S::Review)),
    };
    match row {
        Ok(moves) => MoveSet::of(moves).map_or(Moves::None(NoMove::Ended(state)), Moves::Some), // map_or: every Ok row above lists at least one move; an empty one would read as ended, never as an empty list
        Err(none) => Moves::None(none),
    }
}

/// What `actor` can do on `card` now: the table's row for its role, state and her stance.
pub fn moves_for(card: &WorkCard, actor: PeerId) -> Moves {
    moves_in(CardRole::of(card), card.state, Stance::of(card, actor))
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
        (card.owner == Some(holder)).then(|| Self { role: CardRole::of(&card), moves: moves_for(&card, holder), card })
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
    use CardState as S;
    use Move::*;

    const STATES: [CardState; 7] = [S::Open, S::Claimed, S::InProgress, S::Blocked, S::Review, S::Merged, S::Closed];

    fn her() -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    fn other() -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(2))
    }

    fn reviewer() -> CardRole {
        CardRole::Review { parent: WorkCardId::from_u128(7) }
    }

    fn some(moves: &[Move]) -> Moves {
        Moves::Some(MoveSet::of(moves).expect("a non-empty row in the fixture")) // expect: the literal rows below are non-empty
    }

    // what this catches: Benchy on the M5, 2026-11-04, holding review card 32d53734 in Claimed,
    // called work/submit 5 times ("nothing to submit") because the verdict verb was not in her
    // hands. A held review card's moves are the reviewer's, never submit; a held code card's are
    // the author's; read through the card, her stance comes from its owner.
    #[test]
    fn a_held_review_card_gives_the_reviewer_moves_never_submit() {
        let held = Held::of(fixture_card(S::Claimed, Some(her()), Some(WorkCardId::from_u128(7)), "r"), her()).expect("she owns it");
        assert_eq!(held.role, reviewer());
        assert_eq!(held.moves, some(&[Submission, Review, Note, Release, Heartbeat]));
        assert!(!held.moves.contains(Submit) && !held.moves.contains(Code));
        let code = moves_for(&fixture_card(S::InProgress, Some(her()), None, "c"), her());
        assert!(code.contains(Submit) && code.contains(Code), "{code:?}");
        assert!(Held::of(fixture_card(S::InProgress, Some(other()), None, "c"), her()).is_none());
    }

    // what this catches: a row that drifts from the verbs' own requirements, one test per row:
    // the holder's rows for both roles, and the non-holder rows a refusal reads (claim an open or
    // unowned card; another's card names who holds it; an ended or awaiting-verdict card says so).
    #[test]
    fn every_row_of_the_table() {
        for role in [CardRole::Code, reviewer()] {
            for stance in [Stance::Holder, Stance::Unheld, Stance::HeldBy(other())] {
                assert_eq!(moves_in(role, S::Open, stance), some(&[Claim]), "{role:?} {stance:?}");
                for ended in [S::Merged, S::Closed] {
                    assert_eq!(moves_in(role, ended, stance), Moves::None(NoMove::Ended(ended)));
                }
            }
            for state in [S::Claimed, S::InProgress, S::Blocked] {
                assert_eq!(moves_in(role, state, Stance::HeldBy(other())), Moves::None(NoMove::Held { by: other() }));
                assert_eq!(moves_in(role, state, Stance::Unheld), some(&[Claim]));
            }
            for stance in [Stance::Unheld, Stance::HeldBy(other())] {
                assert_eq!(moves_in(role, S::Review, stance), Moves::None(NoMove::Ended(S::Review)));
            }
        }
        let code = |state| moves_in(CardRole::Code, state, Stance::Holder);
        assert_eq!(code(S::Claimed), some(&[Code, Submit, Note, State, Release, Heartbeat]));
        assert_eq!(code(S::InProgress), code(S::Claimed));
        assert_eq!(code(S::Blocked), some(&[Note, State, Release]));
        assert_eq!(code(S::Review), some(&[Submission, Note, State]));
        let review = |state| moves_in(reviewer(), state, Stance::Holder);
        assert_eq!(review(S::Claimed), some(&[Submission, Review, Note, Release, Heartbeat]));
        assert_eq!(review(S::InProgress), review(S::Claimed));
        assert_eq!(review(S::Blocked), some(&[Note, State, Release]));
        assert_eq!(review(S::Review), some(&[Submission, Note]));
    }

    // what this catches: a role x state x stance the table does not answer (a new variant fails to
    // compile in `moves_in`; this walks every one for a defined result), moves out of canonical
    // order (the surface's bytes would move between acts with nothing changed), or one command
    // spelled for two moves.
    #[test]
    fn every_role_state_and_stance_has_a_defined_result_in_canonical_order() {
        for role in [CardRole::Code, reviewer()] {
            for stance in [Stance::Holder, Stance::Unheld, Stance::HeldBy(other())] {
                for state in STATES {
                    let moves = moves_in(role, state, stance);
                    let listed: Vec<Move> = moves.iter().collect();
                    let mut sorted = listed.clone();
                    sorted.sort_unstable();
                    sorted.dedup();
                    assert_eq!(listed, sorted, "{role:?} {state:?} {stance:?}");
                    assert_eq!(moves_in(role, state, stance), moves, "pure");
                }
            }
        }
        assert!(MoveSet::of(&[]).is_none(), "an empty set is unrepresentable");
        assert_eq!(Move::ALL.iter().map(|m| m.command()).collect::<std::collections::BTreeSet<_>>().len(), Move::ALL.len(), "one command per move");
        assert!(Code.names("code/edit") && !Code.names("codex/run") && Claim.names("work/claim"));
    }
}
