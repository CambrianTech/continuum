//! Typed params + result for the chat module's commands.
//!
//! Every type here carries `#[derive(TS)]` and exports to
//! `protocol/typescript/chat/` so TS consumers get auto-generated
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
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/chat/ChatPollParams.ts")]
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

    /// Anchor message for BACKWARD pagination — the scroll-back cursor.
    /// When set, return the `limit` messages strictly BEFORE this
    /// message's timestamp (still delivered in chronological order).
    /// The endless-scroll loop: render the live tail, then keep passing
    /// the OLDEST id on screen to page history out of durable storage.
    /// Mutually exclusive with `after_message_id`.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub before_message_id: Option<Uuid>,

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
#[ts(export, export_to = "../../../protocol/typescript/chat/ChatPollResult.ts")]
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

    /// Echo of the `before_message_id` the caller passed in — the next
    /// scroll-back page passes the OLDEST id it now holds. An empty
    /// `messages` with this set means history is exhausted.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub before_message_id: Option<Uuid>,
}

// ── chat/send ────────────────────────────────────────────────────────

/// Params for `collaboration/chat/send` (alias: `chat/send`).
///
/// The kernel command takes already-resolved UUIDs for both room and
/// sender. Name/identity resolution (sender priority chain:
/// explicit → owner → fallback; room name → uuid) stays in the TS
/// browser/CLI layer (or a future `channel/resolve` + `user/resolve`
/// pair). That keeps the kernel command compositional with future
/// resolver modules rather than dragging name resolution into every
/// caller of the chat surface.
///
/// Media externalization, full reply-to threading metadata, and vision
/// pre-warming are deferred to follow-up PRs — this first migration
/// stress-tests the dual-write composition (chat → data + chat → airc)
/// which is the substrate-shaped kink the design needed proof of.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/chat/ChatSendParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatSendParams {
    /// Destination room. The kernel command requires an
    /// already-resolved UUID; room-name lookup is the caller's job.
    #[ts(type = "string")]
    pub room_id: Uuid,

    /// Sender identity. The kernel command requires an
    /// already-resolved UUID; the sender priority chain (explicit
    /// senderId → human owner → fallback) is the caller's job.
    #[ts(type = "string")]
    pub sender_id: Uuid,

    /// Message text. Other media types (image, audio, file) are
    /// deferred — when media externalization migrates, this struct
    /// gains a `media: Option<Vec<MediaItem>>` field.
    pub text: String,

    /// Optional thread anchor. When set, both the stored message and
    /// the airc-published envelope carry this as the reply-to link.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub reply_to_id: Option<Uuid>,
}

/// Result of `chat/send`.
///
/// Carries the stored message's id (the local persistence ground
/// truth) AND the airc event id (the broadcast ground truth). When
/// airc partial-fails — data succeeded but airc failed — `event_id`
/// is `None` and `warning` names what happened.
///
/// The kernel-level `success` flag (on the `CommandResponse` envelope
/// wrapping this) is `true` whenever the message was stored locally.
/// An airc-only failure is NOT command-level failure: the message
/// IS in the local store, consumers see it via `chat/poll`, and a
/// future retry/sync mechanism heals the broadcast.
///
/// Hard failure (data/create failed) propagates as a typed `Err`
/// from the handler — the message never reaches the store, no airc
/// publish is attempted.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/chat/ChatSendResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct ChatSendResult {
    /// The stored message's UUID. Always present on success. Callers
    /// thread this when they need to follow up (edit, reply,
    /// delete) — it's the canonical id for the message regardless of
    /// whether the airc broadcast succeeded.
    #[ts(type = "string")]
    pub message_id: Uuid,

    /// The airc realtime event id, when broadcast succeeded. `None`
    /// means the local store has the message but the broadcast didn't
    /// land — see `warning`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event_id: Option<String>,

    /// Set when airc partial-failed. Names the failure mode so the
    /// caller can decide whether to retry, surface a UI warning,
    /// or just log. Absent on full success.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warning: Option<String>,
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

// The `chat/send` + `chat/poll` descriptors are now published by the typed
// `ActionCommand`s in `crate::commands::chat` (each `action_command!` block
// emits its own `register_command!`), so the registry self-assembles from the
// SAME site that owns the runtime object. The old enveloped `CommandSpec`
// stubs that used to live here were deleted in the DynCommand migration — a
// second registration of these names would hard-panic `command_registry()`
// on a duplicate NAME.

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
            before_message_id: None,
        };
        let val = serde_json::to_value(&r).unwrap();
        assert!(
            !val.as_object().unwrap().contains_key("afterMessageId"),
            "missing after_message_id should round-trip as absent, not null"
        );
        // The backward cursor echoes the same way: absent, never null.
        assert!(
            !val.as_object().unwrap().contains_key("beforeMessageId"),
            "missing before_message_id should round-trip as absent, not null"
        );
    }

    #[test]
    fn poll_result_includes_after_message_id_when_set() {
        let id = Uuid::new_v4();
        let r = ChatPollResult {
            messages: vec![],
            count: 0,
            after_message_id: Some(id),
            before_message_id: None,
        };
        let val = serde_json::to_value(&r).unwrap();
        assert_eq!(val["afterMessageId"], json!(id.to_string()));
    }
}
