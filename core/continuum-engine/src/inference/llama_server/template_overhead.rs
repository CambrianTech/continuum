//! What the serving model's chat template adds around a message, MEASURED from the engine.
//!
//! A prompt's estimate counts message text; the engine counts the templated prompt, which wraps
//! every message in the model's role and turn markers, and renders a native tool call as markup
//! that grows with its parameters (Qwen3.6: `<tool_call>\n<function=…>\n<parameter=…>…`). Those
//! tokens belong to the model's template and tokenizer, so they are measured once per engine
//! process and model through the engine's own endpoints (`/apply-template` renders a conversation
//! through the model's template, `/tokenize` counts it: HTTP-thread calls, no slot, no decode),
//! never written as a constant (it was `PER_MESSAGE_TEMPLATE_TOKENS = 5`, removed on #5814, after
//! which a short turn cost 0 and a start kept twenty of them against a 32-token grant: Codex's
//! 100 > 32).
//!
//! The engine publishes the raw observations: each probe message and the engine tokens it added
//! in the middle of a conversation. A two-renders-less-one delta is not a universal per-message
//! cost (BPE joins at the boundaries, the role, the tool markup and the generation suffix all move
//! it; Codex measured an independently templated 2-message prefix of 63 against an actual common
//! prefix of 62), so this module claims no exact count. A caller derives its ALLOCATION ESTIMATE
//! from them against its own text estimate, per kind of message, at their highest, so its estimate
//! errs high on every observation (cognition's `prompt/template.rs`).

use std::sync::Arc;

use crate::ai::types::{ChatMessage, ContentPart, MessageContent};
use crate::inference::engine_residency::EngineIncarnation;

/// One probe message and the engine tokens it added where it stood in the middle of a
/// conversation (its render with it, less its render without it).
#[derive(Debug, Clone)]
pub struct TemplateSample {
    pub message: ChatMessage,
    pub added: u32,
}

/// The engine's observations of its template, for the engine and model a request reaches.
#[derive(Debug, Clone, Default)]
pub enum TemplateMeasurement {
    /// Not measured: a caller prices message text at its density alone (which carries the
    /// template inside its ratio) and says so.
    #[default]
    Unmeasured,
    Measured(Arc<[TemplateSample]>),
}

/// The kind of message a template wraps differently: the role it renders under, or a native tool
/// call or result. The one owner of how a message is classified, for the probes and the pricing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageKind {
    System,
    User,
    Assistant,
    /// An assistant turn carrying native tool calls: wrapped per call, and per parameter.
    ToolCall,
    /// A turn carrying native tool results: wrapped per result.
    ToolResult,
}

/// How a template sees one message: its kind, how many calls or results it carries (1 for a
/// plain message), and how many call parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageShape {
    pub kind: MessageKind,
    pub units: u32,
    pub parameters: u32,
}

impl MessageShape {
    pub fn of(message: &ChatMessage) -> Self {
        let count = |pick: fn(&ContentPart) -> Option<u32>| -> Option<(u32, u32)> {
            let MessageContent::Parts(parts) = &message.content else { return None };
            let picked: Vec<u32> = parts.iter().filter_map(pick).collect();
            (!picked.is_empty()).then(|| (picked.len() as u32, picked.iter().sum()))
        };
        if let Some((units, _)) = count(|part| matches!(part, ContentPart::ToolResult { .. }).then_some(0)) {
            return Self { kind: MessageKind::ToolResult, units, parameters: 0 };
        }
        let parameters_of = |part: &ContentPart| match part {
            ContentPart::ToolUse { input, .. } => Some(input.as_object().map_or(0, |object| object.len() as u32)), // map_or: arguments that are no object carry no named parameter
            _ => None,
        };
        if let Some((units, parameters)) = count(parameters_of) {
            return Self { kind: MessageKind::ToolCall, units, parameters };
        }
        let kind = match message.role.as_str() {
            "system" => MessageKind::System,
            "assistant" => MessageKind::Assistant,
            _ => MessageKind::User,
        };
        Self { kind, units: 1, parameters: 0 }
    }
}

/// The texts each plain kind is probed with: short and long, a trailing newline (a BPE join at the
/// template's own newline), non-ASCII, JSON.
const PROBE_TEXTS: [&str; 5] = ["ok", "Result: 4 files changed.", "line one\nline two\n", "了解しました", "{\"a\": 1}"];

fn call(name: &str, input: serde_json::Value) -> ChatMessage {
    ChatMessage {
        role: "assistant".into(),
        content: MessageContent::Parts(vec![ContentPart::ToolUse { id: "c1".into(), name: name.into(), input }]),
        name: None,
    }
}

/// The probe messages, in order: each plain kind with every [`PROBE_TEXTS`], tool results with
/// them, and tool calls with zero to three parameters (the markup grows per parameter).
pub fn probe_messages() -> Vec<ChatMessage> {
    let mut probes = Vec::new();
    for role in ["system", "user", "assistant"] {
        probes.extend(PROBE_TEXTS.iter().map(|text| ChatMessage::text(role, *text)));
    }
    probes.extend(PROBE_TEXTS.iter().map(|text| ChatMessage::tool_result("c1", *text, false)));
    probes.push(call("f", serde_json::json!({})));
    probes.push(call("code_read", serde_json::json!({ "path": "main.rs" })));
    probes.push(call("work_list", serde_json::json!({ "room": continuum_base::reserved_rooms::GENERAL, "limit": 5 })));
    probes.push(call("code_edit", serde_json::json!({ "path": "src/lib.rs", "old": "a", "new": "b\nc" })));
    probes
}

/// The conversations `probe` is measured between: with it in the middle, and without it. A system
/// message stands first; a tool result follows the call it answers.
fn framed(probe: &ChatMessage) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
    let user = ChatMessage::text("user", "ok");
    match MessageShape::of(probe).kind {
        MessageKind::System => (vec![probe.clone(), user.clone()], vec![user]),
        MessageKind::ToolResult => {
            let asked = call("f", serde_json::json!({}));
            (vec![user.clone(), asked.clone(), probe.clone(), user.clone()], vec![user.clone(), asked, user])
        }
        _ => (vec![user.clone(), probe.clone(), user.clone()], vec![user.clone(), user]),
    }
}

/// The engine's `/apply-template` render of `messages`, on the wire the way a request sends them.
async fn rendered(client: &reqwest::Client, root: &str, messages: &[ChatMessage]) -> Option<String> {
    let wire = crate::inference::request_body::wire_messages(messages, None, false, false, "llama-server");
    let resp = client
        .post(format!("{root}/apply-template"))
        .timeout(super::PROBE_TIMEOUT)
        .json(&serde_json::json!({ "messages": wire }))
        .send()
        .await
        .ok()
        .filter(|resp| resp.status().is_success())?;
    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("prompt")?.as_str().map(str::to_owned)
}

/// The engine's `/tokenize` count of `content`, its template markers parsed as special tokens.
async fn counted(client: &reqwest::Client, root: &str, content: &str) -> Option<u32> {
    let resp = client
        .post(format!("{root}/tokenize"))
        .timeout(super::PROBE_TIMEOUT)
        .json(&serde_json::json!({ "content": content, "add_special": false, "parse_special": true }))
        .send()
        .await
        .ok()
        .filter(|resp| resp.status().is_success())?;
    let body: serde_json::Value = resp.json().await.ok()?;
    u32::try_from(body.get("tokens")?.as_array()?.len()).ok()
}

/// PURE: what a probe added: its conversation's count less the count without it. `None` when the
/// render did not grow (an engine that merged it away is no observation).
pub(crate) fn added(with: u32, without: u32) -> Option<u32> {
    with.checked_sub(without).filter(|added| *added > 0)
}

/// Measure the engine at `root`: every probe's added tokens, or `None` when any endpoint did not
/// answer (a partial measurement is no measurement).
pub(super) async fn measure(client: &reqwest::Client, root: &str) -> Option<Vec<TemplateSample>> {
    let mut samples = Vec::new();
    for message in probe_messages() {
        let (with, without) = framed(&message);
        let with = counted(client, root, &rendered(client, root, &with).await?).await?;
        let without = counted(client, root, &rendered(client, root, &without).await?).await?;
        samples.push(TemplateSample { message, added: added(with, without)? });
    }
    Some(samples)
}

/// One measurement, for the engine process and model it was taken on.
#[derive(Debug, Clone)]
struct Measurement {
    incarnation: Option<EngineIncarnation>,
    model: String,
    measured: TemplateMeasurement,
}

/// The serving lane's measurement. Written by the reconcile that reads the lane's build.
static SERVED_TEMPLATE: parking_lot::Mutex<Option<Measurement>> = parking_lot::Mutex::new(None);

/// Whether the serving lane's measurement is for this engine process and model already.
pub(super) fn measured_for(incarnation: Option<EngineIncarnation>, model: &str) -> bool {
    SERVED_TEMPLATE.lock().as_ref().is_some_and(|m| m.incarnation == incarnation && m.model == model)
}

/// Record the serving lane's measurement for `incarnation` and `model` (an unanswered measurement
/// is recorded `Unmeasured`, so it is taken again only for the next engine or model).
pub(super) fn record(incarnation: Option<EngineIncarnation>, model: &str, samples: Option<Vec<TemplateSample>>) {
    let added: Vec<u32> = samples.iter().flatten().map(|sample| sample.added).collect();
    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.template_overhead",
        model = model,
        measured = samples.is_some(),
        added = ?added,
        "the serving model's chat template measured from the engine: tokens each probe message added"
    );
    let measured = samples.map_or(TemplateMeasurement::Unmeasured, |samples| TemplateMeasurement::Measured(samples.into())); // map_or: no answer is the unmeasured state, a type
    *SERVED_TEMPLATE.lock() = Some(Measurement { incarnation, model: model.to_owned(), measured });
}

/// The serving lane's template measurement for `model`, `Unmeasured` when the lane's measurement
/// is for another model or none was taken.
pub fn serving_template_overhead(model: &str) -> TemplateMeasurement {
    SERVED_TEMPLATE.lock().as_ref().filter(|m| m.model == model).map_or(TemplateMeasurement::Unmeasured, |m| m.measured.clone()) // map_or: no measurement for this model is the unmeasured state
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a message priced under the wrong kind. A native result is wrapped per
    // result whatever its role string, a call per call and per parameter, a plain turn by role.
    #[test]
    fn a_message_is_classified_by_what_the_template_wraps() {
        let shape = |message: &ChatMessage| MessageShape::of(message);
        assert_eq!(shape(&ChatMessage::tool_result("c1", "x", false)), MessageShape { kind: MessageKind::ToolResult, units: 1, parameters: 0 });
        let edit = call("code_edit", serde_json::json!({ "path": "a", "old": "b", "new": "c" }));
        assert_eq!(shape(&edit), MessageShape { kind: MessageKind::ToolCall, units: 1, parameters: 3 });
        assert_eq!(shape(&ChatMessage::text("system", "x")).kind, MessageKind::System);
        assert_eq!(shape(&ChatMessage::text("assistant", "x")).kind, MessageKind::Assistant);
        assert_eq!(shape(&ChatMessage::text("user", "x")).kind, MessageKind::User);
        assert_eq!(added(15, 9), Some(6));
        assert_eq!(added(9, 9), None, "a render that did not grow is no observation");
    }
}
