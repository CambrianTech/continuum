use super::*;

/// Convert one recorded persona turn (recorder JSON) into a chat SFT example.
///
/// Returns `None` for non-`spoke` / empty / malformed turns — they aren't a
/// training pair. The recorder shape: `rustRequest.{systemPrompt,messageText,
/// recentHistory[]}` + `rustResponse` (a `PersonaResponse` enum, so a spoken
/// turn is `{"kind":"spoke","text":...}`). History items are attributed to
/// `assistant` when the sender is this persona, else `user`.
pub(super) fn turn_to_example(turn: &Value, include_system: bool, include_history: bool) -> Option<Value> {
    let req = turn.get("rustRequest")?;
    let resp = turn.get("rustResponse")?;

    // Only a turn the persona actually spoke is a (user → assistant) pair.
    if resp.get("kind").and_then(|k| k.as_str()) != Some("spoke") {
        return None;
    }
    let assistant = resp.get("text").and_then(|t| t.as_str())?.trim();
    let user = req.get("messageText").and_then(|t| t.as_str())?.trim();
    if user.is_empty() || assistant.is_empty() {
        return None;
    }

    let persona_name = turn.get("personaName").and_then(|v| v.as_str()).filter(|name| !name.is_empty());
    let mut messages: Vec<Value> = Vec::new();

    if include_system {
        if let Some(sys) = req.get("systemPrompt").and_then(|s| s.as_str()) {
            let sys = sys.trim();
            if !sys.is_empty() {
                messages.push(json!({ "role": "system", "content": sys }));
            }
        }
    }

    if include_history {
        if let Some(hist) = req.get("recentHistory").and_then(|h| h.as_array()) {
            for h in hist {
                let Some(text) = h.get("text").and_then(|t| t.as_str()) else {
                    continue;
                };
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let sender = h.get("senderName").and_then(|s| s.as_str());
                let role = if persona_name.is_some() && sender == persona_name {
                    "assistant"
                } else {
                    "user"
                };
                messages.push(json!({ "role": role, "content": text }));
            }
        }
    }

    messages.push(json!({ "role": "user", "content": user }));
    messages.push(json!({ "role": "assistant", "content": assistant }));
    Some(json!({ "messages": messages }))
}

/// Convert ONE live prompt-capture record — the glass-box turn: the system prompt
/// + the consolidated burst (`messages`) + the model's `response.text` — into an
/// SFT `{messages}` example, the SAME shape [`turn_to_example`] emits. The
/// prompt-capture IS the canonical experience: the LIVE WorkspaceCycle path writes
/// it on every turn, so the trainer reads the work the persona actually did without
/// a second recorder on the hot path (one turn-truth).
///
/// Structural curation only — QUALITY scoring is a later, pluggable slice (the
/// genome-loop curation layer). Here we drop only what is never a valid learning
/// target: an empty response. Everything else passes through, tagged with its
/// `skillAxis` so the consumer can select; whether a turn is GOOD is a judgment for
/// the curator, not this projection.
///
/// `skillAxis` is intrinsic to the turn: `"operational"` when the turn ACTED —
/// emitted a tool call, either as the model's literal answer (JSON-in-prompt:
/// `response.text` parses as a call) or as a structured `response.toolCalls` array
/// the adapter extracted — else `"domain"` (prose/answer only). Acting turns are the
/// OPERATIONAL training signal the genome loop needs (the "run the test, don't just
/// narrate" habit the Phase-0 baseline showed missing — `selfVerifyRate 0.0`). The
/// old code BLANKET-dropped any tool-call answer; that only ever protected the chat
/// axis (control-plane JSON leaking into room prose) while destroying exactly the
/// dev-task signal. We keep both and let the consumer filter: a chat-prose dataset
/// asks for `"domain"`, the dev-task/self-verify loop for `"operational"`.
pub(super) fn capture_to_example(
    request: &crate::ai::types::TextGenerationRequest,
    response: &crate::ai::types::TextGenerationResponse,
    include_system: bool,
) -> Option<Value> {
    let assistant = response.text.trim();
    if assistant.is_empty() {
        return None;
    }
    let acted = crate::ai::json_in_prompt_tools::parse_tool_call(assistant).is_some() || response.has_tool_calls();
    let skill_axis = if acted { "operational" } else { "domain" };
    let mut messages: Vec<Value> = Vec::new();
    if let Some(sys) = request.system_prompt.as_deref().map(str::trim).filter(|sys| include_system && !sys.is_empty()) {
        messages.push(json!({ "role": "system", "content": sys }));
    }
    for message in &request.messages {
        // a text turn is a pair's context; a multimodal or tool-protocol part is not prose to learn
        let crate::ai::types::MessageContent::Text(content) = &message.content else { continue };
        let content = content.trim();
        if !content.is_empty() {
            messages.push(json!({ "role": message.role, "content": content }));
        }
    }
    // Need at least one non-system (burst) message → an actual (context → reply) pair.
    if messages.iter().all(|m| m.get("role").and_then(|r| r.as_str()) == Some("system")) {
        return None;
    }
    messages.push(json!({ "role": "assistant", "content": assistant }));
    Some(json!({ "messages": messages, "skillAxis": skill_axis }))
}

/// Write examples as JSONL (one JSON object per line).
pub(super) fn write_jsonl(path: &Path, examples: &[Value]) -> Result<(), String> {
    let mut bytes = Vec::new();
    for example in examples {
        serde_json::to_writer(&mut bytes, example) // BOUNDARY: a dataset's JSONL is its disk format
            .map_err(|e| format!("Failed to write JSONL: {e}"))?;
        bytes.push(b'\n');
    }
    // whole or not at all: a reader never trains on half a split
    continuum_host::durable::write_replace(path, &bytes).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}
