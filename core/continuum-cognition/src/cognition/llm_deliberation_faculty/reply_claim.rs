use super::*;

/// THE REPLY'S CLAIM on her window: one claimant in the window's one allocation
/// ([`continuum_context::allocation::allocate`]) beside framing, required input, tools,
/// grounding and her view. Pure.
///
/// A claim is what she is expected to write, measured; it is not a reservation for the worst
/// case. Its grant is what her view is sized beside; a pass carries it as the request's
/// `max_tokens`, so her own evidence bounds her output and her latency. An act's request carries
/// the room its prompt leaves instead (`request_allowance`): her answer is payload, and a claim
/// measured on reads cut every write (the M5, 2026-11-04). The cost accepted: a write larger than any she has measured is cut once (a
/// turn lost; `delib.emission.cut_mid_answer` counts it), recorded as a lower bound on her
/// answer, and the next claim's floor holds the median past it at once
/// (`OutputNeed::answer_room`). A cap here instead reserved 12,288 tokens for acts that measured
/// 809-1,641 and starved
/// her view of the reads she had just made, so every act re-read them (the M5, 2026-10-30,
/// ONE-BUDGET.md slice 6).
///
/// - floor = min: an act's measured output need as ONE claim, think plus answer
///   ([`OutputNeed::act_room`], raised by a recent cut), never the answer alone; an unmeasured
///   act reserves nothing ([`ActRoom::Unmeasured`], a state, not a number). A pass's floor is the
///   engine's one free cell, the least room a reply begins in. The allocation
///   caps it so required input always fits; an act whose need is capped is refused as the typed
///   `IrreducibleShortfall`, never sent with less (the M5, 2026-11-04: an answer-only floor left
///   caps of 72-89 tokens and 302 of 414 acts were cut mid-thought).
///
/// [`OutputNeed::act_room`]: super::super::working_set::OutputNeed::act_room
/// [`ActRoom::Unmeasured`]: super::super::working_set::ActRoom::Unmeasured
/// - max: her measured need (reasoning + answer), or the node's pooled need before her first
///   landed turn; on a node where nothing is measured at all, nothing (the request then carries
///   the room its prompt leaves, once: `request_allowance`). Never a guessed size; at least a pending cut's measured peak, retained until that concern settles
///   (the ordinary need decays through the short reads between, and the retried write would
///   be claimed small and cut again). The peak is evidence the registry already recorded
///   (the sum of the channels as measured); it is demand, never doubled here: what she is
///   granted is the one allocation's decision (Codex on slice 6).
/// - priority: an ambient pass 4, a directed pass 6, an act 8, an act on live held work or
///   after a pending cut 10:
///   the weight the reply grows by against history (5) and grounding (6).
///
/// Why the floor is the min and not the whole want: the allocator's min pass runs for EVERY
/// claim before any claim grows, so a min of the whole want would pre-empt history and
/// grounding regardless of priority (on a 2,048 window an ambient pass's prior took the
/// entire window and the presence framing dropped out, `directed_turn_withholds_the_silence_escape`).
/// The reply's top priority: an act on live held work or after a pending cut.
pub const REPLY_TOP_PRIORITY: u8 = 10;

pub fn reply_claim(
    kind: TurnKind,
    need: Option<super::super::working_set::OutputNeed>,
    pending_peak: u32,
    live_work: bool,
    directed: bool,
    time_term: Option<u32>,
) -> continuum_context::flex::FlexClaim {
    let floor = match kind {
        TurnKind::Act => super::super::working_set::ActRoom::of(need).needed().map_or(0, |room| room), // map_or: unmeasured reserves nothing in her name
        // The least room a reply begins in, the engine's one free cell: her measured need is her
        // want, which the allocation grows toward by priority, so a seat smaller than her answer
        // still serves the part it holds (a floor of her whole answer refused it outright).
        TurnKind::Pass => 1,
    };
    // A recent cut think raises the want toward its lower bound (`reasoning_at_least`, censored
    // and corrected), only under the measured time term: a thinking model fills whatever room it
    // gets (8k to 184k in 20 simulated turns), so growth stops at what her latency allows, and
    // with no measured rate it does not test upward at all (Cormac's design, card b726a311).
    // An act's floor already holds that bound (`act_room`); her upward test is the doubled bound
    // under the same measured time term (both ways, or it ratchets down). A cut ANSWER needs no test here: her floor already
    // holds its estimate past the bound (`OutputNeed::answer_room`).
    let upward = |at_least: u32| match kind {
        TurnKind::Act => at_least.saturating_mul(2),
        TurnKind::Pass => at_least,
    };
    let tested = match (need.and_then(|n| n.reasoning_at_least.map(|at_least| upward(at_least).saturating_add(n.answer_room()))), time_term) {
        (Some(upward), Some(ceiling)) => upward.min(ceiling),
        _ => 0,
    };
    let wanted = need
        .map_or(0, |n| n.total()) // map_or: nothing measured on this node wants nothing; its request carries the room its prompt leaves
        .max(tested)
        .max(pending_peak)
        .max(floor);
    let priority = match kind {
        TurnKind::Act if live_work || pending_peak > 0 => REPLY_TOP_PRIORITY,
        TurnKind::Act => 8,
        TurnKind::Pass if directed => 6,
        TurnKind::Pass => 4,
    };
    continuum_context::flex::FlexClaim {
        source_id: continuum_context::allocation::id::REPLY.to_owned(),
        priority,
        floor_tokens: floor,
        min_tokens: floor,
        max_tokens: wanted,
        required: true,
    }
}
