//! An adoption at her act (card 9cf06ade, `persona/hands/adopt`): a new epoch of her head frozen
//! from the live surface as this act sends it, the explicit repair of a head whose record does not
//! decode. What it replaces is kept.

use super::*;
use crate::cognition::hands_custody::{HandsCustody, HandsRefusal, PendingAdoption};

impl LlmDeliberationFaculty {
    /// Honour an adoption `by` asked for: what it replaces is kept (a refused head's record, else her
    /// held head), and her old head keeps its generation until the new one freezes after it.
    pub(super) fn begin_adoption(&self, by: String) {
        let lifted = self.hands_refused.lock().take();
        let reason = lifted.as_ref().map_or_else(|| "an explicit request".to_string(), HandsRefusal::reason); // map_or_else: nothing was refused
        let prior = lifted.as_ref().map_or_else(|| self.tools_epoch.lock().clone().map_or(HandsCustody::Unrecorded, HandsCustody::Held), HandsRefusal::custody); // map_or_else: a head that was not refused is replaced as held; map_or: none held is none
        continuum_base::probe!(
            class = "tools.custody.adoption_requested",
            persona = %self.persona_name,
            by = %by,
            reason = %reason,
            "her head is adopted at her next frozen epoch, at an explicit request; what it replaces is kept"
        );
        *self.adopting.lock() = Some(PendingAdoption { prior, reason, by });
    }
}
