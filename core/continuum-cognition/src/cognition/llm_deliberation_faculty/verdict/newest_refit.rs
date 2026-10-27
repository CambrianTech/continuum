//! A capacity refusal answered in the same turn, when what it measured shrinks her newest result.
//!
//! Aris (the M5, 2026-10-26/27): her newest result was a 340,832-char `work/submission` answer.
//! `fit_newest_result` (#5505) shows such a result at its summary plus handle, but only at a
//! MEASURED density; right after a restart there is none, the bytes/3 estimate says the result
//! fits, and the request goes whole. The engine refused it ("request (214671 tokens) exceeds the
//! slot's available context size (187136 tokens)") at 03:54, 04:12 and 04:29Z, once per restart,
//! growing 195k, 206k, 215k. The refusal's count was stored as her density, but the corrective
//! refit only drops OPTIONAL input, so it refused too, she parked waiting for capacity to change,
//! and the density the refusal had just measured was never used: no act after 22:22Z.
//!
//! So: the engine's typed count (`InferenceError::ContextExceeded`, parsed once in
//! continuum-engine) prices her view again at that density. When her newest result was whole in
//! the refused view and now comes in smaller, that view is resent ONCE. If it is refused too she
//! parks as before, loudly, and is never resent a second time.

use super::*;

impl LlmDeliberationFaculty {
    /// Her view again, priced at the density the refusal just measured, when that shows her
    /// newest result smaller than the refused request carried it. `None` when the refusal is not
    /// about her newest result (it was already reduced, or still fits whole): the optional-input
    /// refit (`refit_rejected_prompt`) answers that one.
    pub(super) fn refit_newest_after_refusal(
        &self,
        ws: &Workspace,
        fit_window: u32,
        calibration: PromptCalibration,
        visual: Option<&ActiveVisualFeedback>,
        refused: &DeliberationPromptView,
        requested: u32,
        available: u32,
    ) -> Option<DeliberationPromptView> {
        if refused.newest_result_reduced {
            return None;
        }
        let refit = self.prompt_view_with_visual_feedback(ws, fit_window, calibration, visual);
        if !refit.newest_result_reduced {
            return None;
        }
        continuum_base::probe!(
            class = "delib.newest_result.refusal_refit",
            persona = %self.persona_name,
            requested_tokens = requested,
            available_tokens = available,
            refused_estimate = refused.estimated_prompt_tokens,
            refit_estimate = refit.estimated_prompt_tokens,
            "the engine refused her request; at the density it measured her newest result \
             comes in at its summary with its handle: resending once"
        );
        Some(refit)
    }

    /// The resend at her measured density was refused too: she parks on the refusal, never
    /// resent again this turn. Loud, because a summary that still cannot fit is a defect in
    /// what the window leaves her, not a wait.
    pub(super) fn newest_refit_refused(
        &self,
        requested: u32,
        available: u32,
        allowance: u32,
    ) -> crate::cognition::workspace::PromptCapacityError {
        continuum_base::probe!(
            class = "delib.newest_result.refit_refused",
            persona = %self.persona_name,
            requested_tokens = requested,
            available_tokens = available,
            allowance,
            "her newest result at its summary was refused too: parking on the refusal, no second resend"
        );
        crate::cognition::workspace::PromptCapacityError {
            required_tokens: (requested as usize).saturating_add(allowance as usize),
            budget_tokens: available as usize,
        }
    }
}
