//! Her view's price of a block ([`continuum_context::cost::Cost`]): the final prompt's own cost.
//!
//! The window's one allocation grants her view's sources in its tokens, so her view decides what
//! fits each grant by pricing every level it could render with the function the final prompt is
//! charged with ([`LlmDeliberationFaculty::message_cost`], at her calibration). It never converts a
//! grant into characters at a source's average tokens per character: long old turns are cheap per
//! character and short new ones dear, and the average bought her start more than its grant
//! (Codex, `a_view_start_keeps_its_token_grant_with_uneven_message_sizes`).

use continuum_context::cost::Cost;

use super::*;

/// Each block at what the final prompt charges it: its estimate ([`LlmDeliberationFaculty::message_cost`])
/// at her calibration ([`PromptCalibration::charge`]), the allocation's tokens. A charge rounds up
/// once per prompt; charged per block it errs high by under one token a block, never low, so what
/// the view keeps within a grant is within it charged as one prompt.
#[derive(Clone, Copy)]
pub(in crate::cognition::llm_deliberation_faculty) struct ViewCost(pub(in crate::cognition::llm_deliberation_faculty) PromptCalibration);

impl ViewCost {
    /// Her estimate alone (no calibration measured): for a start that grants every source its whole,
    /// where the unit decides nothing.
    pub(in crate::cognition::llm_deliberation_faculty) fn estimate() -> Self {
        Self(PromptCalibration::default())
    }
}

impl Cost<ContextMessage> for ViewCost {
    fn block(&self, message: &ContextMessage) -> usize {
        self.0.charge(LlmDeliberationFaculty::message_cost(&message.to_chat()))
    }

    /// A body evicted from RAM is priced from its measured size, never as the pointer resident in
    /// its place, or the allocation would never grant room to see it (FOCUS-CONTRACT rule 7): the
    /// resident message's cost plus the stored characters beyond it at the guard rate.
    fn stored(&self, resident: &ContextMessage, chars: usize) -> usize {
        let beyond = chars.saturating_sub(resident.text.chars().count());
        self.0.charge(LlmDeliberationFaculty::message_cost(&resident.to_chat()) + beyond / continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN)
    }
}
