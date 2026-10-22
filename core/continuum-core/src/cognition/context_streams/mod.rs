//! Her context as streams of keyed items ([`continuum_context`]): her conversation, her
//! actions, her thoughts and each standing grounding block. Every act her view appends only
//! the items that changed, so the engine reuses the whole previous prompt; turnover, the
//! only rewrite, collapses older items to their pointers once, at an epoch start
//! (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md, "sources are streams").
//!
//! One adapter per source, each small: a new source is a new file here, never a branch in
//! the prompt assembly.

pub(crate) mod actions;
pub(crate) mod blocks;
pub(crate) mod conversation;
pub(crate) mod thoughts;

use continuum_context::epoch::Block;

use crate::ai::types::ChatMessage;

/// Who a message of her view speaks as: her own voice, or anyone else's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// One message of her context view: who it speaks as, and what it says. Serializable, so her
/// view is saved with her working memory and survives a restart.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ContextMessage {
    pub role: Role,
    pub text: String,
}

impl ContextMessage {
    pub fn user(text: String) -> Self {
        Self { role: Role::User, text }
    }

    pub fn to_chat(&self) -> ChatMessage {
        ChatMessage::text(self.role.as_str(), self.text.clone())
    }
}

impl Block for ContextMessage {
    fn chars(&self) -> usize {
        // `str::chars`, not this trait's `chars`, which `String` also implements.
        self.text.as_str().chars().count()
    }
}

/// Each source's weight in the window's one allocation (`flex_allocate`, the same scale as
/// its other claims: required input 10, the reply up to 8). The blocks she works from keep
/// the grounding claim's 6; her conversation, thoughts and results share history's 5.
pub(crate) mod priority {
    pub const GROUNDING: u8 = 6;
    pub const CONVERSATION: u8 = 5;
    pub const THOUGHTS: u8 = 5;
    pub const ACTIONS: u8 = 5;
}

#[cfg(test)]
mod tests;
