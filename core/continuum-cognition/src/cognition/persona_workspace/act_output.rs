//! A resident's act number resolved to its saved output, for `tool/output` called with `act`.
//! Her folded acts name only their range ("#a-#b"), so the number is the handle she holds.

use std::sync::Arc;

use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::working_memory::ActOutputRefusal;

impl PersonaWorkspaceRegistry {
    /// The saved-output handle of `persona`'s act `act`, through her public cycle's working
    /// memory, the record her view's actions source numbers. Her mind cycle keeps its own
    /// record and is not consulted: its acts are not in the view that folds them.
    pub fn act_output_handle(&self, persona: Uuid, act: u64) -> Result<String, ActOutputRefusal> {
        let memory = self
            .cycles
            .lock()
            .get(&persona)
            .map(|resident| Arc::clone(&resident.working_memory));
        memory
            .ok_or(ActOutputRefusal::NoResident { act })?
            .saved_handle(act)
    }
}
