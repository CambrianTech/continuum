//! Persona's half of the scripted conversation fixture
//! (`cognition::conversation::scripted_conversation`): the airc citizen a held-work test
//! hands the loop, read back through persona's [`CitizenConversation`].

use std::sync::Arc;

use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
use crate::persona::airc_citizen::AircCitizen;
use crate::persona::service_loop::CitizenConversation;

/// Seed the citizen `stream_citizen()` hands back — for tests that drive the held-work
/// act-question (which reads `active_claims` and calls `advance_card_to` through the
/// citizen handle).
pub trait ScriptedCitizen {
    fn with_citizen(self, citizen: Arc<dyn AircCitizen>) -> Self;
}

impl ScriptedCitizen for ScriptedConversation {
    fn with_citizen(self, citizen: Arc<dyn AircCitizen>) -> Self {
        self.with_attachment(citizen)
    }
}

impl CitizenConversation for ScriptedConversation {
    fn stream_citizen(&self) -> Option<Arc<dyn AircCitizen>> {
        self.attachment::<Arc<dyn AircCitizen>>()
    }
}
