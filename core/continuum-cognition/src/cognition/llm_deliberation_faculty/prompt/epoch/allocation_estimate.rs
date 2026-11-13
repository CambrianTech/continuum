//! Her view's ALLOCATION ESTIMATE of a block ([`continuum_context::cost::Estimate`]): what decides what
//! her view keeps within each grant.
//!
//! The window's one allocation grants her view's sources in its tokens, so her view decides what
//! fits each grant by pricing every level it could render with the estimate the final prompt is
//! priced with ([`PromptCalibration::estimate`]: her text estimate plus the engine-measured
//! template share of the message's kind, at her calibration). It never converts a grant into
//! characters at a source's average tokens per character: long old turns are cheap per character
//! and short new ones dear, and the average bought her start more than its grant (Codex,
//! `a_view_start_keeps_its_token_grant_with_uneven_message_sizes`).
//!
//! An estimate, never a count: it is built to err high (each kind's template share at its highest
//! observation, `prompt/template.rs`), it claims no exact token count and no exact token prefix,
//! and dispatch never routes through it: the engine's exact quote of the finalized body stays the
//! authority there (`lane_send::PromptQuote`).

use continuum_context::cost::Estimate;

use super::*;

/// Each block at its allocation estimate ([`PromptCalibration::estimate`]) at her calibration
/// ([`PromptCalibration::charge`]), in the allocation's tokens. Without the template's share a short
/// turn cost nothing, and a start kept twenty of them against a 32-token grant (Codex: 100 > 32). A
/// charge rounds up once per prompt; charged per block it errs high by under one more token a block.
#[derive(Clone, Copy)]
pub(in crate::cognition::llm_deliberation_faculty) struct AllocationEstimate(pub(in crate::cognition::llm_deliberation_faculty) PromptCalibration);

impl AllocationEstimate {
    /// Her estimate with no calibration: for a start that grants every source its whole, where the
    /// unit decides nothing.
    pub(in crate::cognition::llm_deliberation_faculty) fn uncalibrated() -> Self {
        Self(PromptCalibration::default())
    }
}

impl Estimate<ContextMessage> for AllocationEstimate {
    fn block(&self, message: &ContextMessage) -> usize {
        self.0.charge(self.0.estimate(&message.to_chat()))
    }

    /// A body evicted from RAM is priced from its measured size, never as the pointer resident in
    /// its place, or the allocation would never grant room to see it (FOCUS-CONTRACT rule 7): the
    /// resident message's estimate plus the stored characters beyond it at the guard rate.
    fn stored(&self, resident: &ContextMessage, chars: usize) -> usize {
        let beyond = chars.saturating_sub(resident.text.chars().count());
        self.0.charge(self.0.estimate(&resident.to_chat()) + beyond / continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN)
    }
}
