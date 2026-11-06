//! What her situation shows now (docs/architecture/PERCEPTION-FACTS.md), as keyed items of her
//! view: one per fact by its id (`perception_facts::render_facts`), and one per perception the
//! system spoke into her room (a `TurnVoice::Perception` turn), by its event.
//!
//! Before this the facts rendered after every append-only block, were dropped from the next act
//! and replaced by what that act appended, so the previous act's tail re-prefilled behind them
//! (the M5, 2026-11-06: the "changed inside her previous prompt" class). Keyed, a fact that holds
//! unchanged appends nothing, a changed one appends its new version, and one that stops holding
//! is ended once ([`Source::ended`]): within an epoch her prompt only appends.
//!
//! Placement, by class: every fact describes her situation as of the act it is appended in, so
//! every class is a state of this source, appended where it fired and ended when it stops; none
//! is true only for the request that carries it (the bounds fact counts the turns the room
//! offered, and changes only when that count does). The clock is the one line true only now; it
//! rides the act's tail, which joins her view as sent (`View::append_sent`), so no class is ever
//! dropped from a sent prompt. A perception turn is an event: it stays what happened.

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{element, priority, ContextMessage, Producer};

/// The key prefix of a perception turn: an event among the states, never ended.
const EVENT: &str = "event/";

pub(crate) struct Fact {
    pub key: String,
    pub text: String,
}

impl Fact {
    /// A fact that holds now, under its id.
    pub fn state(id: &str, text: String) -> Self {
        Self { key: id.to_owned(), text }
    }

    /// A perception the system spoke into her room, under its turn's key.
    pub fn event(turn: &str, text: String) -> Self {
        Self { key: format!("{EVENT}{turn}"), text }
    }
}

#[derive(Default)]
pub(crate) struct Facts {
    /// Whose facts these are (`super::element`).
    pub persona: uuid::Uuid,
    pub facts: Vec<Fact>,
}

impl Source<ContextMessage> for Facts {
    fn name(&self) -> &str {
        "facts"
    }

    fn priority(&self) -> u8 {
        priority::FACTS
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.facts
            .iter()
            .map(|fact| {
                // Observed of her situation, from outside her. Collapsed at a start, a fact leaves
                // her view: it is re-observed every act it holds.
                let id = element(self.persona, Producer::Fact, fact.key.as_bytes());
                Item::whole(id, Cause::Root(Origin::External), fact.key.clone(), ContextMessage::user(fact.text.clone()))
            })
            .collect()
    }

    fn states(&self) -> bool {
        true
    }

    fn ended(&self, key: &str) -> Option<ContextMessage> {
        (!key.starts_with(EVENT)).then(|| ContextMessage::user(format!("[{}: no longer observed]", key.replace('_', " "))))
    }
}
