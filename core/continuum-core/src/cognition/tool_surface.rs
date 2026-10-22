//! A turn's tool surface: WHY it is what it is, and whether it alone outweighs the prompt
//! the lane can afford. Its own owner, out of the deliberation faculty (12,000+ lines).

/// WHY a turn's tool surface is what it is.
///
/// Carried alongside the cost because the cost alone cannot be interpreted: see
/// `SelectedSurface::demand_tokens` (in the deliberation faculty), where two of these arms want different
/// numbers from the same selection.
///
/// `Default` is `NoTools` so a default `ContextComposition` is coherent: zero
/// `tool_tokens` because no tools were offered, not a reason that disagrees with its cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SurfaceReason {
    /// She has no native specs at all — nothing to offer, nothing to price.
    #[default]
    NoTools,
    /// The whole registry fit its share of the served window and was offered.
    Full,
    /// A WORK turn takes her hands for FOCUS. The full surface was never wanted
    /// on this turn, so nothing was truncated and nothing was lost.
    HandsForWork,
    /// The full surface did NOT fit its share of the served window. She wanted it
    /// and the window refused it — this arm is a truncation, the others are not.
    HandsForBudget,
    /// The ROOM's recipe declares affordances, and they selected the surface (S1).
    /// Not a truncation: the recipe is the rule, and a rule is never a loss —
    /// `demand_tokens` prices what was sent. The discovery pair rides along so a
    /// withheld verb stays one `commands/list` away.
    RoomAffordances,
}

/// The prompt this lane affords (served window bounded by prefill t/s x TTFT), read beside
/// what the tool surface and framing cost. Emits `delib.tool_surface.over_affordable` when
/// they alone exceed it. Observation only: nothing here changes what is sent (card 3026b83e,
/// step 1). Returns the affordable token count for the turn's receipt.
pub(crate) fn over_affordable_receipt(
    persona: &str,
    surface_reason: SurfaceReason,
    tool_tokens: usize,
    framing_tokens: usize,
    context_window: u32,
) -> Option<usize> {
    let affordable_tokens =
        super::context_budget::ContextBudget::from_window(context_window).affordable_tokens();
    if let Some(affordable) = affordable_tokens.filter(|a| framing_tokens + tool_tokens > *a) {
        continuum_base::probe!(
            class = "delib.tool_surface.over_affordable",
            persona = %persona,
            surface_reason = ?surface_reason,
            tool_tokens,
            framing_tokens,
            affordable_tokens = affordable,
            context_window,
            "the tool surface and framing alone cost more than the prompt this lane can prefill inside its TTFT budget"
        );
    }
    affordable_tokens
}
