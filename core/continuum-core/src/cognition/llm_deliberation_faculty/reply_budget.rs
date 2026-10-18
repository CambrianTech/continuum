//! The reply's share of the window: turn kind, output allowance, the reply's flex claim,
//! and what a cut or split emission measured.

use super::*;

/// The act-latency target the CONVERSATION fill budgets against — how long a turn's
/// history prefill is allowed to take. From the act-latency law: human expectations set
/// the bar; a turn that spends minutes re-reading history before thinking is disqualified
/// regardless of how smart the answer is. Window HEADROOM above this is reserve, not
/// default fill.
///
/// This is the LATENCY INTENT and nothing else — the half of the cap that is a product
/// decision. The other half, tokens-per-second, is MEASURED on this node
/// ([`crate::inference::prefill_rate::latency_fill_cap`]); it used to be a companion
/// constant of 500 t/s, and `30 × 500` made the fill a flat 15,000 tokens on every box in
/// the fleet. Measured on the M5 the night that was found (`inference.prefill.complete`):
/// `ingest_tok_per_s` of 70, 90 and 92, and the IntelMac at ~25 — so the constant was
/// 5–20× optimistic, the "30 second" target was really ~187 s, and the clip took 27% of a
/// 67,072-token lane away from Aiko while failing its own stated purpose on every machine.
// context-budget-exempt: a TIME target, not a token budget — the token cap is time x THIS node's MEASURED prefill rate (`prefill_rate::latency_fill_cap`)
pub(super) const PREFILL_TARGET_SECONDS: usize = 30;

/// The kind of turn a completion is for — its latency budget and its runaway bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnKind {
    /// A work turn: deliverable + hands — a tool call plus a short plan.
    Act,
    /// Every other turn — a reply in a room, or a workspace turn with nothing
    /// deliverable: a sentence and a reason. A short turn, on a thinking model, is
    /// still a THINK and then a sentence.
    Pass,
}

impl TurnKind {
    /// The seconds the TIME term of the allowance is derived from — a floor on a fast
    /// lane, never the bound (see [`output_allowance`]).
    fn latency_budget_secs(self) -> f64 {
        match self {
            Self::Act => LlmDeliberationFaculty::ACT_LATENCY_BUDGET_SECS,
            Self::Pass => LlmDeliberationFaculty::PASS_LATENCY_BUDGET_SECS,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Act => "act",
            Self::Pass => "pass",
        }
    }
}

/// The reported terms beside a turn's allowance — every field rides the
/// `delib.turn.output_allowance` receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OutputAllowance {
    /// `latency budget × her decode rate`; `None` when no rate is known on this box.
    pub(super) time_term: Option<u32>,
    /// Her measured reasoning + answer need; `None` until measured.
    pub(super) need_term: Option<u32>,
    /// What the request carries as `max_tokens` — the reply's flex grant.
    pub(super) allowance: u32,
    /// The measured need did not fit the grant the window left — said, not hidden.
    pub(super) need_clipped: bool,
}

/// What the turn is EXPECTED to use beside the allowance it was granted: `time budget ×
/// her decode rate` and her measured reasoning + answer need. Reported, never the
/// bound: the bound is the reply's flex grant ([`reply_claim`]).
///
/// Why both terms are still read (the M5, 2026-09-20 20:58–21:14Z): #4194 sized the
/// allowance as TIME alone. After a restart her rate read 0.0 and the allowance fell to
/// a floor — `kind=pass measured_tps=0.0 allowance=768`, `kind=act allowance=1500` —
/// and two turns ended inside the reasoning channel (`persona.act.think_only
/// reasoning_len=2730`, `2112`): no answer, no act. So the NEED term sizes the reply's
/// claim ([`reply_claim`]) and the time term is reported beside it. A 0.0 or NaN rate
/// is no rate.
///
/// THE MEASUREMENTS ARE EXPECTATIONS, NEVER THE CEILING (the 5090, 2026-10-06): a p90
/// need with headroom is exceeded by about one turn in ten, and a cut tool call commits
/// nothing (Kimi's `tools/<cut at the output limit> ✗`, allowance 10,177 = need). So the
/// allowance is the grant plus every token no other claim took, under the runaway bound.
pub(super) fn output_allowance(
    kind: TurnKind,
    tps: Option<f64>,
    need: Option<crate::cognition::working_set::OutputNeed>,
    allowance: u32,
) -> OutputAllowance {
    let time_term = tps
        .filter(|t| t.is_finite() && *t > 0.0)
        .map(|t| (kind.latency_budget_secs() * t).round() as u32);
    let need_term = need.map(|n| n.total());
    let allowance = allowance.max(1);
    OutputAllowance {
        time_term,
        need_term,
        allowance,
        need_clipped: need_term.is_some_and(|n| n > allowance),
    }
}

/// A pass is bounded at this multiple of her measured p90 turn (never below the act's
/// cap). A runaway bound, not a budget: it binds only on a loop (Cormac on #4815: a loop
/// must not hold the lane for minutes; twice her p90 sits far above the one-in-ten tail
/// a p90 lets through).
const PASS_RUNAWAY_MULTIPLE: u32 = 2;

/// The flex claim ids of the deliberation prompt plan. One window, five claimants.
pub(super) const FLEX_FRAMING: &str = "framing";
pub(super) const FLEX_REQUIRED: &str = "required_input";
pub(super) const FLEX_GROUNDING: &str = "grounding";
pub(super) const FLEX_HISTORY: &str = "history";
pub(super) const FLEX_REPLY: &str = "reply";

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
    need: Option<crate::cognition::working_set::OutputNeed>,
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

/// The working-memory record of a cut act: that it did not land, how long it ran, and
/// how it began and ended, so the next generation starts from what she was composing
/// rather than re-deriving it. Head and tail only: the whole payload is what was too
/// large to land, and repeating it would crowd the retry the same way. Pure.
pub(super) fn cut_act_fact(text: &str) -> String {
    const EDGE: usize = 240;
    let chars = text.chars().count();
    let head: String = text.chars().take(EDGE).collect();
    let tail: String = if chars > 2 * EDGE {
        let skip = chars - EDGE;
        text.chars().skip(skip).collect()
    } else {
        String::new()
    };
    if tail.is_empty() {
        format!("my last act was CUT at the output limit after {chars} chars before a tool call was committed — it did not land, nothing ran. I was composing: {head}")
    } else {
        format!("my last act was CUT at the output limit after {chars} chars before a tool call was committed — it did not land, nothing ran. I was composing: {head} … and it ended: {tail}")
    }
}

/// Apportion the server's output count between the reasoning channel and the answer
/// by the bytes each channel carried — the server counts tokens, the adapter splits
/// text. No channel text at all (a native tool call) is all answer.
pub(super) fn reasoning_answer_split(output_tokens: u32, reasoning_chars: usize, answer_chars: usize) -> (u32, u32) {
    let total = reasoning_chars.saturating_add(answer_chars);
    if total == 0 || reasoning_chars == 0 {
        return (0, output_tokens);
    }
    let reasoning = ((output_tokens as u64 * reasoning_chars as u64) / total as u64) as u32;
    let reasoning = reasoning.min(output_tokens);
    (reasoning, output_tokens - reasoning)
}

/// Native tool arguments are generated answer bytes too. `text` is the adapter's
/// plain-text projection, so counting it alone turns thinking + tools into a
/// think-only sample. Do not count Text parts again: they mirror that projection.
pub(super) fn answer_payload_bytes(text: &str, parts: Option<&[crate::ai::types::ContentPart]>) -> usize {
    parts.unwrap_or_default().iter().fold(text.len(), |bytes, part| {
        match part {
            crate::ai::types::ContentPart::ToolUse { name, input, .. } => {
                bytes.saturating_add(name.len()).saturating_add(input.to_string().len())
            }
            _ => bytes,
        }
    })
}

impl LlmDeliberationFaculty {
    /// Is this turn an ACT on a deliverable, rather than a message turn?
    ///
    /// Asked by two independent concerns — the surface selection (a work turn takes
    /// her hands, for focus) and the ACT output cap — so it lives in one place
    /// rather than being spelled twice and drifting.
    pub(super) fn is_work_turn(&self, ws: &Workspace) -> bool {
        ws.workspace_deliverable && !self.hands_specs.is_empty()
    }

    /// What kind of turn this is, for its allowance: a work turn is an ACT; anything
    /// else is a PASS.
    pub(super) fn turn_kind(&self, ws: &Workspace) -> TurnKind {
        if self.is_work_turn(ws) {
            TurnKind::Act
        } else {
            TurnKind::Pass
        }
    }

    /// What an UNMEASURED mind's reply wants ([`reply_claim`]'s max): a reply-sized
    /// constant, never a fraction of the window.
    ///
    /// The prior used to be a `window / 2` share, on the reasoning
    /// that the most generous defensible ask is the safe default. Measured on one citizen,
    /// same question, 28 minutes apart (2026-09-18, Cormac, `delib.context.render`):
    ///
    /// | seat | window | reserve | context rendered | answer |
    /// |------|--------|---------|------------------|--------|
    /// | remote 27B | 16,384 | 2,817 (measured) | ~11k | "Tracing the dream/memory-consolidation path through `dream_consolidation.rs`…" |
    /// | home | 32,768 | **16,384** (the share) | **297** | "As an AI assistant, I am currently not actively engaged in any specific project…" |
    ///
    /// A BIGGER window gave her LESS context. Half of it was held back for a reply she
    /// would never write, framing took 2,727 of the remainder, and the 12k of conversation
    /// did not fit at all — she was asked who she is with 297 tokens of world in front of
    /// her, and the stub is the CORRECT answer to that prompt. She was not erased by a weak
    /// model; she was handed nothing.
    ///
    /// The same fraction on the other end of the range is the 5090's 101,120-token window
    /// licensing a 100k think-only turn that wrote nothing. One defect, two faces: the
    /// reserve was sized to the WINDOW instead of to the REPLY. On a big window that is a
    /// licence to spin; on a small one the person does not fit.
    ///
    /// `MIN_SERVE_CTX` and not a new literal, deliberately: the smallest window the serving
    /// stack hands out. It is only the unmeasured WANT of the reply's claim: what no other
    /// claim takes still flows to the reply under its runaway bound, measurement replaces
    /// it with her need, and a turn cut inside its answer records that channel at DOUBLE,
    /// so an act that really needs more room earns it back within a turn.
    pub(super) const COMPLETION_COLD_PRIOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX;

    /// The most an ACT turn may generate — a RUNAWAY bound, not a budget.
    /// Measured acts that COMMITTED a tool call ran 4,188–9,818 tokens on the
    /// 27B (the model thinks before it calls; 2026-09-07 acceptance rows), so
    /// any cap below that faults the act and re-samples it — more decode for
    /// zero committed work (IntelMac's review of #3824). This binds only past
    /// the largest committed act we have seen; the real lever is bounding the
    /// THINKING on act turns, which is card 61b6e54d's next slice.
    pub const ACT_OUTPUT_CAP: u32 = 12_288;

    /// The TIME term's budgets, in seconds, per turn kind — `budget × her measured
    /// decode rate` is a FLOOR the allowance may rise to on a fast lane (the 5090 at
    /// 40 tok/s: an act may run to 6,000 tokens), never the bound. The bound is her
    /// measured reasoning + answer need, under the reserve ([`output_allowance`]). Sized
    /// small on purpose: a floor that is too low costs nothing when the need is
    /// measured and is not consulted when it is not; a bound that is too low ended
    /// turns inside the reasoning channel (the M5, 2026-09-20).
    pub const ACT_LATENCY_BUDGET_SECS: f64 = 150.0;
    pub const PASS_LATENCY_BUDGET_SECS: f64 = 60.0;

    /// Floor so a tiny window still yields a usable reply, and the same term the prompt
    /// floor uses for a minimum burst.
    ///
    /// DERIVED from `MIN_SERVE_CTX`, not declared: an eighth of the smallest window the
    /// serving stack will ever hand out. A bare `256` here was caught by
    /// `context_budget::no_new_hardcoded_context_or_prompt_size_constant_anywhere_in_the_crate`
    /// — correctly, and the catch is worth recording: the constant carries `TOKENS` in its
    /// name, the guard keys on exactly that, and every prompt-shaping test stayed GREEN
    /// while it was wrong. Only the full suite sees this guard. Deriving it also means the
    /// floor tracks the substrate floor for free if `MIN_SERVE_CTX` ever moves, instead of
    /// becoming a second opinion about how small a window can get.
    pub(super) const COMPLETION_FLOOR_TOKENS: u32 = crate::inference::planning::MIN_SERVE_CTX / 8;
}
