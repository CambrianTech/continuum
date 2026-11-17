//! Her head's custody across acts and restarts (card 9cf06ade): resumed with her view as one pair
//! before anything is selected, kept with it as one pair, and checked against the live surface
//! every act so a contract that moved is told to her before she acts on it (`hands_custody`).

use super::*;
use crate::cognition::hands_custody::HandsCustody;
use crate::cognition::working_memory::{ContextView, WorkingMemory};

impl LlmDeliberationFaculty {
    /// Resume her view and her head together from her checkpoint, before this act selects anything:
    /// a restart sends the bytes her engine holds instead of a head re-read from the live surface.
    /// Only a faculty holding neither resumes (a fresh one after a restart).
    pub(in super::super) fn resume_custody(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        let mut view = self.context_view.lock();
        if view.is_some() || self.tools_epoch.lock().is_some() || self.hands_corrupt.lock().is_some() {
            return;
        }
        let (restored, hands) = memory.take_custody();
        match hands {
            HandsCustody::Held(hands) => {
                continuum_base::probe!(
                    class = "tools.custody.resumed",
                    persona = %self.persona_name,
                    generation = hands.generation,
                    verbs = hands.names.len(),
                    view = restored.is_some(),
                    "her head resumes as her checkpoint froze it: the same schemas her engine holds"
                );
                *self.tools_epoch.lock() = Some(hands);
            }
            HandsCustody::Corrupt { raw, error } => {
                continuum_base::probe!(
                    class = "tools.custody.refused",
                    persona = %self.persona_name,
                    error = %error,
                    bytes = raw.len(),
                    "her head's checkpointed custody does not decode: kept as it was, her acts refused until it is repaired"
                );
                *self.hands_corrupt.lock() = Some((raw, error));
            }
            HandsCustody::Unrecorded if restored.is_some() => {
                // A view checkpointed before her head was: her first act selects her head from the
                // live surface (measured, never called restored) and her view's blocks are kept.
                continuum_base::probe!(
                    class = "tools.custody.unrecorded",
                    persona = %self.persona_name,
                    "her view resumes with no head recorded beside it: her head is selected afresh and the engine pays what differs"
                );
            }
            HandsCustody::Unrecorded => {}
        }
        *view = restored;
    }

    /// Why her acts are refused while her head's custody is corrupt, or `None`.
    pub(in super::super) fn hands_refusal(&self) -> Option<String> {
        self.hands_corrupt.lock().as_ref().map(|(raw, error)| {
            format!("her head's checkpointed custody does not decode ({error}); its {} bytes are kept as they were until it is repaired", raw.len())
        })
    }

    /// Her held verbs whose live contract she has not been told, into her memory for this act's
    /// notices (`context_streams::tool_contracts`), before her view advances.
    pub(in super::super) fn observe_contracts(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        let changes = self.tools_epoch.lock().as_ref().map_or_else(Vec::new, |held| held.changes(|name| self.surface.live_contract(name))); // map_or_else: no held head holds no contract to change
        if !changes.is_empty() {
            continuum_base::probe!(
                class = "tools.contract.changed",
                persona = %self.persona_name,
                verbs = %changes.iter().map(|change| change.name.as_str()).collect::<Vec<_>>().join(","),
                "a held verb's live contract moved: she is told before her next act, her head keeps the bytes her engine holds"
            );
        }
        memory.set_contract_changes(changes);
    }

    /// A served prompt told her what its receipt shows whole: each contract notice in it is now what
    /// she knows (`HeldHands::tell`), never marked when the notice was merely made.
    pub(in super::super) fn tell_served(&self, receipt: &continuum_context::view::Receipt) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        let shown: Vec<_> = receipt
            .items
            .iter()
            .filter(|item| item.level == continuum_context::level::Level::Full && !item.retired)
            .filter_map(|item| item.element)
            .collect();
        let mut held = self.tools_epoch.lock();
        let Some(hands) = held.as_mut() else {
            return;
        };
        // What she was shown is told; what remains is untold, and her calls on it are held back
        // before they run (`act_observe::contract_gate`).
        let (told, untold): (Vec<_>, Vec<_>) = memory
            .contract_changes()
            .into_iter()
            .partition(|change| shown.contains(&crate::cognition::context_streams::tool_contract_element(self.persona_id, change)));
        for change in &told {
            hands.tell(change);
        }
        memory.set_contract_changes(untold);
    }

    /// Her head's custody as this faculty holds it now: corrupt bytes kept, else her held head.
    fn hands_custody(&self) -> HandsCustody {
        if let Some((raw, error)) = self.hands_corrupt.lock().clone() {
            return HandsCustody::Corrupt { raw, error };
        }
        self.tools_epoch.lock().clone().map_or(HandsCustody::Unrecorded, HandsCustody::Held) // map_or: no head held records none
    }

    /// Keep `view` (passed by a caller already holding her view's lock) with her head, as one pair.
    pub(in super::super) fn keep_custody_with(&self, memory: &WorkingMemory, view: Option<ContextView>) {
        memory.keep_custody(view, self.hands_custody());
    }

    /// Keep her view and her head as one pair, for the checkpoint.
    pub(in super::super) fn keep_custody(&self) {
        if let Some(memory) = self.working_memory.as_deref() {
            let view = self.context_view.lock().clone();
            self.keep_custody_with(memory, view);
        }
    }
}
