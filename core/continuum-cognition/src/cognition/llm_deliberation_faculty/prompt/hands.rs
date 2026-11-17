//! Her head's custody across acts and restarts (card 9cf06ade): resumed with her view as one pair
//! before anything is selected, kept with it as one pair, and checked against the live surface
//! every act so a contract that moved is told to her before she acts on it (`hands_custody`).

use super::*;
use crate::cognition::hands_custody::{HandsCustody, HandsRefusal};
use crate::cognition::working_memory::{ContextView, WorkingMemory};

impl LlmDeliberationFaculty {
    /// Resume her view and her head together from her checkpoint, before this act selects anything:
    /// a restart sends the bytes her engine holds instead of a head re-read from the live surface.
    /// Only a faculty holding neither resumes (a fresh one after a restart). A requested adoption
    /// (`persona/hands/adopt`) is honoured after, so it replaces what was resumed (`adoption.rs`).
    pub(in super::super) fn resume_custody(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        self.resume_from_checkpoint(memory);
        if let Some(by) = memory.take_hands_adoption() {
            self.begin_adoption(by);
        }
    }

    /// Her view and head as her checkpoint kept them, once per faculty.
    fn resume_from_checkpoint(&self, memory: &WorkingMemory) {
        let mut view = self.context_view.lock();
        if view.is_some() || self.tools_epoch.lock().is_some() || self.hands_refused.lock().is_some() {
            return;
        }
        let (mut restored, hands) = memory.resumed_custody();
        let refusal = match hands {
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
                None
            }
            HandsCustody::Corrupt(corrupt) => Some(HandsRefusal::Corrupt(corrupt)),
            // A view with no head beside it is legacy data (checkpointed before heads were): nothing
            // says which schemas her engine holds, so it is dropped, never adopted warm. Her memory
            // keeps it no longer, and her next act opens a new epoch (one prefill), not a refusal.
            HandsCustody::Unrecorded => {
                if restored.take().is_some() {
                    continuum_base::probe!(
                        class = "tools.custody.legacy_view_dropped",
                        persona = %self.persona_name,
                        "a view checkpointed with no head is legacy data: dropped, and her next act opens a new epoch"
                    );
                    memory.release_custody();
                }
                None
            }
        };
        if let Some(refusal) = refusal {
            continuum_base::probe!(
                class = "tools.custody.refused",
                persona = %self.persona_name,
                reason = %refusal.reason(),
                "her head's custody cannot be resumed: her view is kept and her acts wait for persona/hands/adopt"
            );
            *self.hands_refused.lock() = Some(refusal);
        }
        *view = restored;
    }

    /// Why her acts are refused while her head waits for adoption, or `None`.
    pub(in super::super) fn hands_refusal(&self) -> Option<String> {
        self.hands_refused.lock().as_ref().map(HandsRefusal::reason)
    }

    /// Her held head as this act offers it: none while an adoption waits to freeze a new one.
    pub(in super::super) fn held_hands(&self) -> Option<crate::cognition::hands_custody::HeldHands> {
        if self.adopting.lock().is_some() {
            return None;
        }
        self.tools_epoch.lock().clone()
    }

    /// A newly frozen head at `generation` completes a waiting adoption: what it replaced is kept.
    pub(in super::super) fn complete_adoption(&self, generation: u64) {
        let Some(pending) = self.adopting.lock().take() else {
            return;
        };
        continuum_base::probe!(
            class = "tools.custody.adopted",
            persona = %self.persona_name,
            by = %pending.by,
            generation = generation,
            "her head was adopted from the live surface as a new epoch; the head it replaced is kept"
        );
        if let Some(memory) = self.working_memory.as_deref() {
            memory.record_superseded(pending.superseded(generation));
        }
    }

    /// Her held verbs whose live contract she has not been told, into her memory for this act's
    /// notices (`context_streams::tool_contracts`), before her view advances.
    pub(in super::super) fn observe_contracts(&self) {
        let Some(memory) = self.working_memory.as_deref() else {
            return;
        };
        let changes = self.held_hands().map_or_else(Vec::new, |held| held.changes(|name| self.surface.live_contract(name))); // map_or_else: no held head holds no contract to change
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

    /// Her head's custody as this faculty holds it now: a refused one as it was, else her held head.
    fn hands_custody(&self) -> HandsCustody {
        if let Some(refusal) = self.hands_refused.lock().as_ref() {
            return refusal.custody();
        }
        // An adoption under way keeps what it replaces until her new head freezes (`adoption.rs`).
        if let Some(pending) = self.adopting.lock().as_ref() {
            return pending.prior.clone();
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
