//! Her held work, read from her own board once per tick (STATE-DRIVEN-SURFACE.md): the cards she
//! holds with what each one's preconditions put in her hands (`work_preconditions`) and the state
//! entry each belongs to (its occurrence's revision), the claims that ended since her
//! last good read, and her live rejected-claim facts. Cognition's cycle reads it through the
//! `held_work` extension slot and carries it on the workspace, where her tool surface and her
//! held-work item (`context_streams::held_work`) both read it. This replaces `ActiveWorkSource`,
//! a RagSource that rendered her claims as one grounding blob and whose text the deliberation
//! faculty grepped for `[InProgress]`.
//!
//! The airc access lives behind [`AircWorkReader`] (a supertrait of `AircCitizen`, same shape as
//! `AircRosterReader`/`AircDoctrineReader`): the real runtime calls `Airc::work_roster_status`;
//! tests use a stub. Cross-activity by construction: the roster spans ALL rooms, so she sees her
//! work whichever room this turn is in.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use airc_lib::{AircError, WorkCard, WorkCardId};
use async_trait::async_trait;
use uuid::Uuid;

use crate::cognition::work_preconditions::Held;
use crate::cognition::workspace::{AwaitingResubmission, ChangedFile, ChangesUnread, EndedClaim, FileChange, HeldWork, SubmissionChanges, SubmissionVerdict};

/// Reads THIS persona's currently-claimed work cards from airc. A supertrait of
/// `AircCitizen` (like `AircRosterReader`): the persona's runtime implements it
/// against its own airc handle, so a claim is read as the persona itself.
#[async_trait]
pub trait AircWorkReader: Send + Sync {
    /// The cards this persona currently owns/claims, across all rooms. Empty when
    /// it holds none (or no daemon, in tests).
    async fn active_claims(&self) -> Result<Vec<WorkCard>, AircError>;

    /// ONE card as the board carries it now, whoever holds it — the read a wake needs
    /// to tell a citizen what became of a card she believed she held (card c8303c32).
    /// `None` when no subscribed board has it (or no daemon, in tests). Default `None`
    /// so a reader that only knows its own claims still compiles; production answers.
    async fn card(&self, _card_id: uuid::Uuid) -> Option<WorkCard> {
        None
    }

    /// Every board she can see, one per subscribed room: where her send-backs and the review cards
    /// of their resubmissions live (card 7f8c248e). Required: a reader that cannot answer says so,
    /// so no reader shows her nothing by default.
    async fn boards(&self) -> Result<Vec<std::sync::Arc<airc_work::BoardSnapshot>>, AircError>;
}

/// A claim as her last successful read saw it: what it is called, its state, and the board
/// revision at the first read that saw it in that state (its state entry).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    title: String,
    state: airc_work::CardState,
    entered_ms: u64,
}

/// Each resident's claims at her last successful read: the baseline an ended claim and a state
/// entry are read against. One entry per resident, replaced on every good read (bounded by the
/// population).
static BASELINES: OnceLock<Mutex<HashMap<Uuid, HashMap<WorkCardId, Seen>>>> = OnceLock::new();

/// Her held work now, read through her airc citizen; `None` when she has no citizen on this node
/// or the read failed (a degraded read is an absence, never an empty board).
pub async fn read(persona: Uuid) -> Option<HeldWork> {
    let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
    let holder = runtime.airc().peer_id();
    read_through(runtime.as_ref(), persona, holder).await
}

/// [`read`] through `reader`, her citizen's board access: the one path, a test's stub included.
pub(crate) async fn read_through(reader: &(dyn AircWorkReader + '_), persona: Uuid, holder: airc_core::PeerId) -> Option<HeldWork> {
    let claims = reader.active_claims().await;
    let boards = match reader.boards().await {
        Ok(boards) => boards,
        Err(error) => {
            continuum_base::probe!(class = "persona.held_work.boards_unread", error = %error,
                "her boards could not be read: her send-backs are not shown this read");
            Vec::new()
        }
    };
    // The baseline lock is released before any await below (the git reads).
    let mut held = {
        let baselines = BASELINES.get_or_init(Default::default);
        let mut baselines = baselines.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a poisoned baseline map still holds the last good reads
        let baseline = baselines.entry(persona).or_default();
        let mut seen = (!baseline.is_empty()).then(|| std::mem::take(baseline));
        let at_ms = continuum_base::time::EpochMs::now().millis(); // the read's own time: what her moves are decided at
        let held = observe(&mut seen, holder, at_ms, claims, crate::persona::claim_rejections::recent(persona));
        *baseline = seen.unwrap_or_default(); // unwrap_or_default: no good read yet leaves no baseline
        held
    };
    if let Some(held) = held.as_mut() {
        held.awaiting = awaiting(&boards, holder);
        held.submissions = submissions(persona, &held.holds, &boards).await;
    }
    held
}

/// A submission's diff never changes, so each resident keeps the ones her current submissions read:
/// keyed by (checkout, base, head), replaced on every read with only the keys still in use, so it
/// is bounded by what she holds (computed once, phased out by usage). Only a read that succeeded is
/// kept; a failure (commits not fetched yet) is tried again on her next read.
type DiffKey = (std::path::PathBuf, String, String);
static DIFFS: OnceLock<Mutex<HashMap<Uuid, HashMap<DiffKey, Vec<ChangedFile>>>>> = OnceLock::new();

/// Her latest submission on each of her own cards, with the files it changes against its recorded
/// base (read in the card's checkout) and the newest verdict on it (card 1e9774cc).
async fn submissions(persona: Uuid, holds: &[Held], boards: &[std::sync::Arc<airc_work::BoardSnapshot>]) -> Vec<SubmissionChanges> {
    let known: HashMap<DiffKey, Vec<ChangedFile>> = {
        let diffs = DIFFS.get_or_init(Default::default);
        let mut diffs = diffs.lock().unwrap_or_else(|poisoned| poisoned.into_inner()); // unwrap_or_else: a poisoned cache still holds diffs that never change
        diffs.remove(&persona).unwrap_or_default() // unwrap_or_default: her first read has none kept
    };
    let mut kept: HashMap<DiffKey, Vec<ChangedFile>> = HashMap::new();
    let mut out = Vec::new();
    for held in holds {
        if !matches!(held.role, crate::cognition::work_preconditions::CardRole::Code) {
            continue; // a review card's submission is someone else's: work/submission reads it
        }
        let Some(latest) = held.card.latest_submission() else { continue };
        let airc_work::SubmissionHead::At(head) = &latest.head else { continue };
        let base = latest.base_sha.to_string();
        let head = head.to_string();
        let files = match crate::persona::card_checkout::checkout_path_for(&persona, &held.card) {
            Some(checkout) => {
                let key: DiffKey = (checkout.clone(), base.clone(), head.clone());
                let read = match known.get(&key) {
                    Some(files) => Ok(files.clone()),
                    None => changed_files(&checkout, &base, &head).await,
                };
                if let Ok(files) = &read {
                    kept.insert(key, files.clone());
                }
                read
            }
            None => Err(ChangesUnread::NoCheckout),
        };
        out.push(SubmissionChanges {
            card: held.card.card_id,
            title: held.card.title.clone(),
            submission: latest.submission_id,
            base,
            head,
            files,
            verdict: verdict(boards, held.card.card_id, latest.submission_id),
        });
    }
    let diffs = DIFFS.get_or_init(Default::default);
    diffs.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(persona, kept); // unwrap_or_else: as above
    out
}

/// What `head` changes against `base` in `checkout`: two reads of one diff (status and counts),
/// both NUL-separated so any path reads exactly.
async fn changed_files(checkout: &std::path::Path, base: &str, head: &str) -> Result<Vec<ChangedFile>, ChangesUnread> {
    let diff = |args: &'static str| async move {
        crate::code::git_bridge::run_git(checkout, &["diff", args, "-z", "-M", base, head]).await
    };
    let (status, counts) = match (diff("--name-status").await, diff("--numstat").await) {
        (Ok(status), Ok(counts)) => (status, counts),
        (Err(detail), _) | (_, Err(detail)) => {
            // Only on failure: is it the commits that are missing (she can fetch them), or git itself?
            let has = |sha: &str| {
                let spec = format!("{sha}^{{commit}}");
                async move { crate::code::git_bridge::run_git(checkout, &["cat-file", "-e", &spec]).await.is_ok() }
            };
            return Err(if has(base).await && has(head).await {
                ChangesUnread::GitRefused { detail }
            } else {
                ChangesUnread::CommitsAbsent { base: base.to_owned(), head: head.to_owned() }
            });
        }
    };
    parse_changes(&status, &counts).map_err(|detail| ChangesUnread::Unparsable { detail })
}

/// PURE: `git diff --name-status -z -M` and `--numstat -z -M` of one diff, in git's one order,
/// into changed files. A rename names both paths in each; a binary file has `-` counts.
fn parse_changes(status: &str, counts: &str) -> Result<Vec<ChangedFile>, String> {
    let mut status = status.split('\0').filter(|field| !field.is_empty());
    let mut changes = Vec::new();
    while let Some(code) = status.next() {
        let path = status.next().ok_or_else(|| format!("git name-status ended after '{code}'"))?;
        let change = match code.chars().next() {
            Some('A') => (FileChange::Added, path.to_owned()),
            Some('D') => (FileChange::Deleted, path.to_owned()),
            Some(kind @ ('R' | 'C')) => {
                let to = status.next().ok_or_else(|| format!("git name-status {kind} of '{path}' has no new path"))?;
                let from = path.to_owned();
                (if kind == 'R' { FileChange::Renamed { from } } else { FileChange::Copied { from } }, to.to_owned())
            }
            _ => (FileChange::Modified, path.to_owned()),
        };
        changes.push(change);
    }
    let mut counts = counts.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::with_capacity(changes.len());
    for (change, path) in changes {
        let line = counts.next().ok_or_else(|| format!("git numstat has no counts for '{path}'"))?;
        let mut fields = line.splitn(3, '\t');
        // Git writes '-' for a binary file's counts; anything else that is not a number is git
        // answering in a shape this reader does not know, never "binary".
        let count = |field: Option<&str>| -> Result<Option<u64>, String> {
            match field {
                Some("-") => Ok(None),
                Some(n) => n.parse().map(Some).map_err(|_| format!("git numstat count '{n}' for '{path}' is not a number")),
                None => Err(format!("git numstat has no count for '{path}'")),
            }
        };
        let added = count(fields.next())?;
        let deleted = count(fields.next())?;
        if matches!(change, FileChange::Renamed { .. } | FileChange::Copied { .. }) {
            // numstat -z writes a rename as "a\td\t" then the old and new paths as their own fields
            counts.next();
            counts.next();
        }
        files.push(ChangedFile { path, change, added, deleted });
    }
    Ok(files)
}

/// PURE: the newest verdict anyone gave on `submission` of `card`, with the reviewer's words.
fn verdict(boards: &[std::sync::Arc<airc_work::BoardSnapshot>], card: WorkCardId, submission: airc_work::SubmissionId) -> Option<SubmissionVerdict> {
    boards
        .iter()
        .flat_map(|board| board.submission_reviews.iter())
        .filter(|review| review.card_id == card && review.submission_id == submission)
        .max_by_key(|review| review.reviewed_at_ms)
        .map(|review| SubmissionVerdict { reviewer: review.reviewer, outcome: review.outcome, evidence: review.evidence_text.clone() })
}

/// PURE: her open send-backs on `boards` (card 7f8c248e). A card counts when her NEWEST verdict on
/// it failed and it is not terminal. It waits while its latest submission is the one she judged;
/// once the author resubmits, it carries the review card the board opened for that exact
/// submission (`submission_review_card_id`), but only while that card is open and nobody else's.
/// Resolved, closed, merged, superseded by a later verdict of hers, or taken by another reviewer:
/// it leaves her view. Rebuilt from the board on every read, so a restart loses nothing.
fn awaiting(boards: &[std::sync::Arc<airc_work::BoardSnapshot>], me: airc_core::PeerId) -> Vec<AwaitingResubmission> {
    use airc_work::{CardState, WorkReviewOutcome};
    let terminal = |state: CardState| matches!(state, CardState::Merged | CardState::Closed);
    let mut awaiting: Vec<AwaitingResubmission> = Vec::new();
    for board in boards {
        for card in board.cards.iter().filter(|card| card.reviews.is_none() && !terminal(card.state)) {
            let newest = board.submission_reviews.iter().filter(|review| review.card_id == card.card_id && review.reviewer == me).max_by_key(|review| review.reviewed_at_ms);
            let Some(mine) = newest.filter(|review| review.outcome == WorkReviewOutcome::Failed) else {
                continue; // no verdict of hers, or her newest one is not a send-back
            };
            let Some(latest) = card.latest_submission() else {
                continue;
            };
            let resubmitted = if latest.submission_id == mine.submission_id {
                None
            } else {
                let review = airc_work::submission_review_card_id(card.card_id, latest.submission_id);
                let open = board.cards.iter().find(|candidate| candidate.card_id == review);
                match open {
                    Some(open) if !terminal(open.state) && open.owner.is_none_or(|owner| owner == me) => Some(review),
                    _ => continue, // decided, taken by another reviewer, or its review card not on the board yet
                }
            };
            if !awaiting.iter().any(|known| known.card == card.card_id) {
                awaiting.push(AwaitingResubmission { card: card.card_id, title: card.title.clone(), reviewed: mine.submission_id, resubmitted });
            }
        }
    }
    awaiting.sort_unstable_by_key(|item| item.card); // reproducible order
    awaiting
}

/// PURE but for the baseline it advances: her held work from one read of her claims. A failed
/// read returns `None` and leaves the baseline untouched, so it never fabricates "you lost every
/// claim"; the first good read has no baseline and reports nothing ended; every later one reports
/// each claim gone since the last, once.
fn observe(
    baseline: &mut Option<HashMap<WorkCardId, Seen>>,
    holder: airc_core::PeerId,
    at_ms: u64,
    claims: Result<Vec<WorkCard>, AircError>,
    rejections: Vec<String>,
) -> Option<HeldWork> {
    let cards = match claims {
        Ok(cards) => cards,
        Err(error) => {
            continuum_base::probe!(
                class = "persona.held_work.read_failed",
                error = %error,
                "her claims could not be read: her turn carries no held work, and no claim reads as ended"
            );
            return None;
        }
    };
    // A card still in the state the last read saw keeps that entry; a new state (or a card new to
    // her) enters at the board's revision now. A heartbeat moves the revision, never the entry.
    let now: HashMap<WorkCardId, Seen> = cards
        .iter()
        .map(|card| {
            let kept = baseline.as_ref().and_then(|before| before.get(&card.card_id)).filter(|seen| seen.state == card.state);
            let entered_ms = kept.map_or(card.updated_at_ms, |seen| seen.entered_ms); // map_or: a state she has not seen it in enters now
            (card.card_id, Seen { title: card.title.clone(), state: card.state, entered_ms })
        })
        .collect();
    let mut ended: Vec<EndedClaim> = match baseline.as_ref() {
        Some(before) => before
            .iter()
            .filter(|(card, _)| !now.contains_key(*card))
            .map(|(card, seen)| EndedClaim { card: *card, title: seen.title.clone() })
            .collect(),
        None => Vec::new(),
    };
    ended.sort_unstable_by_key(|ended| ended.card); // reproducible order
    // Which claim her work turns are bound to, by the one selector hands use.
    let selected = crate::persona::work_focus::focus_workspace_card(cards.iter()).map(|card| card.card_id);
    let holds = cards
        .into_iter()
        .filter_map(|card| {
            let entered_ms = now.get(&card.card_id).map_or(card.updated_at_ms, |seen| seen.entered_ms); // map_or: every card read is in `now`
            Held::of(card, holder, at_ms, entered_ms)
        })
        .collect();
    *baseline = Some(now);
    Some(HeldWork { holds, selected, ended, rejections, awaiting: Vec::new(), submissions: Vec::new() }) // her send-backs and submissions are read from the boards by `read`
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_work::CardState;

    fn her() -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(Uuid::from_u128(1))
    }

    fn card(id: u128, title: &str) -> WorkCard {
        let mut card = crate::cognition::work_preconditions::fixture_card(CardState::Claimed, Some(her()), None, title);
        card.card_id = WorkCardId::from_u128(id);
        card
    }

    const AT: u64 = crate::cognition::work_preconditions::FIXTURE_NOW_MS;

    fn degraded() -> Result<Vec<WorkCard>, AircError> {
        Err(AircError::UnknownPeer(airc_core::PeerId::new()))
    }

    // what this catches (#156, the Benchy silent-handoff case): a card that VANISHES between
    // reads (lease expired, re-claimed by another peer) is an ended claim exactly ONCE, even when
    // it was her LAST card; the next read carries none (a transition, not a nag loop); the first
    // read of a fresh mind reports none; a degraded read never fabricates one and keeps the
    // baseline for the read that really shows the card gone.
    #[test]
    fn an_ended_claim_is_reported_once_and_a_degraded_read_never_fabricates_one() {
        let mut baseline = None;
        let first = observe(&mut baseline, her(), AT, Ok(vec![card(1, "Millbrook Bakery")]), Vec::new()).expect("a good read");
        assert!(first.ended.is_empty(), "the first read has no baseline to lose from");
        assert_eq!(first.holds.len(), 1);
        assert!(observe(&mut baseline, her(), AT, degraded(), Vec::new()).is_none(), "a degraded read is an absence");
        let lost = observe(&mut baseline, her(), AT, Ok(vec![]), Vec::new()).expect("a good read");
        assert_eq!(lost.ended, vec![EndedClaim { card: WorkCardId::from_u128(1), title: "Millbrook Bakery".into() }]);
        assert!(lost.holds.is_empty());
        let after = observe(&mut baseline, her(), AT, Ok(vec![]), Vec::new()).expect("a good read");
        assert!(after.ended.is_empty(), "the baseline adopted: silence, not a nag loop");
    }

    // what this catches: two claims rendering as two "selected" cards, or a rejection fact dropped
    // when she holds nothing (the rejection-amnesia case, glass-boxed 2026-08-02: she reported the
    // rejection for three turns, then "I've claimed the task" once the receipts scrolled).
    #[test]
    fn one_claim_is_selected_and_a_rejection_rides_with_no_claims() {
        let mut baseline = None;
        let mut working = card(1, "Source-hygiene ratchet");
        working.state = CardState::InProgress;
        let mut review = card(2, "django__django-16136");
        review.state = CardState::Review;
        let both = observe(&mut baseline, her(), AT, Ok(vec![review, working]), Vec::new()).expect("a good read");
        assert_eq!(both.selected, Some(WorkCardId::from_u128(1)), "the actionable card is the selected one");
        let rejected = observe(&mut None, her(), AT, Ok(vec![]), vec!["[work] Your claim on card 44ebaa41 was REJECTED".into()]).expect("a good read");
        assert_eq!(rejected.rejections.len(), 1);
        assert!(rejected.holds.is_empty());
    }

    // what this catches (Codex on the occurrence contract): a state entry that moves with every
    // heartbeat (her card re-rendered as new every lease renewal) or that is reused when the card
    // re-enters a state (Review, InProgress, Review again resolving to the first Review). The entry
    // holds through heartbeats and is the board's revision at each new entry.
    #[test]
    fn a_state_entry_holds_through_heartbeats_and_is_new_on_reentry() {
        fn entered(baseline: &mut Option<HashMap<WorkCardId, Seen>>, card: &WorkCard) -> u64 {
            let held = observe(baseline, her(), AT, Ok(vec![card.clone()]), Vec::new()).expect("a good read"); // expect: Ok in
            held.holds.first().map_or(0, |held| held.entered_ms) // map_or: the fixture card is hers, so it is held
        }
        let mut baseline = None;
        let mut c = card(1, "x");
        c.state = CardState::Review;
        c.updated_at_ms = 10;
        assert_eq!(entered(&mut baseline, &c), 10);
        c.updated_at_ms = 15; // a heartbeat
        assert_eq!(entered(&mut baseline, &c), 10, "a heartbeat is not a new entry");
        c.state = CardState::InProgress;
        c.updated_at_ms = 20;
        assert_eq!(entered(&mut baseline, &c), 20);
        c.state = CardState::Review;
        c.updated_at_ms = 30;
        assert_eq!(entered(&mut baseline, &c), 30, "Review again is a new entry");
    }

    // what this catches (card 7f8c248e; Kimi on BigMama 2026-11-10 made placeholder cards to wait for
    // Sahar's resubmission): her send-back must read as a wait while the author has not resubmitted;
    // once the author has, as the review card the board opened for that exact submission, offered
    // to her first; and never when another reviewer holds that card, when the parent is terminal, or
    // when her newest verdict on it passed.
    #[test]
    fn her_send_backs_wait_for_the_exact_resubmission_and_yield_to_other_reviewers() {
        use airc_work::{BoardSnapshot, CardState, SubmissionId, WorkCardId, WorkReviewOutcome};
        let me = her();
        let artifact: airc_work::SubmissionArtifact = serde_json::from_value(serde_json::json!({"hash": "a".repeat(64), "size_bytes": 5, "mime": "text/x-diff"}))
            .expect("test: an artifact");
        let author = airc_core::PeerId::new();
        let parent = |n: u128, state: CardState| {
            let mut card = crate::cognition::work_preconditions::fixture_card(state, Some(author), None, "Sahar's resubmission");
            card.card_id = WorkCardId::from_u128(n);
            card
        };
        let submission = |card: &airc_lib::WorkCard| airc_work::WorkSubmission {
            submission_id: SubmissionId::new(), card_id: card.card_id, claim_id: airc_work::ClaimId::new(), instance: "ordinary".into(),
            base_sha: airc_work::GitObjectId::new("b".repeat(40)).expect("test: a sha"), head: airc_work::SubmissionHead::Unrecorded,
            artifact: artifact.clone(), publisher: author, submitted_at_ms: 10,
        };
        let verdict = |card: &airc_lib::WorkCard, on: SubmissionId, by: airc_core::PeerId, outcome: WorkReviewOutcome, at: u64| airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(), card_id: card.card_id, submission_id: on, artifact: artifact.clone(),
            review_card_id: WorkCardId::new(), review_claim_id: airc_work::ClaimId::new(), reviewer: by, outcome,
            evidence: artifact.clone(), evidence_text: None, reviewed_at_ms: at,
        };
        let board = |cards: Vec<airc_lib::WorkCard>, reviews: Vec<airc_work::WorkSubmissionReview>| std::sync::Arc::new(BoardSnapshot {
            cards, submission_reviews: reviews, lanes: Vec::new(), workspaces: Vec::new(), repo_tracking: Vec::new(),
            pull_requests: Vec::new(), manager_hats: Vec::new(), agent_availability: Vec::new(), hygiene_reports: Vec::new(),
        });
        // 1. waiting: her send-back judged the latest submission
        let mut waiting = parent(1, CardState::Review);
        let judged = submission(&waiting);
        waiting.submissions.insert(0, judged.clone());
        let sent_back = verdict(&waiting, judged.submission_id, me, WorkReviewOutcome::Failed, 20);
        let found = awaiting(&[board(vec![waiting.clone()], vec![sent_back.clone()])], me);
        assert_eq!(found.iter().map(|item| (item.card, item.reviewed, item.resubmitted)).collect::<Vec<_>>(), vec![(waiting.card_id, judged.submission_id, None)]);
        // 2. resubmitted: the review card opened for the EXACT newer submission is offered to her
        let mut resubmitted = waiting.clone();
        let newer = submission(&resubmitted);
        resubmitted.submissions.insert(0, newer.clone());
        let review_card = |owner| {
            let mut card = crate::cognition::work_preconditions::fixture_card(CardState::Open, owner, Some(resubmitted.card_id), "review");
            card.card_id = airc_work::submission_review_card_id(resubmitted.card_id, newer.submission_id);
            card
        };
        let found = awaiting(&[board(vec![resubmitted.clone(), review_card(None)], vec![sent_back.clone()])], me);
        assert_eq!(found.iter().map(|item| item.resubmitted).collect::<Vec<_>>(), vec![Some(airc_work::submission_review_card_id(resubmitted.card_id, newer.submission_id))]);
        // a review card for an older submission is not this one: nothing is offered until the exact card opens
        let mut stale = review_card(None);
        stale.card_id = airc_work::submission_review_card_id(resubmitted.card_id, judged.submission_id);
        assert!(awaiting(&[board(vec![resubmitted.clone(), stale], vec![sent_back.clone()])], me).is_empty(), "only the exact submission's review card");
        // 3. claimed by another reviewer: honoured, it leaves her view
        assert!(awaiting(&[board(vec![resubmitted.clone(), review_card(Some(airc_core::PeerId::new()))], vec![sent_back.clone()])], me).is_empty(), "another reviewer holds it");
        // 4. a terminal parent, or her newest verdict passing, ends the wait
        let mut merged = waiting.clone();
        merged.state = CardState::Merged;
        assert!(awaiting(&[board(vec![merged], vec![sent_back.clone()])], me).is_empty(), "a merged card awaits nothing");
        let passed_later = verdict(&waiting, judged.submission_id, me, WorkReviewOutcome::Passed, 30);
        assert!(awaiting(&[board(vec![waiting.clone()], vec![sent_back.clone(), passed_later])], me).is_empty(), "her newest verdict passed");
        // another reviewer's send-back is not hers
        let theirs = verdict(&waiting, judged.submission_id, airc_core::PeerId::new(), WorkReviewOutcome::Failed, 20);
        assert!(awaiting(&[board(vec![waiting], vec![theirs])], me).is_empty(), "only her own verdicts");
    }

    // what this catches (Codex on #5908 at db440c5dd): the send-back computed but never reaching her,
    // because the reader her read goes through answered no boards. Through the one read path with
    // a citizen whose board carries her send-back, her held work carries it.
    #[tokio::test]
    async fn her_read_carries_her_send_back_from_her_boards() {
        use airc_work::{BoardSnapshot, CardState, SubmissionId, WorkCardId, WorkReviewOutcome};
        let me = her();
        let artifact: airc_work::SubmissionArtifact = serde_json::from_value(serde_json::json!({"hash": "a".repeat(64), "size_bytes": 5, "mime": "text/x-diff"}))
            .expect("test: an artifact");
        let mut card = crate::cognition::work_preconditions::fixture_card(CardState::Review, Some(airc_core::PeerId::new()), None, "Sahar's resubmission");
        card.card_id = WorkCardId::from_u128(7);
        let judged = airc_work::WorkSubmission {
            submission_id: SubmissionId::new(), card_id: card.card_id, claim_id: airc_work::ClaimId::new(), instance: "ordinary".into(),
            base_sha: airc_work::GitObjectId::new("b".repeat(40)).expect("test: a sha"), head: airc_work::SubmissionHead::Unrecorded,
            artifact: artifact.clone(), publisher: airc_core::PeerId::new(), submitted_at_ms: 10,
        };
        card.submissions.insert(0, judged.clone());
        let sent_back = airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(), card_id: card.card_id, submission_id: judged.submission_id, artifact: artifact.clone(),
            review_card_id: WorkCardId::new(), review_claim_id: airc_work::ClaimId::new(), reviewer: me, outcome: WorkReviewOutcome::Failed,
            evidence: artifact, evidence_text: None, reviewed_at_ms: 20,
        };
        let board = BoardSnapshot {
            cards: vec![card.clone()], submission_reviews: vec![sent_back], lanes: Vec::new(), workspaces: Vec::new(), repo_tracking: Vec::new(),
            pull_requests: Vec::new(), manager_hats: Vec::new(), agent_availability: Vec::new(), hygiene_reports: Vec::new(),
        };
        let citizen = crate::persona::airc_citizen::StubAircCitizen::new(me.as_uuid()).with_board_snapshots(vec![board]);
        let held = read_through(&citizen, uuid::Uuid::new_v4(), me).await.expect("test: a good read");
        assert_eq!(held.awaiting.iter().map(|item| (item.card, item.reviewed)).collect::<Vec<_>>(), vec![(card.card_id, judged.submission_id)]);
    }

    // what this catches: the diff she is shown misreading git's NUL-separated output, so a renamed
    // or binary file shifts every later path onto the wrong counts. Real git -z output for one diff:
    // a root file added, one modified, one renamed, one binary, one deleted.
    #[test]
    fn a_submissions_files_parse_from_one_diffs_status_and_counts() -> Result<(), String> {
        let status = "A\0_verify_unified_bracket.rs\0M\0src/lib.rs\0R087\0old/name.rs\0new/name.rs\0M\0logo.png\0D\0gone.rs\0C100\0kept.rs\0copy.rs\0";
        let counts = "40\t0\t_verify_unified_bracket.rs\03\t1\tsrc/lib.rs\02\t2\t\0old/name.rs\0new/name.rs\0-\t-\tlogo.png\00\t12\tgone.rs\00\t0\t\0kept.rs\0copy.rs\0";
        let files = parse_changes(status, counts)?;
        let shown: Vec<(String, FileChange, Option<u64>, Option<u64>)> =
            files.into_iter().map(|file| (file.path, file.change, file.added, file.deleted)).collect();
        assert_eq!(shown, vec![
            ("_verify_unified_bracket.rs".into(), FileChange::Added, Some(40), Some(0)),
            ("src/lib.rs".into(), FileChange::Modified, Some(3), Some(1)),
            ("new/name.rs".into(), FileChange::Renamed { from: "old/name.rs".into() }, Some(2), Some(2)),
            ("logo.png".into(), FileChange::Modified, None, None),
            ("gone.rs".into(), FileChange::Deleted, Some(0), Some(12)),
            ("copy.rs".into(), FileChange::Copied { from: "kept.rs".into() }, Some(0), Some(0)),
        ]);
        // what this catches (Fable on #5924): a count that does not parse shown as "(binary)".
        // Only git's '-' means binary; anything else is an error she is told.
        assert!(parse_changes("M\0a.rs\0", "x\t1\ta.rs\0").is_err(), "a non-numeric count is a parse error, never binary");
        Ok(())
    }
}
