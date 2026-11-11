//! One rendering of a request as the engine reads it, applied alike to the captured request and
//! to the faculty's replayed prompt, so their prefix-sharing numbers are comparable. The order is
//! the chat template's: the system message, the head tools, the messages, then the tail tools
//! (`tail_tools` ride after her messages, outside the prefix).

use continuum_cognition::ai::types::{ChatMessage, ContentPart, MessageContent, NativeToolSpec};
use sha2::{Digest, Sha256};

/// The whole request as one string, in the order the engine prefills it.
pub fn render(system: &str, tools: &[NativeToolSpec], messages: &[ChatMessage], tail_tools: &[NativeToolSpec]) -> String {
    let mut out = String::with_capacity(system.len() + messages.len() * 256);
    out.push_str("system\n");
    out.push_str(system);
    out.push('\n');
    push_tools(&mut out, tools);
    for message in messages {
        out.push_str(&message.role);
        out.push('\n');
        out.push_str(&body(message));
        out.push('\n');
    }
    push_tools(&mut out, tail_tools);
    out
}

fn push_tools(out: &mut String, tools: &[NativeToolSpec]) {
    for tool in tools {
        // BOUNDARY: a tool's schema reaches the engine as JSON; a spec that cannot serialize says so.
        match serde_json::to_string(tool) {
            Ok(json) => out.push_str(&json),
            Err(error) => out.push_str(&format!("<unserializable tool {}: {error}>", tool.name)),
        }
        out.push('\n');
    }
}

/// A message's content as text: its text, or for native protocol each part in order (a call's
/// name and arguments, a result's content, a marker for media).
pub fn body(message: &ChatMessage) -> String {
    match &message.content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Parts(parts) => parts.iter().map(part).collect::<Vec<_>>().join("\n"),
    }
}

fn part(part: &ContentPart) -> String {
    match part {
        ContentPart::Text { text } => text.clone(),
        ContentPart::ToolUse { id, name, input } => format!("<call {id} {name} {input}>"),
        ContentPart::ToolResult { tool_use_id, content, .. } => format!("<result {tool_use_id}>\n{content}"),
        ContentPart::Image { .. } => "<image>".to_owned(),
        ContentPart::Audio { .. } => "<audio>".to_owned(),
        ContentPart::Video { .. } => "<video>".to_owned(),
    }
}

/// A content key for a message: its role and its body, hashed.
pub fn key(message: &ChatMessage) -> String {
    hex(&Sha256::digest(format!("{}\0{}", message.role, body(message)).as_bytes()))
}

/// A content key for a text.
pub fn key_of(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The characters two prompts share from their start: what the engine reuses of the previous one.
pub fn shared_prefix_chars(previous: &str, next: &str) -> usize {
    previous.chars().zip(next.chars()).take_while(|(a, b)| a == b).count()
}
