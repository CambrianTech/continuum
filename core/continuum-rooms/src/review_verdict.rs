//! A pass is earned: whether a submission has passed review is DERIVED here from the verdicts the
//! board recorded, never stored, never read from a flag, and never from "some verdict passed".
//!
//! What this ends (cards 0174211f, 5a94e88e; the M5, 2026-11-14). Aiko filed `passed` on her own
//! submission twice (review 13f23b90 on 15722273 at 01:04Z, review 54f644ac on a558207f at 07:04Z;
//! both recorded with reviewer 75ece751, her own publisher id), and both were accepted: a room that
//! declared no review policy admitted the author's own review. Her 07:04Z evidence was a test of a
//! file copied from commit 905cadb while CI on the PR head failed. And the held-work and kanban
//! guidance said "passed review ... merges through the room's merger" whenever ANY recorded verdict
//! passed (`outcomes.contains(&Passed)`), the author's own included and a later send-back ignored.
//!
//! The rules, each one function here and read by every surface:
//! - the author's MIND never reviews her own work unless the room's policy says `self: true`
//!   ([`author_may_review`]); minds are compared through the identity owner ([`MindOf`]), never as
//!   raw ids from two namespaces;
//! - a submission's state is the policy's count of independent passes on it, and a failed verdict
//!   newer than the newest counted pass sends it back ([`derive`]); nothing recorded, nothing
//!   passed;
//! - a pass names the commit it verified, and that commit is the submission's head
//!   ([`verified_head`]).

use airc_core::PeerId;
use airc_work::{GitObjectId, WorkCard, WorkCardId, WorkReviewOutcome, WorkSubmission, WorkSubmissionReview};
use continuum_experience::experience::binding::ReviewPolicy;
use std::collections::BTreeMap;
use uuid::Uuid;

/// The one identity a mind has behind the board ids she can act under. The persona registry is
/// the production owner (a resident's peer id resolves to her persona); a peer no owner knows is
/// its own mind.
pub trait MindOf {
    fn mind_of(&self, peer: PeerId) -> Uuid;
}

/// No identity owner at hand: each peer is its own mind.
pub struct PeerIsMind;

impl MindOf for PeerIsMind {
    fn mind_of(&self, peer: PeerId) -> Uuid {
        peer.as_uuid()
    }
}

/// Whether two board ids are the same mind: the same id, or two ids the identity owner resolves to
/// one mind.
pub fn same_mind(minds: &impl MindOf, a: PeerId, b: PeerId) -> bool {
    a == b || minds.mind_of(a) == minds.mind_of(b)
}

/// The author asked to review her own submission, in a room whose policy does not admit it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorCannotReview {
    /// The card whose submission she published.
    pub card: WorkCardId,
}

impl std::fmt::Display for AuthorCannotReview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "you published the submission on card {}, and this room's review policy does not admit the author's \
             own review: another member reviews it. Next: work/list group=review_requests, and review a \
             submission someone else published.",
            airc_core::shown_id::shown_form(self.card.as_uuid())
        )
    }
}

/// May `reviewer` judge `submission`? Always, unless she is its publisher's mind; then only where
/// the room declares `self: true`. `policy` is `None` when the room declares none or it could not
/// be read: self-review is never admitted on an absence.
pub fn author_may_review(
    policy: Option<&ReviewPolicy>,
    minds: &impl MindOf,
    submission: &WorkSubmission,
    reviewer: PeerId,
) -> Result<(), AuthorCannotReview> {
    match same_mind(minds, submission.publisher, reviewer) && !policy.is_some_and(|p| p.self_review) {
        true => Err(AuthorCannotReview { card: submission.card_id }),
        false => Ok(()),
    }
}

/// The submission `review_card` judges, as the board shows it: its parent's newest. `None` for a
/// card that reviews nothing, or a parent with no submission yet. The claim verb refuses the card
/// to that submission's mind, and the board does not offer it to her.
pub fn judged_submission<'b>(review_card: &WorkCard, cards: &'b [WorkCard]) -> Option<&'b WorkSubmission> {
    let parent = review_card.reviews?;
    cards.iter().find(|card| card.card_id == parent)?.latest_submission()
}

/// What a review card asks of a reviewer on the board now: the submission it judges, unless it was
/// opened for an earlier submission its parent has since replaced (that card's verdict would judge
/// work that no longer lands). On the M5, 2026-11-14, card 4d34acb9 stood with four open review
/// cards, one per resubmission, each offered as claimable work.
pub fn review_request<'b>(review_card: &WorkCard, cards: &'b [WorkCard]) -> Option<&'b WorkSubmission> {
    let parent_id = review_card.reviews?;
    let parent = cards.iter().find(|card| card.card_id == parent_id)?;
    let judged = parent.latest_submission()?;
    let superseded = parent.submissions.iter().any(|earlier| {
        earlier.submission_id != judged.submission_id
            && airc_work::submission_review_card_id(parent_id, earlier.submission_id) == review_card.card_id
    });
    (!superseded).then_some(judged)
}

/// A part a verdict must carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewPart {
    /// A pass names the commit it verified: the submission's head.
    VerifiedHead,
    /// Every verdict says what the reviewer ran and saw.
    Evidence,
}

/// A verdict missing parts it must carry: every missing part named at once, with the head a pass
/// verifies, so one refusal is her whole next call. Before, `work/review` refused one part at a
/// time, the head first and the evidence on the retry, and neither refusal named the head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewIncomplete {
    pub missing: Vec<ReviewPart>,
    /// The submission's head, when it records one: the commit a pass names.
    pub head: Option<GitObjectId>,
}

impl std::fmt::Display for ReviewIncomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self
            .missing
            .iter()
            .map(|part| match (part, &self.head) {
                (ReviewPart::VerifiedHead, Some(head)) => format!(
                    "verified_head={head} (the submission's head: run its checks at that commit, e.g. code/github/pr-checks and its tests)"
                ),
                (ReviewPart::VerifiedHead, None) => "verified_head (the submission's head, from work/submission)".to_string(),
                (ReviewPart::Evidence, _) => "evidence_text (what you ran at that commit and what you saw)".to_string(),
            })
            .collect();
        write!(f, "this review is missing {}. Call work/review again with them; every other field is read off the board", parts.join(" and "))
    }
}

/// Does a verdict carry what it must? `passing` is a pass; `named_head` the commit she named;
/// `evidence_given` whether she said what she ran. A pass on a submission with no recorded head is
/// [`verified_head`]'s refusal ([`PassUnverified::HeadUnrecorded`]), never a missing part here.
pub fn review_complete(passing: bool, named_head: Option<&str>, evidence_given: bool, submission: &WorkSubmission) -> Result<(), ReviewIncomplete> {
    let head = submission.head.commit().cloned();
    let head_missing = passing && head.is_some() && named_head.map(str::trim).is_none_or(str::is_empty);
    let missing: Vec<ReviewPart> = [(head_missing, ReviewPart::VerifiedHead), (!evidence_given, ReviewPart::Evidence)]
        .into_iter()
        .filter_map(|(missing, part)| missing.then_some(part))
        .collect();
    match missing.is_empty() {
        true => Ok(()),
        false => Err(ReviewIncomplete { missing, head }),
    }
}

/// A submission's review state, derived from its recorded verdicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewState {
    /// Fewer counted passes than the policy requires, and no send-back stands.
    Awaiting { passes: u32, required: u32 },
    /// A counted failed verdict is newer than every counted pass: revise and submit again.
    SentBack { by: PeerId },
    /// Passes stand, but the submission records no head, so no pass can be shown to cover the
    /// commit that would land: resubmit at the head.
    HeadUnrecorded,
    /// The policy's count of independent passes, with no newer failure.
    Passed { passes: u32 },
}

impl ReviewState {
    pub fn passed(self) -> bool {
        matches!(self, ReviewState::Passed { .. })
    }
}

/// The state of `submission` from `reviews` (any reviews: those of other submissions or cards are
/// ignored) under `policy`. Per reviewer MIND only her newest decisive verdict counts (an `unknown`
/// decides nothing); the author's are counted only where the policy admits self-review. A failure
/// at or after the newest counted pass sends it back (a tie fails closed). Passed needs at least
/// one counted pass whatever `required` says: an empty review list never reads passed.
pub fn derive<'a>(
    submission: &WorkSubmission,
    reviews: impl IntoIterator<Item = &'a WorkSubmissionReview>,
    policy: &ReviewPolicy,
    minds: &impl MindOf,
) -> ReviewState {
    let mut newest: BTreeMap<Uuid, &WorkSubmissionReview> = BTreeMap::new();
    for review in reviews.into_iter().filter(|r| {
        r.submission_id == submission.submission_id
            && r.card_id == submission.card_id
            && r.outcome != WorkReviewOutcome::Unknown
            && (policy.self_review || !same_mind(minds, submission.publisher, r.reviewer))
    }) {
        let slot = newest.entry(minds.mind_of(review.reviewer)).or_insert(review);
        if (review.reviewed_at_ms, review.review_id) > (slot.reviewed_at_ms, slot.review_id) {
            *slot = review;
        }
    }
    let at = |outcome| newest.values().filter(move |r| r.outcome == outcome);
    let newest_pass = at(WorkReviewOutcome::Passed).map(|r| r.reviewed_at_ms).max();
    let standing_fail = at(WorkReviewOutcome::Failed)
        .filter(|fail| newest_pass.is_none_or(|pass| fail.reviewed_at_ms >= pass))
        .max_by_key(|fail| (fail.reviewed_at_ms, fail.review_id));
    if let Some(fail) = standing_fail {
        return ReviewState::SentBack { by: fail.reviewer };
    }
    let passes = at(WorkReviewOutcome::Passed).fold(0u32, |n, _| n.saturating_add(1));
    let required = policy.required;
    match (passes > 0 && passes >= required, submission.head.commit()) {
        (false, _) => ReviewState::Awaiting { passes, required },
        (true, None) => ReviewState::HeadUnrecorded,
        (true, Some(_)) => ReviewState::Passed { passes },
    }
}

/// Why a pass does not carry completed verification of the submission it judges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassUnverified {
    /// A pass names the commit it verified (`verified_head`); none was named.
    NoHeadNamed,
    /// The named value is not a commit id of at least [`SHORT_COMMIT`] hex digits.
    NotACommit(String),
    /// The submission records no head, so nothing a reviewer ran can be shown to be it.
    HeadUnrecorded,
    /// The pass verified a different commit than the one submitted.
    OtherCommit { named: String, submitted: GitObjectId },
}

/// git's default abbreviated object name (`core.abbrev`'s fixed floor): the shortest form a commit
/// is shown in, so the shortest a reviewer can name.
pub const SHORT_COMMIT: usize = 7;

impl std::fmt::Display for PassUnverified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoHeadNamed => f.write_str(
                "a pass names the commit you verified: pass verified_head (the submission's head, from work/submission). \
                 Run its tests at that commit; a test of another commit or a copied file is not a verification of it",
            ),
            Self::NotACommit(named) => write!(f, "verified_head '{named}' is not a commit id ({SHORT_COMMIT}+ hex digits)"),
            Self::HeadUnrecorded => f.write_str(
                "this submission records no head commit, so no pass can be shown to cover what would land: \
                 send it back (outcome failed) asking the author to submit again at the pull request's head",
            ),
            Self::OtherCommit { named, submitted } => write!(
                f,
                "you verified {named}, but the submission's head is {submitted}: check out {submitted}, run the checks \
                 there, and pass with verified_head={submitted}, or send it back"
            ),
        }
    }
}

/// The head a pass verified, when the commit she named is `submission`'s head (its full id or an
/// abbreviation of at least [`SHORT_COMMIT`] digits).
pub fn verified_head<'s>(submission: &'s WorkSubmission, named: Option<&str>) -> Result<&'s GitObjectId, PassUnverified> {
    // A submission with no recorded head can never pass, so that is said first: asking her to name a
    // head that does not exist sent the probe of 2026-11-14 (card 9f883920, submission 567303b1)
    // looking for one.
    let submitted = submission.head.commit().ok_or(PassUnverified::HeadUnrecorded)?;
    let named = named.map(str::trim).filter(|n| !n.is_empty()).ok_or(PassUnverified::NoHeadNamed)?;
    if named.len() < SHORT_COMMIT || !named.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(PassUnverified::NotACommit(named.to_string()));
    }
    match submitted.as_str().starts_with(&named.to_ascii_lowercase()) {
        true => Ok(submitted),
        false => Err(PassUnverified::OtherCommit { named: named.to_string(), submitted: submitted.clone() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work_preconditions::{fixture_card, fixture_submission};
    use airc_work::CardState;

    const HEAD: &str = "6345c3a7d925af168d0c3a88a7443cf0af9dd2a4";

    fn peer(n: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(n))
    }

    /// Aiko: her persona's peer publishes; an alias (another id of the same mind) may review.
    const AUTHOR: u128 = 0x75ec;
    const AUTHOR_ALIAS: u128 = 0xe2f0;
    const PEER_A: u128 = 0xa;
    const PEER_B: u128 = 0xb;

    /// The identity owner for the tests: the alias resolves to the author's mind.
    struct Minds;
    impl MindOf for Minds {
        fn mind_of(&self, p: PeerId) -> Uuid {
            match p.as_uuid().as_u128() {
                AUTHOR_ALIAS => Uuid::from_u128(AUTHOR),
                other => Uuid::from_u128(other),
            }
        }
    }

    fn submission() -> WorkSubmission {
        let card = fixture_card(CardState::Review, Some(peer(AUTHOR)), None, "work");
        WorkSubmission { head: airc_work::SubmissionHead::At(GitObjectId::new(HEAD).expect("hex")), ..fixture_submission(&card) } // expect: a literal hex id
    }

    fn verdict(sub: &WorkSubmission, by: u128, outcome: WorkReviewOutcome, at: u64) -> WorkSubmissionReview {
        WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::from_u128((u128::from(at) << 8) | by),
            card_id: sub.card_id,
            submission_id: sub.submission_id,
            artifact: sub.artifact.clone(),
            review_card_id: WorkCardId::from_u128(0x5e),
            review_claim_id: airc_work::ClaimId::from_u128(0x5c),
            reviewer: peer(by),
            outcome,
            evidence: sub.artifact.clone(),
            evidence_text: None,
            reviewed_at_ms: at,
        }
    }

    // what this catches: a "passed" read off anything but the policy's count of independent passes
    // (cards 0174211f, 5a94e88e): an empty list, the author's own pass under either of her ids
    // (Aiko 01:04Z and 07:04Z), a pass a later independent failure reopened, and a reviewer's own
    // newer verdict replacing her older one. Table-driven over the one derivation.
    #[test]
    fn a_pass_is_derived_from_independent_recorded_verdicts_only() {
        use WorkReviewOutcome::{Failed, Passed, Unknown};
        let sub = submission();
        let one = ReviewPolicy::default();
        let two = ReviewPolicy { required: 2, ..ReviewPolicy::default() };
        let zero = ReviewPolicy { required: 0, ..ReviewPolicy::default() };
        let selfish = ReviewPolicy { self_review: true, ..ReviewPolicy::default() };
        let cases: &[(&str, &ReviewPolicy, &[(u128, WorkReviewOutcome, u64)], ReviewState)] = &[
            ("nothing recorded is never passed", &one, &[], ReviewState::Awaiting { passes: 0, required: 1 }),
            ("required: 0 still needs one pass", &zero, &[], ReviewState::Awaiting { passes: 0, required: 0 }),
            ("the author's own pass (01:04Z shape)", &one, &[(AUTHOR, Passed, 1)], ReviewState::Awaiting { passes: 0, required: 1 }),
            ("the author's pass under another id (07:04Z ledger shape)", &one, &[(AUTHOR_ALIAS, Passed, 1)], ReviewState::Awaiting { passes: 0, required: 1 }),
            ("self-review where the room allows it", &selfish, &[(AUTHOR_ALIAS, Passed, 1)], ReviewState::Passed { passes: 1 }),
            ("one independent pass", &one, &[(PEER_A, Passed, 1)], ReviewState::Passed { passes: 1 }),
            ("the author's pass then an independent failure", &one, &[(AUTHOR, Passed, 1), (PEER_A, Failed, 2)], ReviewState::SentBack { by: peer(PEER_A) }),
            ("a later independent failure reopens a pass", &one, &[(PEER_A, Passed, 1), (PEER_B, Failed, 2)], ReviewState::SentBack { by: peer(PEER_B) }),
            ("a failure older than the newest pass is answered", &one, &[(PEER_B, Failed, 1), (PEER_A, Passed, 2)], ReviewState::Passed { passes: 1 }),
            ("a tie fails closed", &one, &[(PEER_A, Passed, 2), (PEER_B, Failed, 2)], ReviewState::SentBack { by: peer(PEER_B) }),
            ("her newer pass replaces her own failure", &one, &[(PEER_A, Failed, 1), (PEER_A, Passed, 2)], ReviewState::Passed { passes: 1 }),
            ("her newer failure withdraws her own pass", &one, &[(PEER_A, Passed, 1), (PEER_A, Failed, 2)], ReviewState::SentBack { by: peer(PEER_A) }),
            ("unknown decides nothing", &one, &[(PEER_A, Passed, 1), (PEER_A, Unknown, 2)], ReviewState::Passed { passes: 1 }),
            ("one reviewer twice is one pass", &two, &[(PEER_A, Passed, 1), (PEER_A, Passed, 2)], ReviewState::Awaiting { passes: 1, required: 2 }),
            ("two reviewers meet required: 2", &two, &[(PEER_A, Passed, 1), (PEER_B, Passed, 2)], ReviewState::Passed { passes: 2 }),
        ];
        for (name, policy, rows, want) in cases {
            let reviews: Vec<_> = rows.iter().map(|(by, outcome, at)| verdict(&sub, *by, *outcome, *at)).collect();
            assert_eq!(derive(&sub, &reviews, policy, &Minds), *want, "{name}");
        }
        // Verdicts on another submission (an earlier head) say nothing about this one.
        let older = WorkSubmission { submission_id: airc_work::SubmissionId::from_u128(8), ..sub.clone() };
        let elsewhere = [verdict(&older, PEER_A, Passed, 1)];
        assert_eq!(derive(&sub, &elsewhere, &one, &Minds), ReviewState::Awaiting { passes: 0, required: 1 });
        // Passes on a submission that recorded no head cannot read passed.
        let headless = WorkSubmission { head: airc_work::SubmissionHead::Unrecorded, ..sub.clone() };
        let passed = [verdict(&headless, PEER_A, Passed, 1)];
        assert_eq!(derive(&headless, &passed, &one, &Minds), ReviewState::HeadUnrecorded);
    }

    // what this catches: the author reviewing her own work (cards 0174211f): refused under the id
    // she published with (01:04Z, reviewer 75ece751) AND under another id of the same mind (the
    // 07:04Z ledger's e2f0e022 shape), by default and on an unreadable policy; admitted only where
    // the room declares self: true. Another mind is never refused.
    #[test]
    fn the_author_mind_cannot_review_her_own_submission_under_any_id() {
        let sub = submission();
        let refused = Err(AuthorCannotReview { card: sub.card_id });
        assert_eq!(author_may_review(None, &Minds, &sub, peer(AUTHOR)), refused, "the same id");
        assert_eq!(author_may_review(None, &Minds, &sub, peer(AUTHOR_ALIAS)), refused, "another id, one mind");
        assert_eq!(author_may_review(Some(&ReviewPolicy::default()), &Minds, &sub, peer(AUTHOR_ALIAS)), refused, "declared, self false");
        let selfish = ReviewPolicy { self_review: true, ..ReviewPolicy::default() };
        assert_eq!(author_may_review(Some(&selfish), &Minds, &sub, peer(AUTHOR_ALIAS)), Ok(()), "the room allows it");
        assert_eq!(author_may_review(None, &Minds, &sub, peer(PEER_A)), Ok(()), "another mind");
        assert_eq!(author_may_review(None, &PeerIsMind, &sub, peer(AUTHOR_ALIAS)), Ok(()), "without an identity owner, ids are minds");
        assert!(AuthorCannotReview { card: sub.card_id }.to_string().contains("work/list"), "the refusal names her next move");
    }

    // what this catches: a review card offered to, and claimable by, the mind whose submission it
    // judges (card 0174211f): her review card is found by its parent's newest submission.
    #[test]
    fn a_review_card_knows_whose_work_it_judges() {
        let mut parent = fixture_card(CardState::Review, Some(peer(AUTHOR)), None, "work");
        let review = WorkCard { card_id: WorkCardId::from_u128(0x5e), ..fixture_card(CardState::Open, None, Some(parent.card_id), "review") };
        assert_eq!(judged_submission(&review, std::slice::from_ref(&parent)), None, "nothing submitted yet");
        parent.submissions.push(fixture_submission(&parent));
        let cards = [parent.clone(), review.clone()];
        let judged = judged_submission(&review, &cards);
        assert_eq!(judged.map(|s| s.publisher), Some(peer(AUTHOR)));
        assert_eq!(judged_submission(&parent, &cards), None, "a code card reviews nothing");
        let refused = judged.map(|s| author_may_review(None, &Minds, s, peer(AUTHOR_ALIAS)).is_err());
        assert_eq!(refused, Some(true), "so her alias may not claim it");
    }

    // what this catches: a pass whose evidence verified another commit (Aiko 07:04Z: a copy from
    // 905cadb while the PR head was 773c650c8). The named commit must be the submission's head.
    #[test]
    fn a_pass_verifies_the_submitted_head_and_no_other_commit() {
        let sub = submission();
        assert_eq!(verified_head(&sub, Some(HEAD)).map(GitObjectId::as_str), Ok(HEAD));
        assert_eq!(verified_head(&sub, Some("6345C3A")).map(GitObjectId::as_str), Ok(HEAD), "an abbreviation, any case");
        assert_eq!(verified_head(&sub, None), Err(PassUnverified::NoHeadNamed));
        assert_eq!(verified_head(&sub, Some("  ")), Err(PassUnverified::NoHeadNamed));
        assert_eq!(verified_head(&sub, Some("6345c3")), Err(PassUnverified::NotACommit("6345c3".into())), "shorter than git shows");
        assert!(matches!(verified_head(&sub, Some("905cadb")), Err(PassUnverified::OtherCommit { .. })));
        let headless = WorkSubmission { head: airc_work::SubmissionHead::Unrecorded, ..sub };
        assert_eq!(verified_head(&headless, Some(HEAD)), Err(PassUnverified::HeadUnrecorded));
        assert_eq!(verified_head(&headless, None), Err(PassUnverified::HeadUnrecorded), "never asked for a head that does not exist");
    }

    // what this catches: a reviewer refused one missing part per call with no head named (mind
    // 7fecdd21 on the academy board, 2026-11-14: work/claim then work/review outcome=passed with no
    // head and no evidence, refused in a loop). One refusal names every missing part and the head.
    #[test]
    fn an_incomplete_review_names_every_missing_part_and_the_head_at_once() {
        let sub = submission();
        let refused = review_complete(true, None, false, &sub).expect_err("a bare pass is incomplete"); // expect_err: the case under test
        assert_eq!(refused.missing, [ReviewPart::VerifiedHead, ReviewPart::Evidence]);
        let said = refused.to_string();
        assert!(said.contains(&format!("verified_head={HEAD}")) && said.contains("evidence_text"), "{said}");
        assert_eq!(review_complete(false, None, false, &sub).map_err(|r| r.missing), Err(vec![ReviewPart::Evidence]), "a send-back names no head");
        assert_eq!(review_complete(true, Some(" "), true, &sub).map_err(|r| r.missing), Err(vec![ReviewPart::VerifiedHead]));
        assert_eq!(review_complete(true, Some(HEAD), true, &sub), Ok(()));
        let headless = WorkSubmission { head: airc_work::SubmissionHead::Unrecorded, ..sub };
        assert_eq!(review_complete(true, None, true, &headless), Ok(()), "no recorded head is verified_head's refusal, not a missing part");
    }

    // what this catches: a review card opened for a submission its parent has since replaced,
    // offered as a review request (card 4d34acb9 on the M5, 2026-11-14: four open review cards).
    #[test]
    fn only_the_review_card_of_the_newest_submission_is_a_request() {
        let mut parent = fixture_card(CardState::Review, Some(peer(AUTHOR)), None, "work");
        let first = fixture_submission(&parent);
        let newer = WorkSubmission { submission_id: airc_work::SubmissionId::from_u128(10), ..fixture_submission(&parent) };
        let (first_id, newer_id) = (first.submission_id, newer.submission_id);
        parent.submissions = vec![newer, first]; // newest first, as airc-work keeps them
        let card_for = |judged: airc_work::SubmissionId| WorkCard {
            card_id: airc_work::submission_review_card_id(parent.card_id, judged),
            ..fixture_card(CardState::Open, None, Some(parent.card_id), "review")
        };
        let (stale, current) = (card_for(first_id), card_for(newer_id));
        let cards = [parent, stale, current];
        let (parent, stale, current) = (&cards[0], &cards[1], &cards[2]);
        assert_eq!(review_request(current, &cards).map(|s| s.submission_id), Some(newer_id));
        assert_eq!(review_request(stale, &cards), None, "superseded by the resubmission");
        assert_eq!(review_request(parent, &cards), None, "a code card requests no review");
    }
}
