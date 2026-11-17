//! Her held verbs whose contract moved since her head froze them (card 9cf06ade): one notice per
//! transition, appended before her next act. Her head keeps sending the schemas her engine holds
//! until its next epoch, so this notice is how she learns what a call is checked against now; she is
//! never held to a contract she was not shown. Keyed by the transition, so a notice is appended once
//! and a later transition (restored to an earlier contract included) appends its own.

use continuum_context::cause::{Cause, Origin};
use continuum_context::source::{Item, Source};

use super::{priority, tool_contract_element, ContextMessage};
use crate::cognition::working_memory::WorkingMemory;

pub(crate) struct ToolContracts<'a> {
    pub memory: &'a WorkingMemory,
    /// Whose occurrences these are (`super::element`).
    pub persona: uuid::Uuid,
}

impl Source<ContextMessage> for ToolContracts<'_> {
    fn name(&self) -> &str {
        "tool-contracts"
    }

    fn priority(&self) -> u8 {
        priority::NOTICES
    }

    /// Required: a contract she acts under is never dropped for room; collapsed, it still says it.
    fn required(&self) -> bool {
        true
    }

    fn items(&self) -> Vec<Item<ContextMessage>> {
        self.memory
            .contract_changes()
            .iter()
            .map(|change| {
                // A contract change comes from her surface, outside her: an external origin.
                let message = ContextMessage::user(change.text());
                Item::whole(tool_contract_element(self.persona, change), Cause::Root(Origin::External), change.key(), message.clone())
                    .with_pointer(message)
            })
            .collect()
    }

    /// No fold: what she is told is bounded by her held verbs, each told once per transition.
    fn fold(&self, _folded: &[Item<ContextMessage>]) -> Option<ContextMessage> {
        None
    }
}
