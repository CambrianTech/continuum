//! Her ALLOCATION ESTIMATE of what the chat template adds around each message, from the engine's
//! measured observations (`llama_server::template_overhead`).
//!
//! The engine publishes what each probe message added to a templated conversation. Against her
//! own text estimate of the same message ([`LlmDeliberationFaculty::message_cost`]), what remains
//! is the template's share, which differs by kind (system, user, assistant, a native tool result,
//! a native tool call that grows per parameter). Each kind is estimated at the HIGHEST the engine
//! was observed adding beyond her estimate, so the estimate errs high, never low, on every
//! observation; the spread (highest less lowest) is what that costs her. It decides what her view
//! keeps; it claims no exact count and no exact token prefix: the engine's exact quote of the
//! finalized body stays the authority for dispatch (`lane_send::PromptQuote`).

use continuum_engine::inference::llama_server::{MessageKind, MessageShape, TemplateMeasurement, TemplateSample};

use super::*;

/// One kind's observations beyond her estimate: the highest (what the estimate charges) and the
/// lowest (the highest less it is the spread an error-high estimate pays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Observed {
    pub(in super::super) high: u32,
    pub(in super::super) low: i64,
}

impl Observed {
    fn of(beyond: impl Iterator<Item = i64>) -> Option<Self> {
        let beyond: Vec<i64> = beyond.collect();
        let high = *beyond.iter().max()?;
        let low = *beyond.iter().min()?;
        Some(Self { high: u32::try_from(high.max(0)).ok()?, low })
    }
}

/// Each kind's template share at its highest observation; a native call's as a line over its
/// parameters (the lowest line on or above every observation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct KindEstimates {
    pub(in super::super) system: Observed,
    pub(in super::super) user: Observed,
    pub(in super::super) assistant: Observed,
    pub(in super::super) result: Observed,
    pub(in super::super) call: Observed,
    pub(in super::super) per_parameter: u32,
}

/// What the template adds around her messages, as an allocation estimate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in super::super) enum TemplateEstimate {
    /// No engine measurement for the engine and model her request reaches: her text is priced at
    /// its density alone (the density carries the template inside its ratio), probed as such.
    #[default]
    Unmeasured,
    Measured(KindEstimates),
}

impl TemplateEstimate {
    /// The estimate from the engine's observations; `Unmeasured` when any kind is missing (a
    /// partial measurement is none).
    pub(in super::super) fn from_measurement(measured: &TemplateMeasurement) -> Self {
        let TemplateMeasurement::Measured(samples) = measured else { return Self::Unmeasured };
        Self::from_samples(samples).map_or(Self::Unmeasured, Self::Measured) // map_or: a kind never observed is the unmeasured state
    }

    fn from_samples(samples: &[TemplateSample]) -> Option<KindEstimates> {
        // What each observation added beyond her estimate of it, per call or result it carries.
        let beyond = |sample: &TemplateSample| -> (MessageShape, i64) {
            let shape = MessageShape::of(&sample.message);
            let extra = i64::from(sample.added) - LlmDeliberationFaculty::message_cost(&sample.message) as i64;
            (shape, extra / i64::from(shape.units.max(1)))
        };
        let observed: Vec<(MessageShape, i64)> = samples.iter().map(beyond).collect();
        let kind = |kind: MessageKind| Observed::of(observed.iter().filter(|(shape, _)| shape.kind == kind).map(|(_, extra)| *extra));
        let calls: Vec<(i64, i64)> = observed.iter().filter(|(shape, _)| shape.kind == MessageKind::ToolCall).map(|(shape, extra)| (i64::from(shape.parameters), *extra)).collect();
        let (call, per_parameter) = call_line(&calls)?;
        Some(KindEstimates {
            system: kind(MessageKind::System)?,
            user: kind(MessageKind::User)?,
            assistant: kind(MessageKind::Assistant)?,
            result: kind(MessageKind::ToolResult)?,
            call,
            per_parameter,
        })
    }

    /// What the template adds around `message` beyond her estimate of it; `None` when unmeasured.
    pub(in super::super) fn around(self, message: &ChatMessage) -> Option<usize> {
        self.around_shape(MessageShape::of(message))
    }

    /// What the template adds around a message of `shape` beyond her estimate of it.
    pub(in super::super) fn around_shape(self, shape: MessageShape) -> Option<usize> {
        let Self::Measured(kinds) = self else { return None };
        let each = match shape.kind {
            MessageKind::System => kinds.system.high,
            MessageKind::User => kinds.user.high,
            MessageKind::Assistant => kinds.assistant.high,
            MessageKind::ToolResult => kinds.result.high,
            MessageKind::ToolCall => kinds.call.high,
        } as usize;
        Some(each * shape.units as usize + kinds.per_parameter as usize * shape.parameters as usize)
    }
}

/// PURE: the call line `base + per_parameter x parameters` on or above every observed call
/// `(parameters, beyond)`, of the candidate slopes (none, and each pair's) the one it overcharges
/// least in all; `None` with no call observed.
fn call_line(calls: &[(i64, i64)]) -> Option<(Observed, u32)> {
    let mut slopes = vec![0i64];
    for (i, &(pa, oa)) in calls.iter().enumerate() {
        for &(pb, ob) in &calls[i + 1..] {
            if pa != pb {
                let (dp, dob) = if pb > pa { (pb - pa, ob - oa) } else { (pa - pb, oa - ob) };
                slopes.push((dob + dp - 1).div_euclid(dp).max(0));
            }
        }
    }
    // A slope's overcharge over every observation, its base the highest observation less its line.
    let overcharge = |slope: i64| -> Option<i64> {
        let base = calls.iter().map(|&(p, o)| o - slope * p).max()?;
        Some(calls.iter().map(|&(p, o)| base + slope * p - o).sum())
    };
    let (_, slope) = slopes.into_iter().filter_map(|slope| Some((overcharge(slope)?, slope))).min()?;
    let base = Observed::of(calls.iter().map(|&(p, o)| o - slope * p))?;
    Some((base, u32::try_from(slope).ok()?))
}

/// The engine fixture: what each `probe_messages()` entry added on the M5's serving engine
/// (Qwen3.6-35B-A3B-Q4_K_M, build b11017-444f386cf), recorded 2026-11-07T11:38Z through its
/// `/apply-template` and `/tokenize` (read-only HTTP-thread calls, no slot, no decode).
#[cfg(test)]
pub(in super::super) fn recorded_qwen36() -> TemplateMeasurement {
    const ADDED: [u32; 24] = [6, 12, 10, 7, 11, 6, 12, 10, 7, 11, 6, 12, 10, 7, 11, 10, 16, 14, 11, 15, 17, 31, 43, 57];
    let probes = continuum_engine::inference::llama_server::probe_messages();
    assert_eq!(probes.len(), ADDED.len(), "the fixture records every probe");
    TemplateMeasurement::Measured(probes.into_iter().zip(ADDED).map(|(message, added)| TemplateSample { message, added }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_engine::ai::types::{ContentPart, MessageContent};

    // what this catches: an estimate that errs LOW. On the recorded engine every probe message is
    // estimated at or above what the engine counted it adding (text estimate plus its kind's share),
    // and a multi-role conversation (system, user, a native call, its result, assistant, a CJK user
    // turn), counted whole by the same engine at 97 tokens with its 5-token generation suffix, is
    // estimated at or above the 92 its messages hold. The spread each kind pays is stated.
    #[test]
    fn the_allocation_estimate_errs_high_on_the_recorded_engine() {
        let TemplateMeasurement::Measured(samples) = recorded_qwen36() else { unreachable!("the fixture is measured") };
        let estimate = TemplateEstimate::from_measurement(&recorded_qwen36());
        let TemplateEstimate::Measured(kinds) = estimate else { panic!("every kind observed") };
        for sample in samples.iter() {
            let priced = LlmDeliberationFaculty::message_cost(&sample.message) + estimate.around(&sample.message).expect("measured");
            assert!(priced >= sample.added as usize, "{:?} estimated {priced} under the engine's {}", sample.message, sample.added);
        }
        assert_eq!((kinds.user.high, kinds.user.low), (9, 1), "plain turns: highest 9 beyond her estimate, spread 8");
        assert_eq!((kinds.result.high, kinds.result.low), (13, 5), "native results: highest 13, spread 8");
        let call = |name: &str, input: serde_json::Value| ChatMessage {
            role: "assistant".into(),
            content: MessageContent::Parts(vec![ContentPart::ToolUse { id: "c1".into(), name: name.into(), input }]),
            name: None,
        };
        let conversation = [
            ChatMessage::text("system", "You are Aiko."),
            ChatMessage::text("user", "Read main.rs please."),
            call("code_read", serde_json::json!({ "path": "main.rs" })),
            ChatMessage::tool_result("c1", "fn main() {\n    println!(\"hi\");\n}\n", false),
            ChatMessage::text("assistant", "It prints hi."),
            ChatMessage::text("user", "了解しました。次は？"),
        ];
        let estimated: usize = conversation.iter().map(|m| LlmDeliberationFaculty::message_cost(m) + estimate.around(m).expect("measured")).sum();
        let (counted, suffix) = (97, 5);
        assert!(estimated >= counted - suffix, "the conversation estimated at {estimated}, under the engine's {}", counted - suffix);
        assert_eq!(TemplateEstimate::from_measurement(&TemplateMeasurement::Unmeasured).around(&conversation[1]), None, "unmeasured is a type, never a size");
    }
}
