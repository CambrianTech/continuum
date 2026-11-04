//! Her held work as keyed items (STATE-DRIVEN-SURFACE.md): one item per card she holds, its
//! occurrence named by the card AND its state, so the item changes only on a transition and an
//! unchanged board appends nothing. Each says what the card is, her role on it, its state, the
//! one fact about its submission she needs, and the moves its state gives her (the same table
//! her tool surface reads, `card_moves`). A claim that ended and a rejected claim are their own
//! items. Replaces the `active-work` grounding block, which re-rendered every card as one
//! blob whenever any of them changed.

use airc_core::shown_id::shown_form;
use continuum_context::cause::{Cause, ElementId, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};
use crate::cognition::card_facts::state_str;
use crate::cognition::card_moves::{CardRole, Held, Moves, NoMove};
use crate::cognition::workspace::{EndedClaim, HeldWork};

/// The one mapping from a held card's state to the occurrence her view renders: a transition is
/// a new occurrence, an unchanged state the same one.
pub(crate) fn held_card_element(persona: uuid::Uuid, held: &Held) -> ElementId {
    let card = held.card.card_id.as_uuid();
    element(persona, Producer::Work, &[card.as_bytes().as_slice(), state_str(&held.card.state).as_bytes()].concat())
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
        // Some moves, or the typed reason there are none (`card_moves::Moves`).
        let can = match &held.moves {
            Moves::Some(_) => held.moves.commands().collect::<Vec<_>>().join(", "),
            Moves::None(NoMove::Held { by }) => format!("nothing: peer {} holds it", shown_form(by.as_uuid())),
            Moves::None(NoMove::Ended(ended)) => format!("nothing: it is {}", state_str(ended)),
        };
        let head = format!("[my work] card {id} \"{}\": {role}, {state}{stance}", card.title);
        let mut lines = vec![head];
        lines.extend(submission_fact(held));
        lines.push(format!("you can: {can}"));
        let whole = lines.join("\n");
        let pointer = format!("[my work] card {id} \"{}\" {state}: you can {can}", card.title);
        Item::whole(held_card_element(self.persona, held), Cause::Root(Origin::ActivityStart), card.card_id.as_uuid().to_string(), ContextMessage::user(whole))
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

impl Source<ContextMessage> for HeldWorkStream {
    fn name(&self) -> &str {
        "held-work"
    }

    fn priority(&self) -> u8 {
        priority::GROUNDING
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        let work = &self.work;
        let mut items: Vec<_> = work.ended.iter().map(|ended| self.ended(ended)).collect();
        items.extend(work.rejections.iter().map(|line| self.rejected(line)));
        items.extend(work.holds.iter().map(|held| self.card(held)));
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_work::{CardState, WorkCard, WorkCardId};
    use continuum_context::epoch::Block;

    fn her() -> airc_core::PeerId {
        airc_core::PeerId::from_uuid(uuid::Uuid::from_u128(1))
    }

    fn card(state: CardState, reviews: Option<WorkCardId>) -> WorkCard {
        crate::cognition::card_moves::fixture_card(state, Some(her()), reviews, "review the parent")
    }

    fn stream(cards: Vec<WorkCard>) -> HeldWorkStream {
        let holds = cards.into_iter().filter_map(|card| Held::of(card, her())).collect();
        HeldWorkStream { persona: uuid::Uuid::from_u128(7), work: std::sync::Arc::new(HeldWork { holds, ..Default::default() }) }
    }

    // what this catches: Benchy and Atlas on the M5, 2026-11-04, learning where they stood by
    // trial (work/list 6 times, work/submit 5 times on a review card). Her held card says her role,
    // its state and what she can do, the reviewer's verdict verb included; an unchanged state is
    // the same occurrence act after act (nothing appended, the prefix kept), and a transition is a
    // new one (one append). Collapsed to its pointer, the card still names her moves.
    #[test]
    fn a_held_card_says_her_moves_and_changes_only_on_a_transition() {
        let parent = WorkCardId::from_u128(0xabcd);
        let claimed = stream(vec![card(CardState::Claimed, Some(parent))]).items();
        assert_eq!(claimed.len(), 1);
        let text = claimed[0].full().text();
        assert!(text.contains("you review card") && text.contains("work/review") && text.contains("work/submission"), "{text}");
        assert!(!text.contains("work/submit,") && !text.ends_with("work/submit"), "a reviewer never submits: {text}");
        assert!(claimed[0].pointer().is_some_and(|p| p.text().contains("work/review")), "collapsed, her card still says what she can do");
        let again = stream(vec![card(CardState::Claimed, Some(parent))]).items();
        assert_eq!((claimed[0].id(), claimed[0].full()), (again[0].id(), again[0].full()), "an unchanged state is byte-identical");
        let reviewed = stream(vec![card(CardState::Review, Some(parent))]).items();
        assert_eq!(claimed[0].key(), reviewed[0].key(), "the same card is the same subject");
        assert_ne!(claimed[0].id(), reviewed[0].id(), "a transition is a new occurrence");
        assert!(!reviewed[0].full().text().contains("work/review"), "{}", reviewed[0].full().text());
        let code = stream(vec![card(CardState::InProgress, None)]).items();
        assert!(code[0].full().text().contains("code/**") && code[0].full().text().contains("work/submit"), "{}", code[0].full().text());
    }
}
