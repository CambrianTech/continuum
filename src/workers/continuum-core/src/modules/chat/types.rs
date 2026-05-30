//! Typed params + result for the chat module's commands.
//!
//! Every type here carries `#[derive(TS)]` and exports to
//! `shared/generated/chat/` so TS consumers get auto-generated
//! bindings — no hand-written duplicate types across the
//! Rust ↔ TS boundary.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ── chat/poll ────────────────────────────────────────────────────────

/// Params for `collaboration/chat/poll` (alias: `chat/poll`).
///
/// Mirrors the TS `ChatPollParams` shape that callers use today
/// (`src/commands/collaboration/chat/poll/shared/ChatPollTypes.ts`),
/// minus the legacy `room: string` name path. Room-name resolution
/// stays in the TS browser/CLI layer (or a future `channel/resolve`
/// command) — the kernel command takes an already-resolved `roomId`.
/// That keeps the kernel command compositional with the future
/// `channel` module rather than dragging room-name semantics into
/// every consumer of the chat surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/chat/ChatPollParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatPollParams {
    /// Restrict the poll to a specific room. Optional — omitting it
    /// returns latest messages across all rooms (the existing CLI
    /// "show me what's happening" smoke-test path).
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub room_id: Option<Uuid>,

    /// Anchor message. When set, return messages strictly AFTER this
    /// message's timestamp (in chronological order). When unset, return
    /// the latest `limit` messages.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub after_message_id: Option<Uuid>,

    /// Max number of messages to return. Defaults to 50 if the caller
    /// omits it.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub limit: Option<usize>,
}

/// Result of `chat/poll` — a chronologically-ordered list of message
/// records. The kernel-level wire response wraps this in
/// `CommandResponse<ChatPollResult>`, so callers see
/// `{ success, data: { messages, count }, error? }`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/chat/ChatPollResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatPollResult {
    /// Messages returned by the poll, in chronological order
    /// (earliest first) regardless of the underlying query direction.
    /// Each entry is the raw `ChatMessageEntity` payload as stored by
    /// the data module — no transformation, no field projection. TS
    /// consumers cast it via the existing `ChatMessageEntity` type
    /// (which itself is already ts-rs-exported from the entity layer).
    #[ts(type = "Array<unknown>")]
    pub messages: Vec<serde_json::Value>,

    /// Number of messages in `messages`. Convenience field so callers
    /// don't have to `.len()` on every consumer.
    #[ts(type = "number")]
    pub count: usize,

    /// Echo of the `after_message_id` the caller passed in, for
    /// pagination/loop ergonomics — the next poll round just keeps
    /// passing the most-recently-seen id.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub after_message_id: Option<Uuid>,
}

/// The collection chat messages live in. Matches
/// `ChatMessageEntity.collection` on the TS side. Centralized here so
/// every chat command in this module reaches the same shelf — and
/// when we change it (or migrate to a per-room collection scheme) it's
/// a single-edit move.
pub const CHAT_MESSAGES_COLLECTION: &str = "chat_messages";

/// Default `limit` when the caller omits it on `chat/poll`. Matches
/// the historical TS default (`params.limit || 50`).
pub const DEFAULT_POLL_LIMIT: usize = 50;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn poll_params_defaults_to_all_none() {
        let p = ChatPollParams::default();
        assert!(p.room_id.is_none());
        assert!(p.after_message_id.is_none());
        assert!(p.limit.is_none());
    }

    #[test]
    fn poll_params_round_trip_through_json_with_camel_case() {
        let raw = json!({
            "roomId": "00000000-0000-0000-0000-000000000001",
            "afterMessageId": "00000000-0000-0000-0000-000000000002",
            "limit": 10,
        });
        let parsed: ChatPollParams = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(parsed.limit, Some(10));
        assert!(parsed.room_id.is_some());
        assert!(parsed.after_message_id.is_some());

        let back = serde_json::to_value(&parsed).unwrap();
        // Round-trip preserves camelCase on the wire (matches the
        // existing TS callsite shape).
        assert_eq!(back["roomId"], raw["roomId"]);
        assert_eq!(back["afterMessageId"], raw["afterMessageId"]);
        assert_eq!(back["limit"], json!(10));
    }

    #[test]
    fn poll_params_accepts_missing_fields() {
        // Whole point of #[serde(default)] — empty object parses.
        let parsed: ChatPollParams = serde_json::from_value(json!({})).unwrap();
        assert!(parsed.room_id.is_none());
    }

    #[test]
    fn poll_result_omits_after_message_id_when_none() {
        let r = ChatPollResult {
            messages: vec![],
            count: 0,
            after_message_id: None,
        };
        let val = serde_json::to_value(&r).unwrap();
        assert!(
            !val.as_object().unwrap().contains_key("afterMessageId"),
            "missing after_message_id should round-trip as absent, not null"
        );
    }

    #[test]
    fn poll_result_includes_after_message_id_when_set() {
        let id = Uuid::new_v4();
        let r = ChatPollResult {
            messages: vec![],
            count: 0,
            after_message_id: Some(id),
        };
        let val = serde_json::to_value(&r).unwrap();
        assert_eq!(val["afterMessageId"], json!(id.to_string()));
    }
}
