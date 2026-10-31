//! Per-call admission for the concrete adapter target: the pre-gate hold defer, then dispatch.
use super::*;
use crate::ai::adapter::GenerationAdmissionOwner;

pub(super) enum GenerationAdmission {
    AdapterOwned,
    SharedLocal,
}

impl LlmDeliberationFaculty {
    pub(super) async fn admit_generation(
        &self,
        ws: &Workspace,
        request: &TextGenerationRequest,
        owner: GenerationAdmissionOwner,
    ) -> Option<GenerationAdmission> {
        admit_target(owner, self.admit_local_generation(ws, request)).await
    }

    async fn admit_local_generation(&self, ws: &Workspace, request: &TextGenerationRequest) -> Option<GenerationAdmission> {
        // MEASURED-HOLD DEFER, ABOVE the admission gates (2026-08-29). The adapter's
        // own defer sits BELOW acquire_serving_lane + the prefill slot, so a
        // Background generation (dream-belief-review, glass-boxed live) would take
        // the lane permits, THEN park on the hold — starving the measured work's
        // own next generation of the very permits it needs to finish and release
        // the hold. Priority inversion; the solve's tick 2 waited forever on a
        // permit a deferred dream held. Defer FIRST, holding nothing; the adapter
        // seam stays as the backstop for entry paths that skip these gates.
        continuum_base::probe!(
            class = "delib.defer.entered",
            persona = %self.persona_name,
            directed = ws.directed_at_self(),
            attention = ?ws.attention,
            "at the pre-gate hold defer"
        );
        {
            let class = deliberation_slot_class(request.purpose.as_deref(), ws.attention);
            crate::inference::measured_hold::defer_while_held(
                class,
                Some(self.persona_id),
                request.purpose.as_deref(),
            )
            .await;
        }
        continuum_base::probe!(
            class = "delib.defer.passed",
            persona = %self.persona_name,
            "past the pre-gate defer — admission gates next"
        );
        // No lane is taken here. The adapter's turn admission and the engine's slot pool are
        // the one queue in front of the engine: a second count of the same slots, held for
        // the whole model call, was a serial gate (one of four slots reserved for directed
        // calls idled while the fourth resident waited a median 25.7 s, p90 131 s; M5,
        // 2026-10-28, build 7ace74020). Ordering when minds outnumber slots belongs to the
        // slot pool, where a mind's KV is placed (SERVING-ENGINE §1a).
        crate::cognition::health_ledger::note_lane_granted_to(self.persona_id);
        Some(GenerationAdmission::SharedLocal)
    }
}

/// The local future is lazy: independent targets never poll or enqueue on it.
/// Keeping selection outside that future also releases partial local admission
/// on cancellation through the existing permit destructors.
pub(super) async fn admit_target(
    owner: GenerationAdmissionOwner,
    local: impl std::future::Future<Output = Option<GenerationAdmission>>,
) -> Option<GenerationAdmission> {
    match owner {
        GenerationAdmissionOwner::Adapter => Some(GenerationAdmission::AdapterOwned),
        GenerationAdmissionOwner::SharedLocal => local.await,
    }
}
