//! Her held work as keyed items (STATE-DRIVEN-SURFACE.md): one item per card she holds, keyed by
//! the card. Its OCCURRENCE is the producer's revision of what she is shown, the same contract
//! every source here keeps (an act's `seq`, an open view's `touched_seq`, a task's terminal state):
//! the state entry it belongs to (`Held::entered_ms`, so Review, InProgress, Review again is three
//! occurrences, never a reused old one) and the content of the rendered projection (title, her
//! stance among several claims, her submission or its refusal, what she has in hand), never a
//! heartbeat, which changes none of it. So an unchanged card appends nothing act after act, and any
//! change she should see appends once. A claim that ended and a rejected claim are their own
//! items. Replaces the `active-work` grounding block, which re-rendered every card as one blob
//! whenever any of them changed.

use airc_core::shown_id::shown_form;
use continuum_context::cause::{Cause, ElementId, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::card_facts::state_str;
use crate::cognition::work_preconditions::{CardRole, Held, NextMove};
use crate::cognition::workspace::{AwaitingResubmission, ChangedFile, ChangesUnread, EndedClaim, FileChange, HeldWork, ReviewRequest, SubmissionChanges};

/// The occurrence of a held card's rendering: its card, the state entry it belongs to, and the
/// rendered projection itself (`whole`).
pub(crate) fn held_card_element(persona: uuid::Uuid, held: &Held, whole: &str) -> ElementId {
    let card = held.card.card_id.as_uuid();
    element(persona, Producer::Work, &[card.as_bytes().as_slice(), &held.entered_ms.to_be_bytes(), whole.as_bytes()].concat())
}

#[derive(Default)]
pub(crate) struct HeldWorkStream {
    pub persona: uuid::Uuid,
    pub work: std::sync::Arc<HeldWork>,
}

impl HeldWorkStream {
    fn card(&self, held: &Held) -> Item<ContextMessage> {
        let card = &held.card;
        let id = shown_form(card.card_id.as_uuid());
        let role = match held.role {
            CardRole::Code => "your work".to_owned(),
            CardRole::Review { parent } => format!("you review card {}", shown_form(parent.as_uuid())),
        };
        // With several held, which one her work turns are bound to (`HeldWork::selected`).
        let stance = if self.work.holds.len() < 2 {
            ""
        } else if self.work.selected == Some(card.card_id) {
            " (selected claim)"
        } else {
            " (held)"
        };
        let state = state_str(&card.state);
        // The verbs whose preconditions hold on this card, with what they grant (`in_hand`).
        let can = held.in_hand.join(", ");
        let head = format!("[my work] card {id} \"{}\": {role}, {state}{stance}", card.title);
        let mut lines = vec![head];
        lines.extend(submission_fact(held));
        lines.extend(held.next.as_ref().map(|next| next_move_line(next, &id)));
        lines.push(format!("you can: {can}"));
        let whole = lines.join("\n");
        let next = held.next.as_ref().map(|next| format!("; next: {}", next.verb().offered())).unwrap_or_default(); // unwrap_or_default: no next move = nothing to add
        let pointer = format!("[my work] card {id} \"{}\" {state}: you can {can}{next}", card.title);
        Item::whole(held_card_element(self.persona, held, &whole), Cause::Root(Origin::ActivityStart), card.card_id.as_uuid().to_string(), ContextMessage::user(whole))
            .with_pointer(ContextMessage::user(pointer))
    }

    /// Her latest submission on one of her cards: the files it changes against its base and the
    /// newest verdict, in the reviewer's own words (card 1e9774cc). Placed right after its card, so
    /// a review that names a file meets the paths she submitted. Keyed by the submission; a new
    /// verdict is a new occurrence.
    fn submission(&self, changes: &SubmissionChanges) -> Item<ContextMessage> {
        let id = shown_form(changes.card.as_uuid());
        let submission = shown_form(changes.submission.as_uuid());
        let base = &changes.base;
        let mut lines = Vec::new();
        let summary = match &changes.files {
            Ok(files) => {
                lines.push(format!(
                    "[my work] your submission {submission} on card {id} \"{}\" changes {} file(s) against its base {base}:",
                    changes.title,
                    files.len()
                ));
                lines.extend(files.iter().map(changed_file_line));
                format!("{} file(s) changed", files.len())
            }
            Err(why) => {
                lines.push(format!(
                    "[my work] your submission {submission} on card {id} \"{}\": what it changes against its base {base} could not be read: {}",
                    changes.title,
                    unread_line(why)
                ));
                "its files could not be read".to_owned()
            }
        };
        let verdict = match &changes.verdict {
            Some(verdict) => {
                let outcome = match verdict.outcome {
                    airc_work::WorkReviewOutcome::Passed => "PASSED",
                    airc_work::WorkReviewOutcome::Failed => "FAILED",
                    // a newer reviewer's outcome this build cannot name: said as such, never guessed
                    airc_work::WorkReviewOutcome::Unknown => "of an outcome this build does not know",
                };
                lines.push(format!("verdict: {outcome} by {}", shown_form(verdict.reviewer.as_uuid())));
                if let Some(evidence) = &verdict.evidence {
                    lines.push(format!("in the reviewer's words:\n{evidence}"));
                }
                format!("; verdict {outcome}")
            }
            None => String::new(),
        };
        let whole = lines.join("\n");
        let pointer = format!("[my work] submission {submission} on card {id}: {summary}{verdict}");
        let occurrence = element(
            self.persona,
            Producer::Work,
            &[b"submission".as_slice(), changes.submission.as_uuid().as_bytes(), whole.as_bytes()].concat(),
        );
        Item::whole(occurrence, Cause::Root(Origin::External), format!("submission:{}", changes.submission.as_uuid()), ContextMessage::user(whole))
            .with_pointer(ContextMessage::user(pointer))
    }

    fn ended(&self, ended: &EndedClaim) -> Item<ContextMessage> {
        let id = shown_form(ended.card.as_uuid());
        let whole = format!(
            "[my work] Your claim on card {id} \"{}\" is no longer held by you (lease expired or released). \
             Do not continue work on it as yours: check the board before touching it again.",
            ended.title
        );
        let occurrence = element(self.persona, Producer::Work, &[b"ended".as_slice(), ended.card.as_uuid().as_bytes()].concat());
        Item::whole(occurrence, Cause::Root(Origin::External), format!("ended:{}", ended.card.as_uuid()), ContextMessage::user(whole))
            .with_pointer(ContextMessage::user(format!("[my work] claim on card {id} ended: work/list")))
    }

    /// A card she sent back (card 7f8c248e): nothing to do on it until its author resubmits, and,
    /// once they have, the review card the board opened for it, offered to her first. Keyed by the
    /// card, so the resubmission supersedes the wait in place; told, never imposed.
    fn awaiting(&self, item: &AwaitingResubmission) -> Item<ContextMessage> {
        let id = shown_form(item.card.as_uuid());
        let reviewed = shown_form(item.reviewed.as_uuid());
        let (whole, pointer) = match item.resubmitted {
            None => (
                format!(
                    "[my work] You sent back card {id} \"{}\" (submission {reviewed}). There is nothing to do on it until its author \
                     resubmits: the board then opens a review card for the new submission, and it is shown to you here first. \
                     No card is needed to wait for it; your other work is yours to choose meanwhile.",
                    item.title
                ),
                format!("[my work] card {id} awaits its author's resubmission"),
            ),
            Some(review) => {
                let review = shown_form(review.as_uuid());
                (
                    format!(
                        "[my work] Card {id} \"{}\", which you sent back, was resubmitted: its review card {review} is open and \
                         offered to you first. To re-review it, claim {review} (work/claim), read the new submission \
                         (work/submission) and give your verdict (work/review).",
                        item.title
                    ),
                    format!("[my work] card {id} was resubmitted: review card {review} is open for you"),
                )
            }
        };
        let resubmitted = item.resubmitted.map(|review| review.as_uuid()).unwrap_or_default(); // unwrap_or_default: still waiting = the nil review card in the occurrence key
        let occurrence = element(
            self.persona,
            Producer::Work,
            &[b"awaiting".as_slice(), item.card.as_uuid().as_bytes(), item.reviewed.as_uuid().as_bytes(), resubmitted.as_bytes()].concat(),
        );
        Item::whole(occurrence, Cause::Root(Origin::External), format!("awaiting:{}", item.card.as_uuid()), ContextMessage::user(whole))
            .with_pointer(ContextMessage::user(pointer))
    }

    fn rejected(&self, line: &str) -> Item<ContextMessage> {
        let occurrence = element(self.persona, Producer::Work, &[b"rejected".as_slice(), line.as_bytes()].concat());
        Item::whole(occurrence, Cause::Root(Origin::External), format!("rejected:{:032x}", occurrence.as_u128()), ContextMessage::user(line.to_owned()))
            .with_pointer(ContextMessage::user("[my work] a claim was rejected: work/list".to_owned()))
    }
}

/// Why her submission's files could not be read, and what she can do about it.
fn unread_line(why: &ChangesUnread) -> String {
    match why {
        ChangesUnread::NoCheckout => "her card's checkout is not on this node".to_owned(),
        ChangesUnread::CommitsAbsent { base, head } => {
            format!("its commits {base}..{head} are not in your checkout; fetch them (git fetch) and it reads on your next turn")
        }
        ChangesUnread::GitRefused { detail } => format!("git refused the diff: {detail}"),
        ChangesUnread::Unparsable { detail } => format!("git's answer could not be read: {detail}"),
    }
}

/// One changed file, as a line: how it changed, its path, and its line counts.
fn changed_file_line(file: &ChangedFile) -> String {
    let change = match &file.change {
        FileChange::Added => "added".to_owned(),
        FileChange::Deleted => "deleted".to_owned(),
        FileChange::Modified => "modified".to_owned(),
        FileChange::Renamed { from } => format!("renamed from {from} to"),
        FileChange::Copied { from } => format!("copied from {from} to"),
    };
    let counts = match (file.added, file.deleted) {
        (Some(added), Some(deleted)) => format!(" (+{added} -{deleted})"),
        _ => " (binary)".to_owned(),
    };
    format!("  {change} {}{counts}", file.path)
}

/// The one fact about a card's submission its holder needs in this role: the parent's submission
/// a reviewer reads, an author's latest submission, or why the board refused her last one.
fn submission_fact(held: &Held) -> Option<String> {
    let card = &held.card;
    if let CardRole::Review { parent } = held.role {
        return Some(format!("its submission is on card {}: read it with work/submission", shown_form(parent.as_uuid())));
    }
    if let Some(refused) = &card.last_submission_rejection {
        return Some(format!("your last submission {} was refused: {}", shown_form(refused.submission_id.as_uuid()), refused.reason));
    }
    // Newest accepted first (airc-work `WorkCard::submissions`).
    card.submissions
        .first()
        .map(|latest| format!("your latest submission: {}", shown_form(latest.submission_id.as_uuid())))
}

/// Her card's next move, rendered from its type: the verb under the name her tool list offers it
/// (`WorkVerb::offered`) and the card it acts on.
fn next_move_line(next: &NextMove, id: &str) -> String {
    match next {
        NextMove::SubmitLinkedPullRequest { pull_request } => format!(
            "next: your pull request #{} is linked to this card but was never submitted, so no reviewer can judge it \
             and it cannot reach your credit: call {} with card_id {id}",
            pull_request.number,
            next.verb().offered()
        ),
    }
}

impl Source<ContextMessage> for HeldWorkStream {
    fn name(&self) -> &str {
        "held-work"
    }

    fn priority(&self) -> u8 {
        priority::GROUNDING
    }

    /// Her held card is what she acts through: a price choice never shows it below its whole.
    /// Teammates' submissions awaiting a reviewer on every board she stands in: one item, the count
    /// and the longest waiting, keyed once so a drained queue supersedes it in place. Told, never
    /// assigned: whether to review is hers.
    fn review_requests(&self, requests: &[ReviewRequest]) -> Option<Item<ContextMessage>> {
        let oldest = requests.first()?;
        let id = shown_form(oldest.review_card.as_uuid());
        let n = requests.len();
        let whole = format!(
            "[review requests] {n} teammate submission(s) await a reviewer on your boards; the longest waiting is review card \
             {id} \"{}\". A review: work/claim {id}, read work/submission (its patch and head), check its CI \
             (code/github/pr-checks), then work/review with outcome, verified_head and evidence_text. \
             work/list group=review_requests lists a board's requests.",
            oldest.title
        );
        let pointer = format!("[review requests] {n} await a reviewer; the longest waiting: review card {id}");
        let occurrence = element(self.persona, Producer::Work, &[b"review-requests".as_slice(), oldest.review_card.as_uuid().as_bytes(), &(n as u64).to_be_bytes()].concat());
        Some(
            Item::whole(occurrence, Cause::Root(Origin::External), "review-requests".to_string(), ContextMessage::user(whole))
                .with_pointer(ContextMessage::user(pointer)),
        )
    }

    fn consumer(&self) -> continuum_context::ladder::Consumer {
        continuum_context::ladder::Consumer::Actionable
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let work = &self.work;
        let mut items: Vec<_> = work.ended.iter().map(|ended| self.ended(ended)).collect();
        items.extend(work.rejections.iter().map(|line| self.rejected(line)));
        for held in &work.holds {
            items.push(self.card(held));
            items.extend(work.submissions.iter().filter(|changes| changes.card == held.card.card_id).map(|changes| self.submission(changes)));
        }
        items.extend(work.awaiting.iter().map(|item| self.awaiting(item)));
        items.extend(self.review_requests(&work.review_requests));
        items
    }

    /// No fold: one line per card she holds, superseded by its key; an ended or rejected claim is
    /// one line until the next start.
    fn fold(&self, _folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_work::{CardState, WorkCardId};
    use continuum_context::epoch::Block;
    use continuum_context::view::{Advance, Claim, GrantMismatch, View};
    use crate::cognition::work_preconditions::{fixture_card, FIXTURE_NOW_MS};
    use crate::cognition::workspace::SubmissionVerdict;

    fn her() -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    /// Her card `id` in `state`, entered at `entered`, reviewing `reviews` when it is a review card.
    fn held(id: u128, state: CardState, reviews: Option<WorkCardId>, entered: u64) -> Held {
        let mut card = fixture_card(state, Some(her()), reviews, "fix the thing");
        card.card_id = WorkCardId::from_u128(id);
        Held::of(card, her(), FIXTURE_NOW_MS, entered).expect("she owns the fixture") // expect: the fixture names her owner
    }

    fn stream(holds: Vec<Held>, selected: Option<u128>) -> HeldWorkStream {
        let work = HeldWork { holds, selected: selected.map(WorkCardId::from_u128), ..Default::default() };
        HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(work) }
    }

    fn whole(claims: &[Claim]) -> Vec<usize> {
        claims.iter().map(|claim| claim.full).collect()
    }

    /// The occurrence her view shows last.
    fn newest(view: &View<ContextMessage>) -> Option<ElementId> {
        view.receipt().items.last().and_then(|shown| shown.element)
    }

    // what this catches: Benchy and Atlas on the M5, 2026-11-04, learning where they stood by
    // trial. Her held review card says her role, its state and what she has in hand (the verdict
    // verb, the parent's submission, the code hands to test it, never submit), and collapsed to
    // its pointer it still says what she can do.
    #[test]
    fn a_held_card_says_what_she_has_in_hand() {
        let items = stream(vec![held(1, CardState::Claimed, Some(WorkCardId::from_u128(0xabcd)), 10)], None).items();
        let text = items[0].full().text();
        for wanted in ["you review card", "work/review", "work/submission", "code/**"] {
            assert!(text.contains(wanted), "{wanted}: {text}");
        }
        assert!(!text.contains("work/submit,") && !text.ends_with("work/submit"), "a reviewer never submits: {text}");
        assert!(items[0].pointer().is_some_and(|p| p.text().contains("work/review")), "collapsed, she still sees her moves");
    }

    // what this catches: Benchy's #5791 on the M5, 2026-11-07, a PR linked to her held card with no
    // submission, so no reviewer could judge it and nothing told her. Her item names the submit move
    // under the name her tool list offers (never the slash form), with the card, and so does its
    // pointer.
    #[test]
    fn a_linked_pr_with_no_submission_is_her_next_move() {
        let mut card = held(1, CardState::Review, None, 10).card;
        card.pull_request = Some(crate::cognition::work_preconditions::fixture_pull_request());
        let held = Held::of(card, her(), FIXTURE_NOW_MS, 10).expect("she owns the fixture"); // expect: the fixture names her owner
        let items = stream(vec![held], None).items();
        let text = items[0].full().text();
        assert!(text.contains("pull request #5791") && text.contains("call work_submit with card_id"), "{text}");
        assert_eq!(text.matches("work_submit").count(), 1, "rendered once: {text}");
        assert!(items[0].pointer().is_some_and(|p| p.text().contains("next: work_submit")), "collapsed, the move stays");
    }

    // what this catches (Codex on the occurrence contract): an occurrence keyed by card and state
    // alone, so a change she should see within one state (a refused submission, the selected
    // claim switching) either never reaches her or reuses an old occurrence, and Review,
    // InProgress, Review again resolves to the first Review's occurrence. Through the real view:
    // an unchanged card or a heartbeat appends nothing; each change appends once; a re-entry is a
    // fresh occurrence.
    #[test]
    fn through_her_view_a_card_appends_once_per_change_and_a_reentry_is_a_new_occurrence() -> Result<(), GrantMismatch> {
        let first = stream(vec![held(1, CardState::InProgress, None, 10)], None);
        let (mut view, _) = View::start(&continuum_context::cost::Chars, Vec::new(), &[&first], whole)?;
        let shown = newest(&view);
        assert_eq!(view.advance(&[&first]), Advance::default(), "unchanged: nothing appended");
        let mut beat = held(1, CardState::InProgress, None, 10);
        beat.card.last_heartbeat_at_ms = Some(FIXTURE_NOW_MS);
        beat.card.updated_at_ms = FIXTURE_NOW_MS;
        assert_eq!(view.advance(&[&stream(vec![beat], None)]), Advance::default(), "a heartbeat changes nothing she reads");
        let mut refused = held(1, CardState::InProgress, None, 10);
        refused.card.last_submission_rejection = Some(airc_work::RejectedSubmission {
            submission_id: airc_work::SubmissionId::from_u128(3),
            card_id: WorkCardId::from_u128(1),
            publisher: her(),
            submitted_at_ms: 5,
            reason: airc_work::SubmissionRejectionReason::WrongClaim,
        });
        let refused = stream(vec![refused], None);
        assert_eq!(view.advance(&[&refused]).items, 1, "a refusal in the same state appends once");
        assert_ne!(newest(&view), shown, "as a new occurrence");
        assert_eq!(view.advance(&[&refused]), Advance::default(), "and once only");

        let two = |selected| stream(vec![held(1, CardState::InProgress, None, 10), held(2, CardState::Claimed, None, 11)], Some(selected));
        let (mut view, _) = View::start(&continuum_context::cost::Chars, Vec::new(), &[&two(1)], whole)?;
        assert_eq!(view.advance(&[&two(2)]).items, 2, "a selection switch re-renders both cards' stance, once each");
        assert_eq!(view.advance(&[&two(2)]), Advance::default());

        let at = |state, entered| stream(vec![held(1, state, None, entered)], None);
        let (mut view, _) = View::start(&continuum_context::cost::Chars, Vec::new(), &[&at(CardState::Review, 20)], whole)?;
        let review = newest(&view);
        view.advance(&[&at(CardState::InProgress, 30)]);
        view.advance(&[&at(CardState::Review, 40)]);
        assert_ne!(newest(&view), review, "Review entered again is a fresh occurrence, never the old one");
        Ok(())
    }

    // what this catches (Aiko, card 60a8875d / #5923, 2026-11-11): a review said three scratch files
    // were committed at the repo root; her view showed only "your latest submission", she guessed
    // their path, and git status showed a clean tree because they were committed. Her own card's
    // latest submission now lists every file it changes against its base, right after the card, with
    // the reviewer's words verbatim, so the file the review names is a path she can see.
    #[test]
    fn her_submission_lists_the_files_it_changes_beside_the_reviewers_words() {
        let card = held(1, CardState::InProgress, None, 10);
        let evidence = "Three scratch files are committed at the repo root: _verify_unified_bracket.rs and two more. Remove them.";
        let changes = SubmissionChanges {
            card: card.card.card_id,
            title: card.card.title.clone(),
            submission: airc_work::SubmissionId::from_u128(9),
            base: "abc123".into(),
            head: "def456".into(),
            files: Ok(vec![
                ChangedFile { path: "_verify_unified_bracket.rs".into(), change: FileChange::Added, added: Some(40), deleted: Some(0) },
                ChangedFile { path: "core/continuum-prompt/src/lib.rs".into(), change: FileChange::Modified, added: Some(3), deleted: Some(1) },
            ]),
            verdict: Some(SubmissionVerdict {
                reviewer: airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(2)),
                outcome: airc_work::WorkReviewOutcome::Failed,
                evidence: Some(evidence.into()),
            }),
        };
        let work = HeldWork { holds: vec![card], submissions: vec![changes], ..Default::default() };
        let items = HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(work) }.items();
        assert_eq!(items.len(), 2, "her card, then its submission");
        let text = items[1].full().text();
        assert!(text.contains("  added _verify_unified_bracket.rs (+40 -0)"), "the root file the review names is listed by its path: {text}");
        assert!(text.contains("  modified core/continuum-prompt/src/lib.rs (+3 -1)"), "{text}");
        assert!(text.contains("verdict: FAILED") && text.contains(evidence), "the reviewer's words, verbatim: {text}");
        assert!(items[1].pointer().is_some_and(|p| p.text().contains("2 file(s) changed") && p.text().contains("FAILED")), "collapsed, the count and the verdict stay");
    }

    // what this catches: a submission whose commits are not in her checkout rendering nothing, so
    // she reads an empty list as "nothing changed". She is told why the files could not be read.
    #[test]
    fn unreadable_files_are_said_never_shown_as_none() {
        let card = held(1, CardState::InProgress, None, 10);
        let changes = SubmissionChanges {
            card: card.card.card_id, title: card.card.title.clone(), submission: airc_work::SubmissionId::from_u128(9),
            base: "abc123".into(), head: "def456".into(), files: Err(ChangesUnread::NoCheckout), verdict: None,
        };
        let work = HeldWork { holds: vec![card], submissions: vec![changes], ..Default::default() };
        let items = HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(work) }.items();
        let text = items[1].full().text();
        assert!(text.contains("could not be read: her card's checkout is not on this node"), "{text}");
    }
    // what this catches: a teammate's submission awaiting a reviewer on a board none of her turns
    // are in (the M5, 2026-11-14: 26 of 28 readable requests on the cambriantech board while her
    // board view read only the turn's room). One item says how many and names the longest waiting,
    // with the calls a review takes; nothing to review is no item at all.
    #[test]
    fn review_requests_on_her_boards_are_one_item_naming_the_longest_waiting() {
        let request = |id: u128, title: &str, at: u64| ReviewRequest { review_card: WorkCardId::from_u128(id), title: title.into(), opened_at_ms: at };
        let work = HeldWork { review_requests: vec![request(0xa, "review: pipefail", 1), request(0xb, "review: head with no body", 2)], ..Default::default() };
        let items = HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(work) }.items();
        assert_eq!(items.len(), 1, "one item, however many requests");
        let text = items[0].full().text();
        let oldest = shown_form(WorkCardId::from_u128(0xa).as_uuid());
        assert!(text.contains("2 teammate submission(s)") && text.contains(&format!("work/claim {oldest}")) && text.contains("review: pipefail"), "{text}");
        assert!(text.contains("verified_head") && text.contains("evidence_text"), "a review's parts are named before she calls: {text}");
        let none = HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(HeldWork::default()) }.items();
        assert!(none.is_empty());
    }
}
