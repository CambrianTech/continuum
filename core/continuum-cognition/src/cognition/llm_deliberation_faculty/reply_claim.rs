use super::*;

/// THE REPLY'S CLAIM on her window: one claimant in the window's one allocation
/// ([`crate::cognition::rag_budget::flex_allocate`]) beside framing, required input, tools,
/// grounding and her view. Pure.
///
/// A claim is what she is expected to write, measured; it is not a reservation for the worst
/// case. Its grant is carried as the request's `max_tokens`, so her own evidence bounds her output
/// and her latency. The cost accepted: a write larger than any she has measured is cut once (a
/// turn lost; `delib.emission.cut_mid_answer` counts it), recorded at double, and the next claim
/// grows. A cap here instead reserved 12,288 tokens for acts that measured 809-1,641 and starved
/// her view of the reads she had just made, so every act re-read them (the M5, 2026-10-30,
/// ONE-BUDGET.md slice 6).
///
/// - floor = min: an act's measured answer channel (the tool call itself), else the smallest
///   usable reply ([`LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS`]). The caller caps it so
///   required input always fits: the reply never takes from required content.
/// - max: her measured need (reasoning + answer), or the reply-sized cold prior until it is
///   measured; at least a pending cut's measured peak, retained until that concern settles
///   (the ordinary need decays through the short reads between, and the retried write would
///   be claimed small and cut again). The peak is evidence the registry already recorded
///   (a cut channel counts double there); it is demand, never doubled again here: what she
///   is granted is the one allocation's decision (Codex on slice 6).
/// - priority: an ambient pass 4, a directed pass 6, an act 8, an act on live held work or
///   after a pending cut 10:
///   the weight the reply grows by against history (5) and grounding (6).
///
/// Why the floor is the min and not the whole want: the allocator's min pass runs for EVERY
/// claim before any claim grows, so a min of the whole want would pre-empt history and
/// grounding regardless of priority (on a 2,048 window an ambient pass's prior took the
/// entire window and the presence framing dropped out, `directed_turn_withholds_the_silence_escape`).
pub fn reply_claim(
    kind: TurnKind,
    need: Option<super::super::working_set::OutputNeed>,
    pending_peak: u32,
    live_work: bool,
    directed: bool,
) -> crate::cognition::rag_budget::RagSourceBudget {
    let floor = match (kind, need) {
        (TurnKind::Act, Some(n)) => n.answer.max(LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS),
        _ => LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS,
    };
    let wanted = need
        .map_or(LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS, |n| n.total()) // map_or: unmeasured = a REPLY-sized prior, never a window fraction
        .max(pending_peak)
        .max(floor);
    let priority = match kind {
        TurnKind::Act if live_work || pending_peak > 0 => 10,
        TurnKind::Act => 8,
        TurnKind::Pass if directed => 6,
        TurnKind::Pass => 4,
    };
    crate::cognition::rag_budget::RagSourceBudget {
        source_id: FLEX_REPLY.to_owned(),
        priority,
        floor_tokens: floor,
        min_tokens: floor,
        max_tokens: wanted,
        required: true,
    }
}
