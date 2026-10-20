use super::*;

/// A pass is bounded at this multiple of her measured p90 turn (never below the act's
/// cap). A runaway bound, not a budget: it binds only on a loop (Cormac on #4815: a loop
/// must not hold the lane for minutes; twice her p90 sits far above the one-in-ten tail
/// a p90 lets through).
const PASS_RUNAWAY_MULTIPLE: u32 = 2;

/// THE REPLY'S CLAIM on the served window, and its runaway bound. Pure.
///
/// The reply is one claimant in the same flexbox allocation as framing, required input,
/// grounding and history ([`crate::persona::rag_budget::flex_allocate`]) — never a share
/// carved off the top by its own rule. The carve-off is what cut Benchy's first write
/// mid-call (2026-10-18 02:10Z): her reserve was twice her read-only peak (~5.4k), her
/// think was capped near 2k (`delib.emission.think_budget_hit` at 1,592–1,746), and the
/// write needed more than either, on a window with room to spare.
///
/// - floor = min: the smallest usable reply
///   ([`LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS`]); an ACT floors at her measured
///   answer channel (the tool call itself). The caller caps it so required input always
///   fits — the reply never takes from required content.
/// - max: what she is expected to write — her measured need, or the reply-sized cold
///   prior when unmeasured; at least twice a pending cut's peak (the cut write gets its
///   room back on the retry). Never past the runaway bound.
/// - priority: an ambient pass 4, a directed pass 6, an act 8, an act on live held work
///   or after a pending cut 10 — the weight the reply grows by against history (5) and
///   grounding (6).
///
/// Why the floor is the min and not the whole want: the allocator's min pass runs for
/// EVERY claim before any claim grows, so a min of the whole want would pre-empt history
/// and grounding regardless of priority — on a 2,048 window an ambient pass's 2,048-token
/// prior took the entire window and the presence framing dropped out of the prompt
/// (`directed_turn_withholds_the_silence_escape`). Above the floor the reply GROWS by its
/// priority, so priority is what decides.
///
/// The runaway bound: [`LlmDeliberationFaculty::ACT_OUTPUT_CAP`] for an act; for a pass,
/// twice her measured turn, never below the act's cap.
pub(super) fn reply_claim(
    kind: TurnKind,
    need: Option<super::super::working_set::OutputNeed>,
    pending_peak: u32,
    live_work: bool,
    directed: bool,
) -> (crate::persona::rag_budget::RagSourceBudget, u32) {
    let cap = LlmDeliberationFaculty::ACT_OUTPUT_CAP;
    let runaway = match kind {
        TurnKind::Act => cap,
        TurnKind::Pass => cap.max(need.map_or(0, |n| n.total().saturating_mul(PASS_RUNAWAY_MULTIPLE))),
    };
    let floor = match (kind, need) {
        (TurnKind::Act, Some(n)) => n.answer.max(LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS),
        _ => LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS,
    }
    .min(runaway);
    let wanted = need
        .map_or(LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS, |n| n.total()) // map_or: unmeasured = a REPLY-sized prior, never a window fraction
        .max(pending_peak.saturating_mul(2))
        .max(floor)
        .min(runaway);
    let priority = match kind {
        TurnKind::Act if live_work || pending_peak > 0 => 10,
        TurnKind::Act => 8,
        TurnKind::Pass if directed => 6,
        TurnKind::Pass => 4,
    };
    (
        crate::persona::rag_budget::RagSourceBudget {
            source_id: FLEX_REPLY.to_owned(),
            priority,
            floor_tokens: floor,
            min_tokens: floor,
            max_tokens: wanted,
            required: true,
        },
        runaway,
    )
}
