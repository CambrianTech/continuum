//! A capacity refusal answered in the same turn, when what it measured shrinks her newest result.
//!
//! Aris (the M5, 2026-10-26/27): her newest result was a 340,832-char `work/submission` answer.
//! `fit_newest_result` shows such a result as its first page, but only at a
//! MEASURED density; right after a restart there is none, the bytes/3 estimate says the result
//! fits, and the request goes whole. The engine refused it ("request (214671 tokens) exceeds the
//! slot's available context size (187136 tokens)") at 03:54, 04:12 and 04:29Z, once per restart,
//! growing 195k, 206k, 215k. The refusal's count was stored as her density, but the corrective
//! refit only drops OPTIONAL input, so it refused too, she parked waiting for capacity to change,
//! and the density the refusal had just measured was never used: no act after 22:22Z.
//!
//! So: the engine's typed count (`InferenceError::ContextExceeded`, parsed once in
//! continuum-engine) prices her view again. When her newest result was whole in the refused view
//! and now comes in smaller, that view is resent ONCE. If it is refused too she parks as before,
//! loudly, and is never resent a second time.
//!
//! The price is the refusal's EXCESS charged to her newest result, not the request's averaged
//! density. Deployed in 6417d15be the averaged ratio did not fire for her (05:20:57Z): 222,348
//! measured for 148,275 estimated is 1.50 over the whole request, and at 1.50 her result's
//! ~113.6k estimate still fit the ~114k the window left it, so it stayed whole and was refused
//! again. Her framing, tools and prose are priced near their estimate; her escaped JSON is not
//! (about 1.8 chars a token against the estimate's 3). Charging the whole excess to the result
//! (every other part at its estimate) prices it at what it can at most cost given the count, so
//! it stays whole only when dropping optional input covers the deficit. Overpricing errs the safe
//! way: shown as its first page she loses nothing, reading on is named.

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
        // The higher of the two prices: the averaged ratio may already charge the result more.
        let calibration = match newest_result_price(refused, requested) {
            Some(price) if denser(price, calibration) => price,
            _ => calibration,
        };
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
            newest_result_estimate = refused.newest_result_tokens,
            refit_estimate = refit.estimated_prompt_tokens,
            "the engine refused her request; at the density it measured her newest result \
             comes in as its first page: resending once"
        );
        Some(refit)
    }

    /// The resend at her measured density was refused too: she parks on the refusal, never
    /// resent again this turn. Loud, because a first page that still cannot fit is a defect in
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
            "her newest result as its first page was refused too: parking on the refusal, no second resend"
        );
        crate::cognition::workspace::PromptCapacityError {
            required_tokens: (requested as usize).saturating_add(allowance as usize),
            budget_tokens: available as usize,
        }
    }
}

/// Her newest result priced by the refusal: the engine's count less every other part of the
/// refused request at its estimate, over the result's own estimate. `None` without a result, or
/// when the count leaves it nothing (the estimate already covered it).
fn newest_result_price(refused: &DeliberationPromptView, requested: u32) -> Option<PromptCalibration> {
    let others = refused.estimated_prompt_tokens.saturating_sub(refused.newest_result_tokens);
    let measured = u32::try_from((requested as usize).saturating_sub(others)).ok()?;
    PromptCalibration::observed(refused.newest_result_tokens, measured, Some(refused.input_identity))
}

/// Whether `a` prices every estimated token higher than `b` (ratios compared exactly, in u128).
fn denser(a: PromptCalibration, b: PromptCalibration) -> bool {
    u128::from(a.measured.get()) * b.estimated.get() as u128 > u128::from(b.measured.get()) * a.estimated.get() as u128
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::ResultCustody;
    use crate::cognition::working_memory::WorkingMemory;

    // What this catches: Sahar's 2026-11-08 19:48 refusal attributed the excess
    // to her newest result, then charged that result-only density to every source.
    // Her unrelated framing and tools must retain the whole-prompt calibration.
    #[test]
    fn newest_result_refit_does_not_reprice_unrelated_sources() {
        let memory = Arc::new(WorkingMemory::new(8));
        let whole = (0..12_000).map(|i| format!("result line {i}: retained evidence\n")).collect::<String>();
        memory.record_receipt_typed(&[], &whole, None,
            ResultCustody::Whole { handle: "sahar-refit-evidence".into() });
        let window = 187_136;
        let faculty = LlmDeliberationFaculty::new(
            Uuid::new_v4(), "Sahar", "You are Sahar.",
            Arc::new(HeuristicInferenceAdapter::new()),
        ).with_context_window(window).with_working_memory(memory);
        let workspace = Workspace::new("Inspect the retained evidence.");
        let first = faculty.prompt_view_with_visual_feedback(
            &workspace, window, PromptCalibration::default(), None,
        );
        assert!(!first.newest_result_reduced, "fixture begins with a whole result");
        assert!(first.newest_result_tokens > 0);
        let requested = (first.estimated_prompt_tokens * 2) as u32;
        let Some(average) = PromptCalibration::observed(
            first.estimated_prompt_tokens, requested, Some(first.input_identity),
        ) else { panic!("fixture has positive counts"); };
        let Some(refit) = faculty.refit_newest_after_refusal(
            &workspace, window, average, None, &first, requested, window,
        ) else { panic!("denser newest result must be paged"); };
        assert_eq!(refit.pricing.density(), average.density(),
            "the result-only surcharge must not replace the shared prompt price");
    }
}
