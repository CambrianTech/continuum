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
use crate::cognition::workspace::{EndedClaim, HeldWork};

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

    fn rejected(&self, line: &str) -> Item<ContextMessage> {
        let occurrence = element(self.persona, Producer::Work, &[b"rejected".as_slice(), line.as_bytes()].concat());
        Item::whole(occurrence, Cause::Root(Origin::External), format!("rejected:{:032x}", occurrence.as_u128()), ContextMessage::user(line.to_owned()))
            .with_pointer(ContextMessage::user("[my work] a claim was rejected: work/list".to_owned()))
    }
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
    fn consumer(&self) -> continuum_context::ladder::Consumer {
        continuum_context::ladder::Consumer::Actionable
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let work = &self.work;
        let mut items: Vec<_> = work.ended.iter().map(|ended| self.ended(ended)).collect();
        items.extend(work.rejections.iter().map(|line| self.rejected(line)));
        items.extend(work.holds.iter().map(|held| self.card(held)));
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
}
