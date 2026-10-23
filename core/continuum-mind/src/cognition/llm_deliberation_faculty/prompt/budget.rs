//! The one budget's invariant, checked once where the final messages are fixed (goal 7).
//!
//! Every part of the prompt is a claim in the window's one allocation
//! ([`crate::persona::rag_budget::flex_allocate`]), each at its charged price, so
//! `charge(prompt) + completion_reserve <= window` holds by construction and her view fits
//! without a fit step dropping it. When the assembled prompt says otherwise, the allocation
//! is wrong: the act fails loud as a budget fault with each claim's grant beside its actual
//! cost, and nothing is sent. No fit step can make it comply by dropping her view: the fit
//! never receives the view (`view.rs`), the head clip the canary prompt passed by (Benchy on
//! the M5, 2026-10-23).

use super::*;

/// Where the assembled prompt's parts sit and what the allocation granted each, in the
/// allocation's unit (charged tokens).
pub(in super::super) struct BudgetLedger<'a> {
    pub context_window: u32,
    pub calibration: PromptCalibration,
    pub completion_reserve: usize,
    pub system: &'a str,
    pub tool_tokens: usize,
    pub messages: &'a [ChatMessage],
    /// Her view leads `messages`, whole: the fit never cuts it (`view.rs`).
    pub view_len: usize,
    /// The grounding block range, right after her view.
    pub grounding_len: usize,
    pub framing_grant: u32,
    pub required_grant: u32,
    pub history_grant: u32,
    pub grounding_grant: u32,
    /// Her view's sources' grants plus the room no claim took that she may append into.
    pub view_grant: u32,
}

impl LlmDeliberationFaculty {
    /// `Some(fault)` when the assembled prompt breaks the one budget; the caller sends nothing.
    pub(in super::super) fn budget_violation(&self, ledger: &BudgetLedger<'_>) -> Option<String> {
        let charge = |tokens: usize| ledger.calibration.charge(tokens);
        let estimated = Self::framing_cost(ledger.system)
            .saturating_add(Self::messages_cost(ledger.messages))
            .saturating_add(ledger.tool_tokens);
        let charged = charge(estimated).saturating_add(ledger.completion_reserve);
        if charged <= ledger.context_window as usize {
            return None;
        }
        let view_end = ledger.view_len.min(ledger.messages.len());
        let grounding_end = view_end.saturating_add(ledger.grounding_len).min(ledger.messages.len());
        let view_cost = charge(Self::messages_cost(&ledger.messages[..view_end]));
        let grounding_cost = charge(Self::messages_cost(&ledger.messages[view_end..grounding_end]));
        let tail_cost = charge(Self::messages_cost(&ledger.messages[grounding_end..]));
        let framing_cost = charge(Self::framing_cost(ledger.system).saturating_add(ledger.tool_tokens));
        continuum_base::probe!(
            class = "delib.budget.violated",
            persona = %self.persona_name,
            context_window = ledger.context_window,
            charged_with_reserve = charged,
            completion_reserve = ledger.completion_reserve,
            framing_grant = ledger.framing_grant,
            framing_cost,
            view_grant = ledger.view_grant,
            view_cost,
            grounding_grant = ledger.grounding_grant,
            grounding_cost,
            tail_grant = ledger.required_grant.saturating_add(ledger.history_grant),
            tail_cost,
            "the assembled prompt breaks the one budget: some part of it is not a charged \
             claim; the act fails and nothing is sent"
        );
        Some(format!(
            "prompt budget violated: charged prompt + reserve {charged} against window {}",
            ledger.context_window
        ))
    }
}
