//! An adoption at her act (card 9cf06ade): from the live surface, a new epoch frozen as this act
//! sends it; from a captured request, only where this act sends exactly what the capture sent
//! (`crate::cognition::capture_adoption`). A capture adoption holds the head this act's live
//! selection froze uncommitted until the act's whole payload is compared, then commits it over
//! the custody it was asked over under the one lock, or refuses with where it differs and keeps
//! her refusal and her view.

use super::*;
use crate::cognition::capture_adoption::{act_matches, frontier_matches, ActPayload, AdoptionRequest, CaptureMismatch};
use crate::cognition::hands_custody::{HandsCustody, HandsRefusal, HeldHands, PendingAdoption};

impl LlmDeliberationFaculty {
    /// Honour an adoption asked for: what it replaces is kept (a refused head's record, else her
    /// held head), and her old head keeps its generation until the new one freezes after it. A
    /// capture adoption first requires her accepted frontier to be the captured messages.
    pub(super) fn begin_adoption(&self, request: AdoptionRequest) {
        let lifted = self.hands_refused.lock().take();
        *self.capture_refused.lock() = None;
        let reason = lifted.as_ref().map_or_else(|| "an explicit request".to_string(), HandsRefusal::reason); // map_or_else: nothing was refused
        let prior = lifted.as_ref().map_or_else(|| self.tools_epoch.lock().clone().map_or(HandsCustody::Unrecorded, HandsCustody::Held), HandsRefusal::custody); // map_or_else: a head that was not refused is replaced as held; map_or: none held is none
        let by = request.by().to_string();
        let capture = match request {
            AdoptionRequest::Live { .. } => None,
            AdoptionRequest::Capture { head, .. } => {
                let frontier = self.context_view.lock().as_ref().map_or(Err(CaptureMismatch::NoAcceptedFrontier), |view| frontier_matches(view, &head.request.messages)); // map_or: no view has no frontier
                if let Err(mismatch) = frontier {
                    self.refuse_capture(lifted, mismatch);
                    return;
                }
                Some(head)
            }
        };
        continuum_base::probe!(
            class = "tools.custody.adoption_requested",
            persona = %self.persona_name,
            by = %by,
            reason = %reason,
            from_capture = capture.is_some(),
            "her head is adopted at her next frozen epoch, at an explicit request; what it replaces is kept"
        );
        *self.adopting.lock() = Some(PendingAdoption { prior, reason, by, capture, candidate: None });
    }

    /// A head this act's live selection froze, held uncommitted when a capture adoption must first
    /// compare the act's whole payload; returned when there is none to compare.
    pub(super) fn hold_for_capture(&self, hands: HeldHands) -> Option<HeldHands> {
        let mut adopting = self.adopting.lock();
        match adopting.as_mut() {
            Some(pending) if pending.capture.is_some() => {
                pending.candidate = Some(hands);
                None
            }
            _ => Some(hands),
        }
    }

    /// Settle a capture adoption against what this act sends: `None` when none waits on this act,
    /// else whether it committed (`Ok`) or was refused with where it differs (`Err`).
    pub(super) fn settle_capture_adoption(&self, act: &ActPayload<'_>) -> Option<Result<(), CaptureMismatch>> {
        let mut pending = {
            let mut adopting = self.adopting.lock();
            if !matches!(adopting.as_ref(), Some(pending) if pending.capture.is_some() && pending.candidate.is_some()) {
                return None;
            }
            adopting.take()?
        };
        let (Some(head), Some(candidate)) = (pending.capture.clone(), pending.candidate.take()) else {
            return None;
        };
        let committed = act_matches(act, &head.request).and_then(|()| {
            let memory = self.working_memory.as_deref().ok_or(CaptureMismatch::NoMemory)?;
            memory.adopt_custody_if(&head.binding.expected_custody_sha256, HandsCustody::Held(candidate.clone()))
        });
        Some(match committed {
            Ok(()) => {
                let generation = candidate.generation;
                *self.tools_epoch.lock() = Some(candidate);
                continuum_base::probe!(
                    class = "tools.custody.adopted_from_capture",
                    persona = %self.persona_name,
                    by = %pending.by,
                    cursor = %head.binding.cursor,
                    generation = generation,
                    "her head was adopted from the captured request it matches byte for byte: the engine's KV for it is kept"
                );
                if let Some(memory) = self.working_memory.as_deref() {
                    memory.record_superseded(pending.superseded(generation));
                }
                self.keep_custody();
                Ok(())
            }
            Err(mismatch) => {
                let lifted = HandsRefusal::of(&pending.prior);
                self.refuse_capture(lifted, mismatch.clone());
                Err(mismatch)
            }
        })
    }

    /// A capture adoption that does not match: her refusal is restored as it was, her view is kept,
    /// and where it differs is shown with her refusal until the next adoption is asked.
    fn refuse_capture(&self, lifted: Option<HandsRefusal>, mismatch: CaptureMismatch) {
        continuum_base::probe!(
            class = "tools.custody.capture_refused",
            persona = %self.persona_name,
            mismatch = %mismatch,
            "an adoption from a captured request does not match what she sends: refused, her refusal and view kept"
        );
        *self.hands_refused.lock() = lifted;
        *self.capture_refused.lock() = Some(mismatch);
        self.keep_custody();
    }
}
