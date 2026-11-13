//! Her native protocol (her tool calls, their results) as messages of her view, so her act's tail
//! (`tail::Tail`) renders her calls and results in the exact protocol the engine is sent, and once
//! it joins her view the next act renders them back as the same protocol. Before this the tail was
//! re-rendered every act: her newest call and result as native protocol at the end of one prompt,
//! as a `[result #n]` item of her actions in the next, so the engine re-prefilled the whole
//! previous tail (the M5, 2026-11-06: 2,822 s over 78 acts).

use crate::ai::types::{ChatMessage, ContentPart, MessageContent};

use super::{ContextMessage, Role};

/// One part of a native message, hashable so the epoch can compare and fingerprint it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NativePart {
    Text(String),
    ToolUse { id: String, name: String, input: ToolInput },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

impl NativePart {
    /// `part` as her view holds it; `None` for media, which her view never holds.
    fn of(part: &ContentPart) -> Option<Self> {
        match part {
            ContentPart::Text { text } => Some(Self::Text(text.clone())),
            ContentPart::ToolUse { id, name, input } => Some(Self::ToolUse { id: id.clone(), name: name.clone(), input: ToolInput(input.clone()) }),
            ContentPart::ToolResult { tool_use_id, content, is_error } => {
                Some(Self::ToolResult { tool_use_id: tool_use_id.clone(), content: content.clone(), is_error: *is_error })
            }
            ContentPart::Image { .. } | ContentPart::Audio { .. } | ContentPart::Video { .. } => None,
        }
    }

    /// The part as the request carries it.
    fn to_part(&self) -> ContentPart {
        match self {
            Self::Text(text) => ContentPart::Text { text: text.clone() },
            Self::ToolUse { id, name, input } => ContentPart::ToolUse { id: id.clone(), name: name.clone(), input: input.0.clone() },
            Self::ToolResult { tool_use_id, content, is_error } => {
                ContentPart::ToolResult { tool_use_id: tool_use_id.clone(), content: content.clone(), is_error: *is_error }
            }
        }
    }
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
    /// A native message of `parts`: what it costs and is hashed by is the text it carries, a call
    /// as its name and arguments.
    pub fn native(role: Role, parts: Vec<NativePart>) -> Self {
        let text = parts
            .iter()
            .map(|part| match part {
                NativePart::Text(text) => text.clone(),
                NativePart::ToolUse { name, input, .. } => format!("{name}({})", input.0),
                NativePart::ToolResult { content, .. } => content.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self { role, text, native: Some(parts) }
    }

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
        match &message.content {
            MessageContent::Text(text) => Ok(Self::new(role, text.clone())),
            // `holdable` refused a message with media above.
            MessageContent::Parts(parts) => Ok(Self::native(role, parts.iter().filter_map(NativePart::of).collect())),
        }
    }

    /// Whether `sent` shows this message: it exactly, or it with parts attached after it when it
    /// was rendered (her newest result's pixels and visual notes, `tail::Tail::attach`), which her
    /// view never holds.
    pub fn sent_as(&self, sent: &ChatMessage) -> bool {
        if sent.role != self.role.as_str() {
            return false;
        }
        match (&self.native, &sent.content) {
            (None, MessageContent::Text(text)) => *text == self.text,
            (None, MessageContent::Parts(parts)) => matches!(parts.first(), Some(ContentPart::Text { text }) if *text == self.text),
            (Some(_), MessageContent::Text(_)) => false,
            (Some(native), MessageContent::Parts(parts)) => {
                native.len() <= parts.len() && native.iter().zip(parts).all(|(native, part)| NativePart::of(part).as_ref() == Some(native))
            }
        }
    }

    /// The native parts as the request carries them.
    pub(super) fn native_content(parts: &[NativePart]) -> MessageContent {
        MessageContent::Parts(parts.iter().map(NativePart::to_part).collect())
    }
}
