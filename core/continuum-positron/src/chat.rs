//! Typed chat payloads — `ChatViewState`, the substrate-shaped view of
//! a room's chat that fills `StateEnvelope.payload` for `kind="chat"`.
//!
//! Per the positron design (consumer-typed payloads, positron frames):
//! the wire envelope carries `payload: unknown` because positron-core
//! does not define widget vocabularies. This module is continuum's
//! contribution to the chat widget's vocabulary — every field is
//! `#[derive(TS)]` exported, so the widget side's TypeScript shape is
//! generated from these structs at the same time the Rust types ship.
//!
//! ## Why structs, not `serde_json::Value`
//!
//! Per `[[strong-typing-across-boundaries]]`: a substrate that emits
//! `Value::Object` payloads has trained reviewers and renderers to
//! accept "best-effort" shapes. Renderers grow defensive parsers,
//! schemas drift, and one missing field becomes a UI bug instead of a
//! compile error. The substrate types here ARE the schema; ts-rs
//! mirrors them; the widget side reads typed objects, not `unknown`.
//!
//! ## What's in scope today
//!
//! - `ChatViewState`: top-level state — current room + roster + most
//!   recent messages.
//! - `ChatMessageView`: the message bits the chat widget needs to
//!   render a row (sender, content, timestamp).
//! - `PersonaSlotView`: a roster entry (persona id + display name +
//!   presence). Used by the right-rail roster + the message-row
//!   avatar.
//! - `SenderKind`: tagged enum — `Human`, `Persona`, `System`. The
//!   widget side keys avatar + styling off this discriminant.
//!
//! ## What's deferred
//!
//! Media attachments, reactions, threads, typing indicators —
//! deferred to follow-up slices once the substrate event source is
//! wired (subsequent task). The schema grows by extending these
//! structs; the wire kind string stays `"chat"`. Renderers that don't
//! know the new fields ignore them; the ts-rs flow makes those
//! additions visible to the widget side at the same time.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// One chat message — the bits the chat widget needs to render a row.
///
/// `id`, `room_id`, `sender_id` are continuum's substrate UUIDs
/// rendered as strings on the wire (the ts-rs default for `Uuid` is
/// `string`, which matches JSON behavior — `Uuid` isn't a JSON
/// primitive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatMessageView {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub sender_id: Uuid,
    /// Display name resolved at the substrate side. Renderers must
    /// not re-resolve from `sender_id` — that would re-introduce the
    /// widget-local source-of-truth cache positron's contract exists
    /// to prevent.
    pub sender_name: String,
    /// What kind of citizen sent this. The widget reads this
    /// discriminant for avatar / styling — no `if sender_name ==
    /// "system"` string-sniffing per `[[strong-typing-across-boundaries]]`.
    pub sender_kind: SenderKind,
    pub content: String,
    /// Unix-ms substrate-local time of arrival.
    #[ts(type = "number")]
    pub timestamp: u64,
}

/// What kind of citizen authored a message. Tagged enum on the wire
/// so the widget side reads a discriminant, not a stringly-typed
/// `sender_type` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[ts(export)]
pub enum SenderKind {
    /// Carbon — typed at the keyboard, dictated through STT, etc.
    Human,
    /// One of the substrate's own personas. Distinct from `Human` so
    /// the widget can render a different avatar treatment and AI
    /// observers can attribute provenance per
    /// `[[strong-typing-across-boundaries]]`.
    Persona,
    /// A substrate-generated event surfaced into chat — room joined,
    /// model swapped, a `[[observability-as-substrate]]` notification.
    /// Carries no `sender_id` semantic (id is `Uuid::nil()`).
    System,
}

/// A roster entry — a persona present in this room.
///
/// Roster is substrate-owned and refreshed on join / leave / spawn /
/// despawn. The widget never derives "who is here?" from message
/// senders — that's a stale-cache footgun #794 is currently a symptom
/// of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PersonaSlotView {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub display_name: String,
    /// `true` if the persona is currently attached and ready to
    /// receive turns. `false` for paged-out or spawning. The widget
    /// shows a presence indicator off this bit — single source of
    /// truth in the substrate.
    pub active: bool,
}

/// Top-level state for the `"chat"` widget kind. Fills
/// `StateEnvelope.payload` when `kind == "chat"`.
///
/// Substrate emits a fresh `ChatViewState` on every monotonic-revision
/// transition. The widget renders from the snapshot; positron's
/// `Renderer` contract requires no widget-local cache.
///
/// Today's shape is intentionally minimal — enough to render a chat
/// surface that closes §6 (#793 / #794 / #773) by structural design.
/// Reactions, threads, typing indicators, attachments grow the struct
/// (additive — additive ts-rs deltas are wire-compatible) in
/// follow-up slices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChatViewState {
    /// The room this snapshot describes.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Human-readable room name (e.g. `"general"`). Substrate-resolved;
    /// widget must not derive from URL slug.
    pub room_name: String,
    /// Most recent messages, oldest first. Bounded — substrate decides
    /// the window (see `MAX_MESSAGES_PER_SNAPSHOT` in the builder).
    /// Past-the-window history is pulled on demand through a
    /// `chat/history` command, not by carrying the whole transcript
    /// in every snapshot.
    pub messages: Vec<ChatMessageView>,
    /// Personas present in the room. Roster is bounded by spawn —
    /// the substrate hosts at most a handful at a time.
    pub roster: Vec<PersonaSlotView>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_kind_wire_shape_is_tagged() {
        // what this catches: regression where a refactor changes the
        // tagged-enum representation. The widget side keys
        // discriminant + per-variant fields off `tag = "kind"`; an
        // accidental flip to internally-tagged or untagged would
        // break TS clients silently.
        assert_eq!(
            serde_json::to_string(&SenderKind::Human).unwrap(),
            r#"{"kind":"human"}"#
        );
        assert_eq!(
            serde_json::to_string(&SenderKind::Persona).unwrap(),
            r#"{"kind":"persona"}"#
        );
        assert_eq!(
            serde_json::to_string(&SenderKind::System).unwrap(),
            r#"{"kind":"system"}"#
        );
    }

    #[test]
    fn chat_view_state_round_trips() {
        // what this catches: regression where a field rename / type
        // tweak breaks the serde shape. Minimum bar for a wire type.
        let room_id = Uuid::from_u128(0xa);
        let state = ChatViewState {
            room_id,
            room_name: "general".into(),
            messages: vec![ChatMessageView {
                id: Uuid::from_u128(0xb),
                room_id,
                sender_id: Uuid::from_u128(0xc),
                sender_name: "Joel".into(),
                sender_kind: SenderKind::Human,
                content: "hi".into(),
                timestamp: 1_700_000_000_000,
            }],
            roster: vec![PersonaSlotView {
                persona_id: Uuid::from_u128(0xd),
                display_name: "Helper".into(),
                active: true,
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: ChatViewState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}
