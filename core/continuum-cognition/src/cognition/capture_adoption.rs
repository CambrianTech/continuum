//! Adopting her head from one captured request, kept warm (card 9cf06ade, Codex's review).
//!
//! A view checkpointed before heads were recorded has no record of the tool schemas her engine
//! holds. The captured request her last sent act carried does: its tools, system prompt, model
//! and messages are what the engine prefilled. An operator names that capture explicitly
//! (`persona/hands/adopt_capture`: its cursor, its digest, and the custody it is asked over). Her
//! next act adopts it only when every part matches exactly: the custody is still the one named,
//! her accepted frontier (the prompt her last sent act carried) is the captured request's messages,
//! and this act's system prompt, tool payload and model are the captured ones byte for byte, with
//! its messages beginning with the captured ones. Only then is her head frozen from this act's
//! live selection, which is the same payload, so the engine's KV holds every token of it. Any
//! difference refuses with the place it differs; her refusal and her view are kept. Nothing is
//! reconstructed and nothing is forced: a mismatch is adopted live (`persona/hands/adopt`), cold.

use std::sync::Arc;

use sha2::Digest;

use super::context_streams::ContextMessage;
use super::hands_custody::HandsCustody;
use super::working_memory::ContextView;
use crate::ai::types::{ChatMessage, NativeToolSpec, TextGenerationRequest};

/// The explicit identity an operator adopts by: which capture, at what digest, over which custody.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CaptureBinding {
    /// The capture's stable cursor (`cognition/playback`).
    pub(crate) cursor: String,
    /// The captured request's canonical digest (`continuum_capture::request_digest`).
    pub(crate) request_sha256: String,
    /// Her custody's digest when the adoption was asked ([`custody_sha256`]).
    pub(crate) expected_custody_sha256: String,
}

/// The captured request an adoption is checked against, decoded once by the command.
#[derive(Clone, Debug)]
pub(crate) struct CapturedHead {
    pub(crate) binding: CaptureBinding,
    pub(crate) request: Arc<TextGenerationRequest>,
}

/// What an adoption asks for, and who asked.
#[derive(Clone, Debug)]
pub(crate) enum AdoptionRequest {
    /// Her head selected afresh from the live surface: a new epoch, prefilled cold.
    Live { by: String },
    /// Her head adopted from one captured request, only where it matches exactly.
    Capture { by: String, head: CapturedHead },
}

impl AdoptionRequest {
    pub(crate) fn by(&self) -> &str {
        match self {
            AdoptionRequest::Live { by } | AdoptionRequest::Capture { by, .. } => by,
        }
    }
}

/// Where a capture adoption does not match; her refusal and her view are kept.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub(crate) enum CaptureMismatch {
    #[error("her custody is {actual}, not the {expected} the adoption was asked over")]
    CustodyMoved { expected: String, actual: String },
    #[error("her view has no accepted frontier: no act of this epoch has been sent")]
    NoAcceptedFrontier,
    #[error("her accepted frontier is {frontier} messages and the captured request has {captured}")]
    FrontierLength { frontier: usize, captured: usize },
    #[error("her accepted frontier differs from the captured request at message {index}")]
    FrontierMessage { index: usize },
    #[error("this act's system prompt differs from the captured one at byte {at}")]
    System { at: usize },
    #[error("this act sends {now} tool schemas and the captured request sent {captured}")]
    ToolCount { now: usize, captured: usize },
    #[error("this act's tool schema {index} ({name}) differs from the captured one")]
    Tool { index: usize, name: String },
    #[error("this act is served as {now:?} and the captured request as {captured:?}")]
    Model { now: Option<String>, captured: Option<String> },
    #[error("this act's messages do not begin with the captured request's: they differ at message {index}")]
    Messages { index: usize },
    #[error("she has no working memory to hold her custody")]
    NoMemory,
    #[error("a wire value would not encode: {0}")]
    Unencodable(String),
}

/// Her custody as an operator reads it before asking an adoption over it (`persona/hands/custody`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CustodyReceipt {
    /// [`custody_sha256`] of her custody now.
    pub(crate) sha256: String,
    /// What it holds: `held generation N`, `corrupt`, or `unrecorded`.
    pub(crate) holds: String,
    /// How many events of her view's epoch her last sent act carried; `None` with no view.
    pub(crate) frontier_events: Option<usize>,
}

impl CustodyReceipt {
    pub(crate) fn of(custody: &HandsCustody, view: Option<&ContextView>) -> Result<Self, CaptureMismatch> {
        let holds = match custody {
            HandsCustody::Held(hands) => format!("held generation {}", hands.generation),
            HandsCustody::Corrupt(_) => "corrupt".to_string(),
            HandsCustody::Unrecorded => "unrecorded".to_string(),
        };
        Ok(CustodyReceipt { sha256: custody_sha256(custody)?, holds, frontier_events: view.map(ContextView::sent_events) })
    }
}

/// Her custody's digest, as her checkpoint writes it: what an adoption is asked over and checked
/// against, so a custody that moved in between refuses it.
pub(crate) fn custody_sha256(custody: &HandsCustody) -> Result<String, CaptureMismatch> {
    fn written<S: serde::Serializer>(hands: &&HandsCustody, serializer: S) -> Result<S::Ok, S::Error> {
        super::hands_custody::serialize(hands, serializer)
    }
    #[derive(serde::Serialize)]
    struct Written<'a> {
        #[serde(serialize_with = "written")]
        hands: &'a HandsCustody,
    }
    let bytes = serde_json::to_vec(&Written { hands: custody }).map_err(|error| CaptureMismatch::Unencodable(error.to_string()))?; // boundary: the digest of her custody as her checkpoint writes it
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

/// Her accepted frontier (the prompt her last sent act carried, through the same render her acts
/// send) is the captured request's messages, exactly.
pub(crate) fn frontier_matches(view: &ContextView, captured: &[ChatMessage]) -> Result<(), CaptureMismatch> {
    if view.sent_events() == 0 {
        return Err(CaptureMismatch::NoAcceptedFrontier);
    }
    let frontier: Vec<ChatMessage> = view.sent_render().map(ContextMessage::to_chat).collect();
    if frontier.len() != captured.len() {
        return Err(CaptureMismatch::FrontierLength { frontier: frontier.len(), captured: captured.len() });
    }
    first_difference(&frontier, captured)?.map_or(Ok(()), |index| Err(CaptureMismatch::FrontierMessage { index })) // map_or: no difference matches
}

/// What this act would send, as its view assembled it.
pub(crate) struct ActPayload<'a> {
    pub(crate) system: &'a str,
    pub(crate) tools: &'a [NativeToolSpec],
    pub(crate) model: Option<&'a str>,
    pub(crate) messages: &'a [ChatMessage],
}

/// This act sends what the captured request sent, byte for byte, with its messages extending the
/// captured ones: the engine's KV for the captured request is a prefix of this act's prompt.
pub(crate) fn act_matches(act: &ActPayload<'_>, captured: &TextGenerationRequest) -> Result<(), CaptureMismatch> {
    let captured_system = captured.system_prompt.as_deref().unwrap_or_default(); // unwrap_or_default: a request with no system prompt sent none
    if act.system != captured_system {
        let at = act.system.bytes().zip(captured_system.bytes()).take_while(|(now, then)| now == then).count();
        return Err(CaptureMismatch::System { at });
    }
    let captured_tools = captured.tools.as_deref().unwrap_or_default(); // unwrap_or_default: a request with no tools sent none
    if act.tools.len() != captured_tools.len() {
        return Err(CaptureMismatch::ToolCount { now: act.tools.len(), captured: captured_tools.len() });
    }
    if let Some(index) = first_difference(act.tools, captured_tools)? {
        return Err(CaptureMismatch::Tool { index, name: act.tools[index].name.clone() });
    }
    if act.model != captured.model.as_deref() {
        return Err(CaptureMismatch::Model { now: act.model.map(str::to_string), captured: captured.model.clone() });
    }
    if act.messages.len() < captured.messages.len() {
        return Err(CaptureMismatch::Messages { index: act.messages.len() });
    }
    first_difference(&act.messages[..captured.messages.len()], &captured.messages)?.map_or(Ok(()), |index| Err(CaptureMismatch::Messages { index })) // map_or: no difference matches
}

/// The first index where two equal-length lists differ on the wire, or `None`.
fn first_difference<T: serde::Serialize>(now: &[T], then: &[T]) -> Result<Option<usize>, CaptureMismatch> {
    for (index, (now, then)) in now.iter().zip(then).enumerate() {
        let encode = |value: &T| serde_json::to_vec(value).map_err(|error| CaptureMismatch::Unencodable(error.to_string())); // boundary: compared as the bytes the engine is sent
        if encode(now)? != encode(then)? {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(text: &str) -> ChatMessage {
        ChatMessage { role: "user".to_string(), content: crate::ai::types::MessageContent::Text(text.to_string()), name: None }
    }

    fn captured(messages: Vec<ChatMessage>) -> TextGenerationRequest {
        TextGenerationRequest { messages, system_prompt: Some("system".to_string()), model: Some("model".to_string()), ..Default::default() }
    }

    // what this catches (card 9cf06ade, Codex's review): a capture adoption declared warm while what
    // the act sends differs from what the engine holds. Each part is compared on the wire, and the
    // refusal names where it differs.
    #[test]
    fn an_act_matches_the_capture_only_byte_for_byte() {
        let request = captured(vec![message("a"), message("b")]);
        let longer = [message("a"), message("b"), message("c")];
        let act = |system, model, messages| ActPayload { system, tools: &[], model, messages };
        assert_eq!(act_matches(&act("system", Some("model"), &longer), &request), Ok(()), "a prompt extending the captured one matches");
        assert_eq!(act_matches(&act("systEm", Some("model"), &longer), &request), Err(CaptureMismatch::System { at: 4 }));
        assert_eq!(act_matches(&act("system", Some("other"), &longer), &request), Err(CaptureMismatch::Model { now: Some("other".to_string()), captured: Some("model".to_string()) }));
        let changed = [message("a"), message("x"), message("c")];
        assert_eq!(act_matches(&act("system", Some("model"), &changed), &request), Err(CaptureMismatch::Messages { index: 1 }));
        assert_eq!(act_matches(&act("system", Some("model"), &longer[..1]), &request), Err(CaptureMismatch::Messages { index: 1 }));
    }

    // what this catches: two custodies with one digest, so an adoption asked over one adopts the other.
    #[test]
    fn a_custody_digest_tells_custodies_apart() {
        let unrecorded = custody_sha256(&HandsCustody::Unrecorded).expect("encodes");
        let corrupt = HandsCustody::Corrupt(super::super::hands_custody::CorruptHands { raw: serde_json::json!(1), error: "e".to_string(), original: None });
        assert_ne!(unrecorded, custody_sha256(&corrupt).expect("encodes"));
        assert_eq!(unrecorded, custody_sha256(&HandsCustody::Unrecorded).expect("encodes"), "and is stable");
    }
}
