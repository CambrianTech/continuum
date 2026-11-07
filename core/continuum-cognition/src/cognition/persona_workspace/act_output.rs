//! A resident's act number resolved to its saved output, for `tool/output` called with `act`.
//! Her folded acts name only their range ("#a-#b"), so the number is the handle she holds.

use std::sync::Arc;

use uuid::Uuid;

use super::PersonaWorkspaceRegistry;
use crate::cognition::working_memory::ActOutputRefusal;

/// No resident mind runs for the caller, so no record can resolve an act number: a fault of the
/// substrate's, never a refusal of hers.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("act #{act} cannot be resolved: no resident mind runs for this caller")]
pub struct NoResidentMind {
    pub act: u64,
}

impl PersonaWorkspaceRegistry {
    /// The saved-output handle of `persona`'s act `act`, through her public cycle's working
    /// memory, the record her view's actions source numbers. Her mind cycle keeps its own
    /// record and is not consulted: its acts are not in the view that folds them. The outer
    /// error is the substrate's (no resident), the inner one her refusal.
    pub fn act_output_handle(&self, persona: Uuid, act: u64) -> Result<Result<String, ActOutputRefusal>, NoResidentMind> {
        let memory = self
            .cycles
            .lock()
            .get(&persona)
            .map(|resident| Arc::clone(&resident.working_memory));
        Ok(memory.ok_or(NoResidentMind { act })?.saved_handle(act))
    }
}
