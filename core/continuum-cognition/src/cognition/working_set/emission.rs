//! How a generation ended and what it measured, read once from the response: the server's output
//! count split by channel, whether she answered (text or a call: presence, never a count), and
//! where the cap fell ([`EmissionStop`]). The faculty records this after every completed
//! generation, and the act-room replay (`examples/act_room_replay.rs`) reads captured responses
//! through the same function, so the live path and its validator cannot disagree.

use crate::ai::types::{ContentPart, FinishReason, TextGenerationResponse};

/// How a generation ENDED, as the emission record has to know it. The registry sizes
/// the next allowance from what it records here, so the record must say not only that
/// a generation was cut at its cap but WHICH CHANNEL the cap fell in.
///
/// Why (BigMama, Kimi, 2026-09-26): every `FinishReason::Length` used to record at DOUBLE
/// — "clamped by the reserve, so the true demand is above the count". Kimi's turns
/// refuted that reading: 41% of her requests failed, and the failed ones were exactly
/// the large ones (`maxTokens` ≥ 9,848 against a completed p50 of 6,448). A pass cut
/// inside its think doubled the need, the next allowance grew to hold it, the longer
/// generation ran past the turn deadline and failed outright — and a failure records
/// nothing, so only the cuts were ever measured. A one-way ratchet; her live
/// `emission.json` held think-only cuts at 14,272 and 18,154 and a peak of 18,154.
///
/// The first revision halved such a cut. Cormac's review of #4406 showed that is a
/// ratchet the other way once the loop closes — a smaller need cuts her earlier, which
/// halves again, C → C/2 → C/4 down to the floor — and that "committed = a parsed tool
/// call" mislabels a `code/write` cut mid-payload, the very cut that needed more room.
/// So the classification is by CHANNEL, and no factor exists anywhere in the record: a cut
/// channel is a CENSORED sample (her need is above what she got), kept as a bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmissionStop {
    /// The model stopped on its own (stop token or tool call): both channels record
    /// verbatim — this is her real size for that turn.
    Landed,
    /// Cut at the cap WITH answer tokens: she was saying or calling something and the
    /// cap fell in it. Her think ended, so it records verbatim; her answer is a lower bound
    /// (`Observed::AtLeast`, what she wrote before the cap), read by the one censored
    /// estimator ([`super::need`]), so one cut raises her next answer room past it to its
    /// median ([`super::OutputNeed::answer_room`])
    /// (the M5, 2026-11-07, fe91c2b55: the answer used to be pushed doubled into a p90 ring
    /// of 16, where one sample is never read, so a cut left the room unchanged and only a
    /// second wasted act grew it). The peak is the SUM of the channels as measured.
    CutMidAnswer,
    /// Cut at the cap with NO answer tokens: the think consumed the whole allowance and
    /// nothing was said. Not a sample of her need at all — a thinking model fills
    /// whatever room it is given, so a censored think says only "at least this much
    /// think fits in this allowance"; pushed into the need ring it would carry the
    /// landed turns' headroom on top of a value that already IS the allowance, and the
    /// loop would drift up ×5/4 a turn to the share. The peak takes it verbatim (so the
    /// reserve holds), `turns` counts it, the need ring does not: the allowance stays
    /// sized by the turns in which she said something, which is the branch's name.
    /// The cut itself is the fault paths' business (`persona.act.think_only`,
    /// `delib.truncated_not_an_answer`), not the measurement's.
    CutMidThought,
    /// The turn LANDED, but its think stopped AT the reasoning budget the request set
    /// (three quarters of the allowance, `deliberation_reasoning_budget`). The reasoning
    /// sample is censored: a thinking model fills whatever room it is given, so the
    /// count says only "at least the budget", not her need. Pushed verbatim it would
    /// teach the need ring the budget, and with the 5/4 headroom the next allowance is
    /// 5/4 × (3/4 A + answer) = 15/16 A + 5/4 answer — a geometric contraction toward
    /// max(time floor, 20 × answer), the forbidden clamp arrived at by feedback (Cormac
    /// on #4409). Recording it AT the allowance instead compounds the other way: a
    /// thinking model fills every budget, so "at least what she was given" ×5/4 each
    /// turn drives every deliberating mind to the full reserve (simulated: 8k → 184k in
    /// 20 turns), and on a slow lane that is the latency law broken. So a budget-hit is
    /// NOT a need sample, exactly as a think-only cut is not: the need ring keeps her
    /// landed shape, the peak takes the turn verbatim, and growth comes only from turns
    /// she ends on her own or from a cut inside the answer.
    ThinkBudgetHit { allowance: u32 },
}

impl EmissionStop {
    /// Classify a finished generation by where the cap fell. `hit_cap` is a
    /// `FinishReason::Length` stop; `answered` is whether the response CARRIES an answer
    /// (text or a call), never a token count, which a stream may not report: a cut with an
    /// answer present and usage missing is a cut in the answer (Codex on #5616). Only a
    /// cap hit is a cut. This is the one classification: the allowance policy, its probes
    /// and the hour line's counter ([`Self::counted`]) all read it.
    pub fn classify(hit_cap: bool, answered: bool) -> Self {
        match (hit_cap, answered) {
            (false, _) => Self::Landed,
            (true, false) => Self::CutMidThought,
            (true, true) => Self::CutMidAnswer,
        }
    }

    /// [`Self::classify`] with the request's reasoning budget in hand: a landed turn
    /// whose think reached the budget is a `ThinkBudgetHit`, the rest classify as before.
    /// The reasoning count is a measurement: missing usage reads 0 and so never claims a hit.
    pub fn classify_with_budget(
        hit_cap: bool,
        answered: bool,
        reasoning_tokens: u32,
        reasoning_budget: Option<u32>,
        allowance: u32,
    ) -> Self {
        match reasoning_budget {
            Some(budget) if !hit_cap && budget > 0 && reasoning_tokens >= budget => {
                Self::ThinkBudgetHit { allowance }
            }
            _ => Self::classify(hit_cap, answered),
        }
    }

    /// The hour line's count of this stop (`health_ledger::Stop`). A landed turn splits by
    /// what it carries: an answer is landed, reasoning alone ended in thought, nothing is empty.
    /// `answered` is the same presence [`Self::classify`] was given.
    pub fn counted(self, answered: bool, reasoned: bool) -> continuum_vitals::health_ledger::Stop {
        use continuum_vitals::health_ledger::Stop;
        match self {
            Self::CutMidAnswer => Stop::CutAnswer,
            Self::CutMidThought => Stop::CutThink,
            Self::ThinkBudgetHit { .. } => Stop::ThinkBudget,
            Self::Landed if answered => Stop::Landed,
            Self::Landed if reasoned => Stop::ThinkEnded,
            Self::Landed => Stop::Empty,
        }
    }
}

/// One completed generation as the emission record reads it ([`ResponseEmission::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseEmission {
    /// The share of the server's output count the reasoning channel took
    /// ([`reasoning_answer_split`]); the answer is the rest.
    pub reasoning_tokens: u32,
    /// The response carries an answer: text, or a call (complete or cut at the limit).
    pub answered: bool,
    /// The response carries reasoning.
    pub reasoned: bool,
    pub stop: EmissionStop,
}

impl ResponseEmission {
    /// Read `resp` against the request's think budget and `max_tokens` (`allowance`). A tool call
    /// cut at the limit is an answer (the call IS her answer): its arguments count as answer bytes
    /// ([`answer_payload_bytes`]) and its presence makes the cut a [`EmissionStop::CutMidAnswer`],
    /// whatever its text.
    pub fn of(resp: &TextGenerationResponse, reasoning_budget: Option<u32>, allowance: u32) -> Self {
        // The calls' bytes, when the content parts do not already mirror them (an adapter that
        // carries calls only in `tool_calls`): never an open call counted as think.
        let mirrored = resp.content.iter().flatten().any(|part| matches!(part, ContentPart::ToolUse { .. }));
        let unmirrored_calls = match mirrored {
            true => 0,
            false => resp.tool_calls.iter().flatten().map(|call| call.name.len().saturating_add(call.input.to_string().len())).sum(),
        };
        let (reasoning_tokens, _) = reasoning_answer_split(
            resp.usage.output_tokens,
            resp.reasoning.as_ref().map_or(0, |r| r.len()), // map_or: no reasoning channel carried zero bytes
            answer_payload_bytes(&resp.text, resp.content.as_deref()).saturating_add(unmirrored_calls),
        );
        let answered = answer_payload_bytes(resp.text.trim(), resp.content.as_deref()) > 0
            || resp.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty());
        let reasoned = resp.reasoning.as_deref().is_some_and(|r| !r.trim().is_empty());
        let stop = EmissionStop::classify_with_budget(
            matches!(resp.finish_reason, FinishReason::Length),
            answered,
            reasoning_tokens,
            reasoning_budget,
            allowance,
        );
        Self { reasoning_tokens, answered, reasoned, stop }
    }
}

/// Apportion the server's output count between the reasoning channel and the answer
/// by the bytes each channel carried — the server counts tokens, the adapter splits
/// text. No channel text at all (a native tool call) is all answer.
pub fn reasoning_answer_split(output_tokens: u32, reasoning_chars: usize, answer_chars: usize) -> (u32, u32) {
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
pub fn answer_payload_bytes(text: &str, parts: Option<&[ContentPart]>) -> usize {
    parts.unwrap_or_default().iter().fold(text.len(), |bytes, part| {
        match part {
            ContentPart::ToolUse { name, input, .. } => {
                bytes.saturating_add(name.len()).saturating_add(input.to_string().len())
            }
            _ => bytes,
        }
    })
}
