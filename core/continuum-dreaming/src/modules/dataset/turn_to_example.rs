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

    let persona_name = turn
        .get("personaName")
        .and_then(|v| v.as_str())
        .unwrap_or("");
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
                let sender = h.get("senderName").and_then(|s| s.as_str()).unwrap_or("");
                let role = if !persona_name.is_empty() && sender == persona_name {
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
pub(super) fn capture_to_example(cap: &Value, include_system: bool) -> Option<Value> {
    let assistant = cap.get("response")?.get("text")?.as_str()?.trim();
    if assistant.is_empty() {
        return None;
    }
    let acted = crate::ai::json_in_prompt_tools::parse_tool_call(assistant).is_some()
        || cap
            .get("response")
            .and_then(|r| r.get("toolCalls"))
            .and_then(|t| t.as_array())
            .is_some_and(|a| !a.is_empty());
    let skill_axis = if acted { "operational" } else { "domain" };
    let mut messages: Vec<Value> = Vec::new();
    if include_system {
        if let Some(sys) = cap.get("system").and_then(|s| s.as_str()) {
            let sys = sys.trim();
            if !sys.is_empty() {
                messages.push(json!({ "role": "system", "content": sys }));
            }
        }
    }
    if let Some(arr) = cap.get("messages").and_then(|m| m.as_array()) {
        for m in arr {
            let (Some(role), Some(content)) = (
                m.get("role").and_then(|r| r.as_str()),
                m.get("content").and_then(|c| c.as_str()),
            ) else {
                continue;
            };
            let content = content.trim();
            if content.is_empty() {
                continue;
            }
            messages.push(json!({ "role": role, "content": content }));
        }
    }
    // Need at least one non-system (burst) message → an actual (context → reply) pair.
    if messages
        .iter()
        .all(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
    {
        return None;
    }
    messages.push(json!({ "role": "assistant", "content": assistant }));
    Some(json!({ "messages": messages, "skillAxis": skill_axis }))
}

/// Write examples as JSONL (one JSON object per line).
pub(super) fn write_jsonl(path: &Path, examples: &[Value]) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path)
        .map_err(|e| format!("Failed to create {}: {e}", path.display()))?;
    let mut writer = std::io::BufWriter::new(file);

    for example in examples {
        serde_json::to_writer(&mut writer, example)
            .map_err(|e| format!("Failed to write JSONL: {e}"))?;
        writeln!(&mut writer).map_err(|e| format!("Failed to write newline: {e}"))?;
    }

    Ok(())
}
