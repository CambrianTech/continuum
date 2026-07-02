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
//!   render a row (sender, content, timestamp, opaque badges).
//! - `RosterSlotView`: a roster entry (member id + display name +
//!   presence + kind + opaque badges). Used by the right-rail roster
//!   + the message-row avatar.
//! - `SenderKind`: tagged enum — `Human`, `Agent`, `System`. The
//!   widget side keys avatar + styling off this discriminant.
//!
//! ## positron is general-purpose — it does NOT know "persona"
//!
//! positron is its own repo ("React + agents + modern terminals"),
//! consumed by continuum but adoptable by anyone. So its vocabulary is
//! *neutral*: an AI author is an `Agent`, full stop — positron never
//! learns whose agent it is. Framework-specific identity (a continuum
//! persona, an openclaw actor, a Hermes agent) rides the **opaque
//! `integrations` badge map**, transported and not interpreted —
//! exactly the move airc's `Identity.integrations` makes one layer
//! down (*"never interprets the values; it just persists +
//! transports"*). continuum reads `integrations["continuum.persona*"]`
//! at ITS app layer to style its own personas distinctly; a different
//! adopter reads their own key. The neutrality is fractal: airc
//! neutral (mesh) → positron neutral (view/state) → the app interprets.
//! See `docs/architecture/WIDGET-AS-STATE-KIND.md`.
//!
//! ## What's deferred
//!
//! Media attachments, reactions, threads, typing indicators —
//! deferred to follow-up slices once the **airc source wiring** lands
//! (the projection subscribes to airc's room stream; see the "State
//! ownership" note in `lib.rs`). The schema grows by extending these
//! structs; the wire kind string stays `"chat"`. Renderers that don't
//! know the new fields ignore them; the ts-rs flow makes those
//! additions visible to the widget side at the same time.
//!
//! ## These fields are a VIEW onto airc-owned state
//!
//! `room_id` is the airc `RoomId`; `roster` is airc presence; messages
//! ride airc's room event stream. This struct is the *projection* the
//! renderer reads — the substrate resolves display names / sender kind
//! ONCE here so the renderer never re-derives from ids (that re-derive
//! is the widget-local cache #794 is a symptom of). It is not a second
//! store of room truth; the airc row is the truth.

use std::collections::BTreeMap;

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
    /// Neutral author kind. The widget reads this discriminant for
    /// avatar / styling — no `if sender_name == "system"`
    /// string-sniffing per `[[strong-typing-across-boundaries]]`.
    /// `Agent` covers *every* AI author; whose agent it is (a
    /// continuum persona, an openclaw actor, …) is read from
    /// `integrations`, not baked into this enum.
    pub sender_kind: SenderKind,
    /// Opaque cross-system identity badges, transported straight from
    /// the authoritative airc `Identity.integrations` map. positron
    /// does NOT interpret these — the app layer does (continuum reads
    /// `continuum.persona*` to style its own personas; another adopter
    /// reads its own key). Empty when the sender's identity card has
    /// not yet resolved.
    #[ts(type = "Record<string, string>")]
    pub integrations: BTreeMap<String, String>,
    pub content: String,
    /// Unix-ms substrate-local time of arrival.
    #[ts(type = "number")]
    pub timestamp: u64,
}

/// What kind of citizen authored a message. Tagged enum on the wire
/// so the widget side reads a discriminant, not a stringly-typed
/// `sender_type` field.
///
/// **Neutral by design.** positron is general-purpose and knows
/// nothing about continuum, so there is no `Persona` variant — an AI
/// author is an `Agent`, and framework-specific identity rides
/// `integrations`. This keeps positron a repo others adopt without
/// learning continuum (see module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[ts(export)]
pub enum SenderKind {
    /// Carbon — typed at the keyboard, dictated through STT, etc.
    Human,
    /// Any AI author. Whose agent it is (continuum persona, openclaw
    /// actor, Hermes agent, remote peer) is NOT this enum's concern —
    /// that distinction lives in `integrations`, read at the app
    /// layer. positron treats all AI authors uniformly.
    Agent,
    /// A substrate-generated event surfaced into chat — room joined,
    /// model swapped, a `[[observability-as-substrate]]` notification.
    /// Carries no `sender_id` semantic (id is `Uuid::nil()`).
    System,
}

/// A roster entry — one member present in this room.
///
/// Roster is airc presence (surfaced through `RoomRosterSource`),
/// projected into this view and refreshed on join / leave / spawn /
/// despawn. The widget never derives "who is here?" from message
/// senders — that's a stale-cache footgun #794 is currently a symptom
/// of. This is also the lookup table the projection uses to resolve a
/// message sender's name / kind / badges by `sender_id`.
///
/// Neutral like the rest of positron: a member is identified by
/// `member_id` + `kind` + opaque `integrations`, never a
/// continuum-specific "persona" field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RosterSlotView {
    #[ts(type = "string")]
    pub member_id: Uuid,
    pub display_name: String,
    /// Neutral member kind (`Human` / `Agent` / `System`), resolved
    /// from the airc identity card. Lets the roster rail style AI
    /// members distinctly and lets `apply_message` resolve a sender's
    /// kind by id.
    pub kind: SenderKind,
    /// Opaque cross-system identity badges from the airc
    /// `Identity.integrations` map — transported, not interpreted (see
    /// `ChatMessageView.integrations`).
    #[ts(type = "Record<string, string>")]
    pub integrations: BTreeMap<String, String>,
    /// `true` if the member is currently attached and ready to receive
    /// turns. `false` for paged-out or spawning. The widget shows a
    /// presence indicator off this bit — single source of truth in the
    /// substrate.
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
    /// Members present in the room. Roster is bounded by presence —
    /// the substrate hosts at most a handful at a time.
    pub roster: Vec<RosterSlotView>,
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
            serde_json::to_string(&SenderKind::Agent).unwrap(),
            r#"{"kind":"agent"}"#
        );
        assert_eq!(
            serde_json::to_string(&SenderKind::System).unwrap(),
            r#"{"kind":"system"}"#
        );
    }

    #[test]
    fn integrations_ride_the_wire_opaquely() {
        // what this catches: the neutral-passthrough contract — a
        // framework badge (continuum.persona_id) must round-trip on a
        // message without positron growing a typed field for it. If a
        // refactor drops the map or renames the key, the app layer
        // loses its only channel for framework-specific identity.
        let mut integrations = BTreeMap::new();
        integrations.insert("continuum.persona_id".to_string(), "abc-123".to_string());
        let msg = ChatMessageView {
            id: Uuid::from_u128(0xb),
            room_id: Uuid::from_u128(0xa),
            sender_id: Uuid::from_u128(0xc),
            sender_name: "Helper".into(),
            sender_kind: SenderKind::Agent,
            integrations: integrations.clone(),
            content: "hi".into(),
            timestamp: 1_700_000_000_000,
        };
        let back: ChatMessageView =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(back.integrations, integrations);
        assert_eq!(back.sender_kind, SenderKind::Agent);
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
                integrations: BTreeMap::new(),
                content: "hi".into(),
                timestamp: 1_700_000_000_000,
            }],
            roster: vec![RosterSlotView {
                member_id: Uuid::from_u128(0xd),
                display_name: "Helper".into(),
                kind: SenderKind::Agent,
                integrations: BTreeMap::new(),
                active: true,
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: ChatViewState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}
