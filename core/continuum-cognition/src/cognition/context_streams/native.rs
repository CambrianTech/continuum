//! Her native protocol (her tool calls, their results) as messages of her view, so a prompt's tail
//! joins her view in the bytes it was sent (`continuum_context::view::View::append_sent`) and the
//! next act renders it back as the same protocol. Before this the tail was re-rendered every act:
//! her newest call and result as native protocol at the end of one prompt, as a `[result #n]` item
//! of her actions in the next, so the engine re-prefilled the whole previous tail (the M5,
//! 2026-11-06: 2,822 s over 78 acts).

use crate::ai::types::{ChatMessage, ContentPart, MessageContent};

use super::{ContextMessage, Role};

/// One part of a native message, hashable so the epoch can compare and fingerprint it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NativePart {
    Text(String),
    ToolUse { id: String, name: String, input: ToolInput },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

/// A call's arguments as she sent them. Hashed by their JSON text: one value, one text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ToolInput(pub serde_json::Value);

impl std::hash::Hash for ToolInput {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_string().hash(state);
    }
}

/// Why a sent message cannot join her view as sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotKept {
    /// Pixels, audio or video ride the message; her view keeps text and protocol, and a media
    /// part re-renders from its producer instead.
    Media,
    /// A role her view has no voice for (a system message).
    Role,
}

impl ContextMessage {
    /// Whether her view can hold `message` exactly as sent, without building the copy.
    pub fn holdable(message: &ChatMessage) -> Result<(), NotKept> {
        if !matches!(message.role.as_str(), "assistant" | "user") {
            return Err(NotKept::Role);
        }
        match &message.content {
            MessageContent::Parts(parts) if parts.iter().any(|part| matches!(part, ContentPart::Image { .. } | ContentPart::Audio { .. } | ContentPart::Video { .. })) => Err(NotKept::Media),
            _ => Ok(()),
        }
    }

    /// `message` as her view holds it: its text, or its native parts, so [`ContextMessage::to_chat`]
    /// renders the same message back.
    pub fn from_chat(message: &ChatMessage) -> Result<Self, NotKept> {
        Self::holdable(message)?;
        let role = if message.role == "assistant" { Role::Assistant } else { Role::User };
        let parts = match &message.content {
            MessageContent::Text(text) => return Ok(Self::new(role, text.clone())),
            MessageContent::Parts(parts) => parts,
        };
        let native: Vec<NativePart> = parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(NativePart::Text(text.clone())),
                ContentPart::ToolUse { id, name, input } => Some(NativePart::ToolUse { id: id.clone(), name: name.clone(), input: ToolInput(input.clone()) }),
                ContentPart::ToolResult { tool_use_id, content, is_error } => {
                    Some(NativePart::ToolResult { tool_use_id: tool_use_id.clone(), content: content.clone(), is_error: *is_error })
                }
                // `holdable` refused a message with media above.
                ContentPart::Image { .. } | ContentPart::Audio { .. } | ContentPart::Video { .. } => None,
            })
            .collect();
        // What the message costs and is hashed by: the text it carries, a call as its name and
        // arguments.
        let text = native
            .iter()
            .map(|part| match part {
                NativePart::Text(text) => text.clone(),
                NativePart::ToolUse { name, input, .. } => format!("{name}({})", input.0),
                NativePart::ToolResult { content, .. } => content.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Self { role, text, native: Some(native) })
    }

    /// The native parts as the request carries them.
    pub(super) fn native_content(parts: &[NativePart]) -> MessageContent {
        MessageContent::Parts(
            parts
                .iter()
                .map(|part| match part {
                    NativePart::Text(text) => ContentPart::Text { text: text.clone() },
                    NativePart::ToolUse { id, name, input } => ContentPart::ToolUse { id: id.clone(), name: name.clone(), input: input.0.clone() },
                    NativePart::ToolResult { tool_use_id, content, is_error } => {
                        ContentPart::ToolResult { tool_use_id: tool_use_id.clone(), content: content.clone(), is_error: *is_error }
                    }
                })
                .collect(),
        )
    }
}
