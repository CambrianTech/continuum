//! Each work verb's PRECONDITION, as a typed, pure predicate over (card, actor): the one owner of
//! what may execute on a card (docs/architecture/STATE-DRIVEN-SURFACE.md).
//!
//! The verb's handler calls its predicate at the mutation boundary, against the card as the board
//! holds it NOW (never the snapshot she acted from), with the arguments she actually passed
//! ([`Request`]: the claim she names, the revision she saw, the state she asks for); everything
//! that tells a resident what she can do calls the same predicate with the "any valid arguments"
//! form ([`Request::any`]): her tool surface
//! (the verbs whose precondition holds on a card she holds), her held-work item ("you can: ..."),
//! and a refusal's next moves (the verbs whose precondition holds for that card and actor). There
//! is no separate table of moves, so what she is shown cannot drift from what executes.
//!
//! Decisions are deterministic: a predicate reads the time it is given (`at_ms`, the time the
//! mutation's event will record), never the wall clock, so replaying recorded inputs replays the
//! decision.
//!
//! The failure this ends: on the M5, 2026-11-04, Benchy held REVIEW card 32d53734 and called
//! `commands/help` 4 times and `work/submit` 5 times because `work/review` was not on her surface;
//! Atlas called `work/list` 6 times to find where he stood. The substrate knew and did not say.
//!
//! The predicates mirror airc's own guards where airc is the authority (claim: airc-lib's
//! not-claimable states and the projection's `apply_card_claimed`, projection/apply.rs:285-312;
//! submit: `WorkSubmission::validate_for_card`, event.rs:343-378). They live here, beside the
//! board's other facts (`card_facts`, `card_holder`), so cognition (the surface) and mind (the
//! handlers) both read them; they move down into airc-work with the board's transition rules
//! (Cormac, after this merges).

use airc_core::PeerId;
use airc_work::{CardState, ClaimId, WorkCard, WorkCardId};

use crate::card_holder::refused_by_claim;

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

/// A work verb whose execution depends on the card it acts on. Declared in canonical order (by
/// command name), so a set of verbs renders and selects the same bytes whatever order it was
/// built in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkVerb {
    Claim,
    Heartbeat,
    Note,
    Release,
    Review,
    State,
    Submission,
    Submit,
}

impl WorkVerb {
    /// Every verb, in canonical order: the exhaustive list the surface and the tests walk.
    pub const ALL: [WorkVerb; 8] = [
        WorkVerb::Claim,
        WorkVerb::Heartbeat,
        WorkVerb::Note,
        WorkVerb::Release,
        WorkVerb::Review,
        WorkVerb::State,
        WorkVerb::Submission,
        WorkVerb::Submit,
    ];

    /// The registry's name for the verb. The mind's work commands pin their `NAME` against it.
    pub const fn command(self) -> &'static str {
        match self {
            WorkVerb::Claim => "work/claim",
            WorkVerb::Heartbeat => "work/heartbeat",
            WorkVerb::Note => "work/note",
            WorkVerb::Release => "work/release",
            WorkVerb::Review => "work/review",
            WorkVerb::State => "work/state",
            WorkVerb::Submission => "work/submission",
            WorkVerb::Submit => "work/submit",
        }
    }

    /// What holding this verb's precondition entitles her to besides the verb itself, as command
    /// names or subtrees a tool surface selects: an author edits her checkout to submit, and a
    /// reviewer reads the parent's submission and runs its tests in the checkout under review
    /// (`code/**`: read, search, diff and the test runners), whatever the room's recipe declares.
    pub fn grants(self) -> &'static [&'static str] {
        match self {
            WorkVerb::Submit => &[CODE_SUBTREE],
            WorkVerb::Review => &[CODE_SUBTREE, "work/submission"],
            WorkVerb::Claim | WorkVerb::Heartbeat | WorkVerb::Note | WorkVerb::Release | WorkVerb::State | WorkVerb::Submission => &[],
        }
    }
}

/// Her hands on a checkout: every `code/` verb, selected whole.
pub const CODE_SUBTREE: &str = "code/**";

/// Whether `pattern` (a command name, or a subtree `code/**`) names `command`.
pub fn names(pattern: &str, command: &str) -> bool {
    match pattern.strip_suffix("/**") {
        Some(subtree) => command.strip_prefix(subtree).is_some_and(|rest| rest.starts_with('/')),
        None => pattern == command,
    }
}

/// Why a verb's precondition does not hold: the typed reason a refusal carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unmet {
    /// Another actor holds the card: a live lease, or a lapsed claim still on record (airc keeps
    /// a card its holder's while she is away, `TAKEOVER_ON_RECORD_SINCE_MS`).
    HeldBy(PeerId),
    /// Nobody's claim names her: claim it first.
    NotHeld,
    /// The claim verb refuses this state (Review, Merged, Closed).
    Settled(CardState),
    /// The card's lane is over (Merged, Closed): nothing more is published on it.
    Ended(CardState),
    /// A submit on a review card: a reviewer's word is a verdict, through `work/review`.
    ReviewCard { parent: WorkCardId },
    /// A review on a card that reviews nothing.
    NotAReviewCard,
    /// The review card reviews `reviews`, not the card named.
    ReviewsAnother { reviews: WorkCardId },
    /// The card has no submission to read.
    NoSubmission,
    /// `work/state open` from the card's own holder would drop her claim and leave her diff.
    OpenByHolder,
    /// The claim she named is not the card's claim now (`current`): a stale receipt.
    StaleClaim { current: Option<ClaimId> },
    /// The card changed since the revision she acted from (`current` is its revision now).
    StaleRevision { current: u64 },
}

impl std::fmt::Display for Unmet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown = |id: uuid::Uuid| airc_core::shown_id::shown_form(id);
        match self {
            Unmet::HeldBy(peer) => write!(f, "the card is held by peer {}", shown(peer.as_uuid())),
            Unmet::NotHeld => write!(f, "you do not hold this card: claim it first (work/claim)"),
            Unmet::Settled(state) => write!(f, "a card in {} is not claimable", crate::card_facts::state_str(state)),
            Unmet::Ended(state) => write!(f, "the card is {}: its work is over", crate::card_facts::state_str(state)),
            Unmet::ReviewCard { parent } => write!(
                f,
                "this is a review card for card {}: your verdict goes through work/review, not work/submit",
                shown(parent.as_uuid())
            ),
            Unmet::NotAReviewCard => write!(f, "the card you named is not a review card (it reviews nothing)"),
            Unmet::ReviewsAnother { reviews } => write!(f, "this review card reviews card {}, not the one named", shown(reviews.as_uuid())),
            Unmet::NoSubmission => write!(f, "the card has no submission yet"),
            Unmet::StaleClaim { current: Some(current) } => write!(
                f,
                "the claim id you passed is not this card's live claim ({}): omit claim_id and the live claim is read off the board",
                shown(current.as_uuid())
            ),
            Unmet::StaleClaim { current: None } => write!(f, "the claim you named has ended: the card has no claim now"),
            Unmet::StaleRevision { current } => write!(f, "the card changed since you read it (revision {current}): read it again"),
            Unmet::OpenByHolder => write!(
                f,
                "you HOLD this card: `open` from its holder would drop your claim and leave your diff behind \
                 (to read it: work/get; to hand it back: work/release with a note)"
            ),
        }
    }
}

/// How `work/state` executes a requested state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateRoute {
    /// The column moves as asked.
    Direct,
    /// A holder's column (claimed, in_progress) is held through the one claim path.
    ThroughClaim,
    /// Her own live review returns to in_progress for a revision, under the same claim.
    ResumeReview,
}

/// The claim a command acts under: hers as the board holds it, or one she names (a receipt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimArg {
    Hers,
    Named(ClaimId),
}

/// What an execution brings beyond the card and the actor: when its event will be recorded, the
/// claim it names, and the card revision it acted from (`None`: not stated). Discoverability asks
/// with [`Request::any`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub at_ms: u64,
    pub claim: ClaimArg,
    pub seen_revision: Option<u64>,
}

impl Request {
    /// The "any valid arguments" form: her own claim, no stated revision.
    pub fn any(at_ms: u64) -> Self {
        Self { at_ms, claim: ClaimArg::Hers, seen_revision: None }
    }
}

/// The claim `actor` holds on `card` for `request`: hers when the card's owner is her and a claim
/// is on it, whatever the lease (an act renews a lapsed lease; responsibility outlives presence).
/// A named claim must be the card's claim now, and a stated revision the card's revision now.
pub fn holder_claim(card: &WorkCard, actor: PeerId, request: &Request) -> Result<ClaimId, Unmet> {
    if request.seen_revision.is_some_and(|seen| seen != card.updated_at_ms) {
        return Err(Unmet::StaleRevision { current: card.updated_at_ms });
    }
    let claim = match (card.owner, card.claim_id) {
        (Some(owner), Some(claim)) if owner == actor => claim,
        (Some(owner), _) if owner != actor => return Err(Unmet::HeldBy(owner)),
        (Some(_), _) | (None, _) => return Err(Unmet::NotHeld),
    };
    match request.claim {
        ClaimArg::Named(named) if named != claim => Err(Unmet::StaleClaim { current: Some(claim) }),
        ClaimArg::Named(_) | ClaimArg::Hers => Ok(claim),
    }
}

/// The peer whose claim stops anyone else's, or `None`: a live lease, or a claim still on record
/// (the projection ignores a stranger's claim on it, apply.rs:293). A card the claim verb refuses
/// anyway has no contending holder: its refusal is its state.
pub fn holder_on_record(card: &WorkCard, now_ms: u64) -> Option<PeerId> {
    if refused_by_claim(card.state) {
        return None;
    }
    let owner = card.owner?;
    let live = card.claim_expires_at_ms.is_some_and(|expires| expires > now_ms);
    let on_record = card.claim_id.is_some() && now_ms >= airc_work::TAKEOVER_ON_RECORD_SINCE_MS;
    (live || on_record).then_some(owner)
}

/// `work/claim`: not a settled state, and nobody else's claim stands. Her own claim resumes (the
/// verb answers with it). An unowned card in any working column is claimable end to end: the
/// projection skips a claim only while a claim stands (apply.rs:285-300).
pub fn claim(card: &WorkCard, actor: PeerId, now_ms: u64) -> Result<(), Unmet> {
    if refused_by_claim(card.state) {
        return Err(Unmet::Settled(card.state));
    }
    match holder_on_record(card, now_ms) {
        Some(holder) if holder != actor => Err(Unmet::HeldBy(holder)),
        Some(_) | None => Ok(()),
    }
}

/// `work/submit`: her own (not a review) card, not ended, under her claim. Review is allowed:
/// a revised candidate is publishable there (`validate_for_card`); an identical patch is answered
/// at submission identity, never refused here.
pub fn submit(card: &WorkCard, actor: PeerId, request: &Request) -> Result<ClaimId, Unmet> {
    if matches!(card.state, CardState::Merged | CardState::Closed) {
        return Err(Unmet::Ended(card.state));
    }
    if let Some(parent) = card.reviews {
        return Err(Unmet::ReviewCard { parent });
    }
    holder_claim(card, actor, request)
}

/// `work/review` on `review_card`: it reviews something (the `named` parent, when one is named),
/// and she holds it, unless she states the review claim herself (`stated_claim`, an out-of-band
/// review). Returns the claim the verdict is recorded under and the card it judges.
pub fn review(
    review_card: &WorkCard,
    actor: PeerId,
    named: Option<WorkCardId>,
    stated_claim: Option<ClaimId>,
) -> Result<(ClaimId, WorkCardId), Unmet> {
    let parent = review_card.reviews.ok_or(Unmet::NotAReviewCard)?;
    if named.is_some_and(|named| named != parent) {
        return Err(Unmet::ReviewsAnother { reviews: parent });
    }
    let claim = match stated_claim {
        Some(claim) => claim,
        None => holder_claim(review_card, actor, &Request::any(0))?,
    };
    Ok((claim, parent))
}

/// `work/submission`: the card carries a submission to read.
pub fn submission(card: &WorkCard) -> Result<(), Unmet> {
    if card.submissions.is_empty() {
        return Err(Unmet::NoSubmission);
    }
    Ok(())
}

/// `work/state` to `requested`: a holder never opens her own card; a holder's column goes through
/// the claim path, except her own live review returning to in_progress for a revision. `now_ms`
/// is the time the transition's event will record.
pub fn state(card: &WorkCard, actor: PeerId, requested: CardState, now_ms: u64) -> Result<StateRoute, Unmet> {
    let holds = card.owner == Some(actor);
    match requested {
        CardState::Open if holds => Err(Unmet::OpenByHolder),
        CardState::InProgress if resumes_held_review(card, actor, now_ms) => Ok(StateRoute::ResumeReview),
        CardState::Claimed | CardState::InProgress => claim(card, actor, now_ms).map(|()| StateRoute::ThroughClaim),
        CardState::Open | CardState::Blocked | CardState::Review | CardState::Merged | CardState::Closed => Ok(StateRoute::Direct),
    }
}

/// Her own live review returns to in_progress under the same claim; never another's, and never
/// done work (Kimi, 2026-09-30: re-claiming refused Review as settled, so the owner could never
/// leave it).
pub fn resumes_held_review(card: &WorkCard, actor: PeerId, now_ms: u64) -> bool {
    card.state == CardState::Review
        && card.owner == Some(actor)
        && crate::card_holder::hold_of(card, now_ms) == crate::card_holder::Hold::Held
}

/// Whether `verb` can execute on `card` for `actor` with some argument: the question the surface
/// and a refusal's next moves ask. For `work/state`, a holder always has a lifecycle move; for
/// `work/review`, the review is her own (no stated claim, no named parent).
pub fn check(verb: WorkVerb, card: &WorkCard, actor: PeerId, now_ms: u64) -> Result<(), Unmet> {
    match verb {
        WorkVerb::Claim => claim(card, actor, now_ms),
        WorkVerb::Heartbeat | WorkVerb::Release | WorkVerb::State => holder_claim(card, actor, &Request::any(now_ms)).map(|_| ()),
        WorkVerb::Note => Ok(()), // any card on a board she stands in takes a ledger note
        WorkVerb::Review => review(card, actor, None, None).map(|_| ()),
        WorkVerb::Submission => submission(card),
        WorkVerb::Submit => submit(card, actor, &Request::any(now_ms)).map(|_| ()),
    }
}

/// The verbs whose precondition holds for `actor` on `card`, in canonical order.
pub fn available(card: &WorkCard, actor: PeerId, now_ms: u64) -> Vec<WorkVerb> {
    WorkVerb::ALL.into_iter().filter(|verb| check(*verb, card, actor, now_ms).is_ok()).collect()
}

/// What `actor` has in hand on `card`: every available verb and what each one grants, as command
/// names and subtrees, each once, in canonical order. Stable while the card's facts are.
pub fn in_hand(card: &WorkCard, actor: PeerId, now_ms: u64) -> Vec<&'static str> {
    let verbs = available(card, actor, now_ms);
    let mut commands: Vec<&'static str> = verbs.iter().map(|verb| verb.command()).collect();
    commands.extend(verbs.iter().flat_map(|verb| verb.grants().iter().copied()));
    commands.sort_unstable();
    commands.dedup();
    commands
}

/// A verb refused at the mutation boundary: why, the card as the board holds it now, and what
/// she can do on it now (refreshed, never the snapshot she acted from). Cormac's refusal reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub unmet: Unmet,
    pub state: CardState,
    pub owner: Option<PeerId>,
    pub claim: Option<ClaimId>,
    pub revision: u64,
    pub next: Vec<WorkVerb>,
}

impl Refusal {
    /// `unmet`, with `card_now` (the board's card at the boundary) and her moves on it at `at_ms`.
    pub fn at(unmet: Unmet, card_now: &WorkCard, actor: PeerId, at_ms: u64) -> Self {
        Self {
            unmet,
            state: card_now.state,
            owner: card_now.owner,
            claim: card_now.claim_id,
            revision: card_now.updated_at_ms,
            next: available(card_now, actor, at_ms),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let next: Vec<&str> = self.next.iter().map(|verb| verb.command()).collect();
        write!(f, "{}. The card is {} now (revision {}); you can: {}", self.unmet, crate::card_facts::state_str(&self.state), self.revision, next.join(", "))
    }
}

/// Decide `verb` at the mutation boundary: its precondition over the card as the board holds it
/// now, with the request she made; refused, the refusal carries the refreshed card and her moves.
pub fn decide<T>(decision: Result<T, Unmet>, card_now: &WorkCard, actor: PeerId, at_ms: u64) -> Result<T, Refusal> {
    decision.map_err(|unmet| Refusal::at(unmet, card_now, actor, at_ms))
}

/// A card she holds: its role, when it entered its current state (the producer's revision of
/// it, so a re-entry is a new occurrence), and what she has in hand on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub card: WorkCard,
    pub role: CardRole,
    /// The board's `updated_at_ms` at the first read that saw the card in its current state.
    pub entered_ms: u64,
    pub in_hand: Vec<&'static str>,
}

impl Held {
    /// `card` as `holder` holds it at `now_ms`; `None` when she does not own it.
    pub fn of(card: WorkCard, holder: PeerId, now_ms: u64, entered_ms: u64) -> Option<Self> {
        (card.owner == Some(holder)).then(|| Self {
            role: CardRole::of(&card),
            in_hand: in_hand(&card, holder, now_ms),
            entered_ms,
            card,
        })
    }
}

/// A card for tests: `title`, in `state`, owned by `owner` under a claim when owned, reviewing
/// `reviews` when it is a review card. The one held-card fixture, shared by every crate's tests
/// (`test-fixtures`).
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
        claim_id: owner.map(|_| ClaimId::from_u128(0xc1a1)),
        claim_expires_at_ms: owner.map(|_| FIXTURE_NOW_MS + 60_000),
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

/// The clock the fixture's lease is live at: past airc's takeover-on-record date.
#[cfg(any(test, feature = "test-fixtures"))]
pub const FIXTURE_NOW_MS: u64 = airc_work::TAKEOVER_ON_RECORD_SINCE_MS + 1_000;

#[cfg(test)]
mod tests {
    use super::*;
    use CardState as S;

    const NOW: u64 = FIXTURE_NOW_MS;

    fn her() -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    fn other() -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(2))
    }

    fn parent() -> WorkCardId {
        WorkCardId::from_u128(7)
    }

    // what this catches: Benchy on the M5, 2026-11-04, holding review card 32d53734 in Claimed,
    // called work/submit 5 times because the verdict verb was not in her hands. Her review card's
    // precondition-satisfied set is the reviewer's: the verdict, the parent's submission and the
    // code hands to run its tests, never submit; a code card's is the author's, Review included.
    #[test]
    fn a_held_review_card_puts_the_reviewers_verbs_in_hand_never_submit() {
        let review = fixture_card(S::Claimed, Some(her()), Some(parent()), "r");
        assert_eq!(check(WorkVerb::Submit, &review, her(), NOW), Err(Unmet::ReviewCard { parent: parent() }));
        let hand = in_hand(&review, her(), NOW);
        for wanted in ["work/review", "work/submission", "code/**", "work/note"] {
            assert!(hand.contains(&wanted), "{wanted}: {hand:?}");
        }
        assert!(!hand.contains(&"work/submit"), "{hand:?}");
        for state in [S::Claimed, S::InProgress, S::Review] {
            let code = in_hand(&fixture_card(state, Some(her()), None, "c"), her(), NOW);
            assert!(code.contains(&"work/submit") && code.contains(&"code/**"), "{state:?}: a revision is publishable in Review: {code:?}");
        }
    }

    // what this catches: a predicate that drifts from what executes, one case per rule: claim
    // (settled states refuse; another's claim, live or on record, stands; hers resumes; an unowned
    // working card is claimable), the holder verbs, submit (ended, review card), review (not a review
    // card, another parent, a stated claim), submission, and state's three routes and its refusal.
    #[test]
    fn every_precondition_rule() {
        for state in [S::Review, S::Merged, S::Closed] {
            assert_eq!(claim(&fixture_card(state, None, None, "x"), her(), NOW), Err(Unmet::Settled(state)));
        }
        let theirs = fixture_card(S::InProgress, Some(other()), None, "x");
        assert_eq!(claim(&theirs, her(), NOW), Err(Unmet::HeldBy(other())));
        let mut lapsed = theirs.clone();
        lapsed.claim_expires_at_ms = Some(NOW - 1);
        assert_eq!(claim(&lapsed, her(), NOW), Err(Unmet::HeldBy(other())), "on record: airc keeps it hers while she is away");
        assert_eq!(claim(&fixture_card(S::InProgress, Some(her()), None, "x"), her(), NOW), Ok(()), "her own claim resumes");
        for state in [S::Open, S::Claimed, S::InProgress, S::Blocked] {
            assert_eq!(claim(&fixture_card(state, None, None, "x"), her(), NOW), Ok(()), "{state:?} unowned");
        }
        let any = Request::any(NOW);
        assert_eq!(holder_claim(&theirs, her(), &any), Err(Unmet::HeldBy(other())));
        assert_eq!(holder_claim(&fixture_card(S::Open, None, None, "x"), her(), &any), Err(Unmet::NotHeld));
        for state in [S::Merged, S::Closed] {
            assert_eq!(submit(&fixture_card(state, Some(her()), None, "x"), her(), &any), Err(Unmet::Ended(state)));
        }
        let review_card = fixture_card(S::InProgress, Some(her()), Some(parent()), "r");
        assert_eq!(review(&fixture_card(S::InProgress, Some(her()), None, "x"), her(), None, None), Err(Unmet::NotAReviewCard));
        assert_eq!(review(&review_card, her(), Some(WorkCardId::from_u128(8)), None), Err(Unmet::ReviewsAnother { reviews: parent() }));
        assert_eq!(review(&review_card, other(), None, None), Err(Unmet::HeldBy(her())));
        let stated = ClaimId::from_u128(5);
        assert_eq!(review(&review_card, other(), Some(parent()), Some(stated)), Ok((stated, parent())), "an out-of-band review states its claim");
        assert_eq!(submission(&review_card), Err(Unmet::NoSubmission));
        let mine = fixture_card(S::InProgress, Some(her()), None, "x");
        assert_eq!(state(&mine, her(), S::Open, NOW), Err(Unmet::OpenByHolder));
        assert_eq!(state(&mine, her(), S::Review, NOW), Ok(StateRoute::Direct));
        assert_eq!(state(&mine, her(), S::InProgress, NOW), Ok(StateRoute::ThroughClaim));
        let in_review = fixture_card(S::Review, Some(her()), None, "x");
        assert_eq!(state(&in_review, her(), S::InProgress, NOW), Ok(StateRoute::ResumeReview));
        assert_eq!(state(&in_review, other(), S::InProgress, NOW), Err(Unmet::Settled(S::Review)));
        let refusal = Refusal::at(Unmet::HeldBy(other()), &theirs, her(), NOW);
        assert_eq!(refusal.next, vec![WorkVerb::Note], "on another's card she can still write its ledger");
    }

    // what this catches: a decision taken on the snapshot she acted from instead of the card now
    // (Codex on the redesign): she submits with the claim from a receipt, the card has since moved
    // (released and re-claimed, so its claim is new; and to Review). At the boundary the stale
    // claim is refused with the card as it is now and her moves on it now; a stated revision that
    // is not the card's revision is refused the same way; and the decision reads the time it is
    // given, so the same inputs decide the same way on replay.
    #[test]
    fn a_stale_claim_or_revision_is_refused_with_the_refreshed_card_and_her_moves_now() {
        let old_claim = ClaimId::from_u128(0x01d);
        let mut now_card = fixture_card(S::Review, Some(her()), None, "x");
        now_card.updated_at_ms = 42;
        let named = Request { at_ms: NOW, claim: ClaimArg::Named(old_claim), seen_revision: None };
        let refused = decide(submit(&now_card, her(), &named), &now_card, her(), NOW).expect_err("a stale claim");
        assert_eq!(refused.unmet, Unmet::StaleClaim { current: now_card.claim_id });
        assert_eq!((refused.state, refused.revision), (S::Review, 42), "the card as it is now");
        assert!(refused.next.contains(&WorkVerb::Submit) && !refused.next.contains(&WorkVerb::Claim), "her moves now: {:?}", refused.next);
        let seen = Request { at_ms: NOW, claim: ClaimArg::Hers, seen_revision: Some(41) };
        assert_eq!(submit(&now_card, her(), &seen), Err(Unmet::StaleRevision { current: 42 }));
        let current = Request { at_ms: NOW, claim: ClaimArg::Named(now_card.claim_id.expect("owned fixture")), seen_revision: Some(42) }; // expect: the fixture claims an owned card
        assert_eq!(submit(&now_card, her(), &current), Ok(now_card.claim_id.expect("owned fixture"))); // expect: as above
        let mut lapsing = fixture_card(S::InProgress, Some(other()), None, "x");
        lapsing.claim_id = None;
        lapsing.claim_expires_at_ms = Some(1_000);
        assert_eq!(claim(&lapsing, her(), 999), Err(Unmet::HeldBy(other())), "live at the recorded time");
        assert_eq!(claim(&lapsing, her(), 1_000), Ok(()), "expired at the recorded time, whatever the wall clock says");
    }

    // what this catches: an unowned InProgress card advertised as unclaimable while airc claims it
    // (Cormac, projection/apply.rs:285-300). Proven through the real projection: the claim lands
    // Claimed under the new owner, and the precondition agrees beforehand.
    #[test]
    fn an_unowned_in_progress_card_is_claimed_through_the_real_projection() {
        use airc_work::{CardCreated, CardStateChanged, ClaimOrigin, WorkBoardProjection, WorkCardClaimed, WorkEvent};
        let card_id = WorkCardId::from_u128(0x5);
        let mut board = WorkBoardProjection::new();
        let events = [
            WorkEvent::CardCreated(CardCreated {
                card_id,
                repo: airc_work::RepoId::new("acme/continuum").expect("a fixed, valid repo id"), // expect: a literal RepoId accepts
                title: "unowned working card".into(),
                body: None,
                priority: airc_work::Priority::P2,
                lane_id: None,
                created_by: other(),
                created_at_ms: NOW,
                reviews: None,
                origin: None,
            }),
            WorkEvent::CardStateChanged(CardStateChanged { card_id, state: S::InProgress, changed_by: other(), changed_at_ms: NOW + 1 }),
        ];
        for event in &events {
            board.apply(event).expect("the projection accepts the fixture's events"); // expect: test setup
        }
        let before = board.card(card_id).expect("created").clone(); // expect: created above
        assert_eq!((before.state, before.owner), (S::InProgress, None));
        assert_eq!(claim(&before, her(), NOW + 2), Ok(()), "the precondition says claimable");
        let claimed = WorkEvent::CardClaimed(WorkCardClaimed {
            selected_at_ms: None,
            card_id,
            claim_id: ClaimId::from_u128(0xc1),
            owner: her(),
            ttl_ms: 60_000,
            claimed_at_ms: NOW + 2,
            origin: ClaimOrigin::Explicit,
        });
        board.apply(&claimed).expect("the projection accepts the claim"); // expect: test setup
        let after = board.card(card_id).expect("still on the board"); // expect: claimed above
        assert_eq!((after.state, after.owner), (S::Claimed, Some(her())), "it lands Claimed under the new owner");
    }

    // what this catches: a verb or state the predicates do not answer (each `match` above has no
    // wildcard, so a new variant fails to compile; this walks every verb, state and stance for a
    // defined answer), verbs out of canonical order, or one command spelled for two verbs.
    #[test]
    fn every_verb_state_and_actor_has_a_defined_answer_in_canonical_order() {
        for state in [S::Open, S::Claimed, S::InProgress, S::Blocked, S::Review, S::Merged, S::Closed] {
            for reviews in [None, Some(parent())] {
                for owner in [Some(her()), Some(other()), None] {
                    let card = fixture_card(state, owner, reviews, "x");
                    let verbs = available(&card, her(), NOW);
                    let mut sorted = verbs.clone();
                    sorted.sort_unstable();
                    assert_eq!(verbs, sorted, "{state:?} {reviews:?} {owner:?}");
                    assert!(verbs.contains(&WorkVerb::Note), "a ledger note is always a move");
                    let hand = in_hand(&card, her(), NOW);
                    let mut sorted = hand.clone();
                    sorted.sort_unstable();
                    sorted.dedup();
                    assert_eq!(hand, sorted, "{state:?}");
                }
            }
        }
        let commands: std::collections::BTreeSet<_> = WorkVerb::ALL.iter().map(|verb| verb.command()).collect();
        assert_eq!(commands.len(), WorkVerb::ALL.len(), "one command per verb");
        assert!(names(CODE_SUBTREE, "code/edit") && !names(CODE_SUBTREE, "codex/run") && names("work/claim", "work/claim"));
    }
}
