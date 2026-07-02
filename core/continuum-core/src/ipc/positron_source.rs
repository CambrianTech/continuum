//! The airc → positron chat projection (task #29 airc source wiring).
//!
//! ## What this is
//!
//! A passive consumer of the airc room stream on the `MessageBus`, off
//! the transport hot path — the same shape as
//! [`crate::modules::airc_bridge_directive`]. On boot it subscribes via
//! `MessageBus::receiver()` and runs a consume loop on the runtime; each
//! recognized event folds into a [`ChatViewState`] and is written to the
//! thin-client [`Substrate`] with a monotonic revision. WS sessions
//! subscribed to `kind="chat"` (see [`crate::ipc::ws`]) then see the
//! projected view stream down as a `State` frame.
//!
//! ## Why the projection exists — airc owns the truth, positron caches it
//!
//! Per `[[airc-native-identity-rooms-security]]`: airc is the source of
//! room + roster truth. The positron `Substrate` is a *projection* the
//! thin-client fleet reads — never a second store of room state. This
//! module is the seam that keeps the projection tracking airc's owned
//! stream: airc publishes room events on the bus, we fold them into the
//! renderer-shaped [`ChatViewState`], the renderer reads the snapshot.
//! The renderer never re-derives identity from ids (the widget-local
//! cache footgun #794 exists to kill) — the projection resolves
//! `sender_name`/`sender_kind`/roster ONCE here, exactly as
//! [`crate::chat`-shaped] `ChatViewState` documents.
//!
//! ## Which airc streams map to `kind="chat"`
//!
//! Two bus streams fold onto the single existing `KnownKind::Chat`:
//!
//! - **`chat:posted`** — a posted message. Deserialized into
//!   [`AircChatPosted`], appended to the room's bounded message ring.
//! - **`presence:updated`** — the room roster changed. Deserialized into
//!   [`AircPresenceUpdate`], replaces the room's roster.
//!
//! Wall / coordination / kanban / widget state (task #89) are *different*
//! kinds and are deliberately out of scope here — they get their own
//! `KnownKind` + payload structs when those renderers land. This slice
//! projects exactly the chat surface, no more.
//!
//! ## The input contract, and why missing fields are skipped not faked
//!
//! Per `[[strong-typing-across-boundaries]]` + `[[fallbacks-are-illegal-fail-loud]]`:
//! the projection declares the typed shape it needs
//! ([`AircChatPosted`]/[`AircPresenceUpdate`]) rather than scraping the
//! bus `Value` field-by-field. An event that does not deserialize into
//! that shape is simply **not a chat event this projection can render** —
//! it is skipped (classification, exactly like
//! `airc_bridge_directive::classify_inbound` returning `None` for
//! non-directives), never partially rendered with fabricated identity.
//! The continuum-side turn streamer (task #84) is the emitter that fills
//! this contract; until it lands, well-formed events are exercised by the
//! unit tests below, proving the fold end-to-end through the real
//! `Substrate`.
//!
//! ## Single active room (single-cache-per-kind)
//!
//! The `Substrate` cache is keyed by `kind` string alone, so it holds one
//! `chat` envelope at a time — the currently-focused room's view. On an
//! event whose `room_id` differs from the accumulator's current room, the
//! projection resets to the new room (clears ring + roster). Per-room
//! instancing (many rooms cached at once) is kind-instancing, deferred
//! with the same `RevisionKey` note in `continuum-positron/src/kinds.rs`.

use std::collections::VecDeque;
use std::sync::Arc;

use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{
    ChatMessageView, ChatViewState, KnownKind, PersonaSlotView, SenderKind, StateBuilder, Substrate,
};

use crate::runtime::MessageBus;

/// Bus event prefix carrying posted-message payloads. A cheap prefix
/// check keeps presence/media/transport events out of the message arm.
const CHAT_POSTED: &str = "chat:posted";
/// Bus event carrying a room roster/presence delta.
const PRESENCE_UPDATED: &str = "presence:updated";
/// Bounded message window carried in each snapshot. Matches the
/// `chat/poll` default (`ChatPollParams.limit` defaults to 50) — the
/// renderer shows a recent window; deeper history is a `chat/history`
/// pull, not a fatter snapshot (see `ChatViewState.messages` doc).
const MAX_MESSAGES_PER_SNAPSHOT: usize = 50;

/// Typed `chat:posted` payload — the fields the projection needs to fold
/// a message into the renderer view. camelCase to match the bus JSON
/// convention (`chat/types.rs` wire shapes are camelCase). `sender_kind`
/// reuses the canonical [`SenderKind`] wire representation
/// (`{"kind":"human"}`) — one type, one shape, per the compression
/// principle. `room_name` is required: the emitter resolved the room to
/// post, so it knows the name; an event without it is incomplete and
/// skipped.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircChatPosted {
    message_id: Uuid,
    room_id: Uuid,
    room_name: String,
    sender_id: Uuid,
    sender_name: String,
    sender_kind: SenderKind,
    content: String,
    timestamp: u64,
}

/// Typed `presence:updated` payload — a full roster snapshot for a room.
/// The roster is airc presence; the projection replaces (not merges) the
/// room's roster from this snapshot so a leave is reflected by absence,
/// never a stale merged entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircPresenceUpdate {
    room_id: Uuid,
    room_name: String,
    roster: Vec<AircPresenceSlot>,
}

/// One roster entry inside an [`AircPresenceUpdate`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircPresenceSlot {
    persona_id: Uuid,
    display_name: String,
    active: bool,
}

/// Accumulates airc room events into the renderer-shaped
/// [`ChatViewState`] and writes each transition to the [`Substrate`].
///
/// Holds the single focused room's state (see the single-active-room note
/// in the module docs). Not `Clone` — one owner per projection; the
/// consume loop owns it.
struct ChatProjection {
    substrate: Substrate,
    builder: StateBuilder,
    /// The room this accumulator currently describes. `None` before the
    /// first recognized event.
    room_id: Option<Uuid>,
    room_name: String,
    /// Bounded ring of recent messages, oldest first (the order
    /// `ChatViewState.messages` is documented to carry).
    messages: VecDeque<ChatMessageView>,
    roster: Vec<PersonaSlotView>,
}

impl ChatProjection {
    fn new(substrate: Substrate) -> Self {
        Self {
            substrate,
            // The projection is the SOLE writer of the `chat` kind, so
            // its own standalone `Revisions` well is the authoritative
            // monotonic source for that kind.
            builder: StateBuilder::standalone(),
            room_id: None,
            room_name: String::new(),
            messages: VecDeque::new(),
            roster: Vec::new(),
        }
    }

    /// Switch the accumulator to `room_id`/`room_name` if it differs from
    /// the current room, clearing the prior room's ring + roster. The
    /// single-cache-per-kind substrate holds one room's view at a time.
    fn focus_room(&mut self, room_id: Uuid, room_name: &str) {
        if self.room_id != Some(room_id) {
            self.room_id = Some(room_id);
            self.messages.clear();
            self.roster.clear();
        }
        // Always adopt the latest resolved name for the focused room.
        self.room_name = room_name.to_string();
    }

    /// Fold a posted message into the view and store the new snapshot.
    /// Idempotent on `message_id`: a redelivered event (the bus is
    /// best-effort) does not double-append.
    fn apply_message(&mut self, msg: AircChatPosted) {
        self.focus_room(msg.room_id, &msg.room_name);
        if self.messages.iter().any(|m| m.id == msg.message_id) {
            return;
        }
        self.messages.push_back(ChatMessageView {
            id: msg.message_id,
            room_id: msg.room_id,
            sender_id: msg.sender_id,
            sender_name: msg.sender_name,
            sender_kind: msg.sender_kind,
            content: msg.content,
            timestamp: msg.timestamp,
        });
        while self.messages.len() > MAX_MESSAGES_PER_SNAPSHOT {
            self.messages.pop_front();
        }
        self.store();
    }

    /// Replace the focused room's roster from a presence snapshot and
    /// store the new view.
    fn apply_presence(&mut self, update: AircPresenceUpdate) {
        self.focus_room(update.room_id, &update.room_name);
        self.roster = update
            .roster
            .into_iter()
            .map(|s| PersonaSlotView {
                persona_id: s.persona_id,
                display_name: s.display_name,
                active: s.active,
            })
            .collect();
        self.store();
    }

    /// Frame the current accumulator as a `chat` `StateEnvelope` and write
    /// it to the substrate (cache + live broadcast).
    fn store(&self) {
        let Some(room_id) = self.room_id else {
            return;
        };
        let view = ChatViewState {
            room_id,
            room_name: self.room_name.clone(),
            messages: self.messages.iter().cloned().collect(),
            roster: self.roster.clone(),
        };
        self.substrate
            .store(self.builder.session(KnownKind::Chat, view));
    }
}

/// Classify a bus event into a typed projection input, or `None` when the
/// event is not a chat/presence event this projection renders. Pure — no
/// substrate side effect — so it's unit-testable without a live bus.
enum ProjectionInput {
    Message(AircChatPosted),
    Presence(AircPresenceUpdate),
}

fn classify(name: &str, payload: &serde_json::Value) -> Option<ProjectionInput> {
    // The airc bus wraps event bodies under a `payload` key (see
    // `airc_bridge_directive::str_field`); accept a nested `payload`
    // object, else the top-level value.
    let body = payload.get("payload").unwrap_or(payload);
    match name {
        CHAT_POSTED => serde_json::from_value::<AircChatPosted>(body.clone())
            .ok()
            .map(ProjectionInput::Message),
        PRESENCE_UPDATED => serde_json::from_value::<AircPresenceUpdate>(body.clone())
            .ok()
            .map(ProjectionInput::Presence),
        _ => None,
    }
}

/// Subscribe the chat projection to the bus and run its consume loop on
/// `rt`. Holds a clone of the same [`Substrate`] the WS server serves, so
/// the projected view reaches subscribed thin clients. Runs for the
/// process lifetime.
///
/// Subscribes synchronously (before spawning) so no publish can race
/// ahead of the receiver — the same ordering discipline
/// `airc_bridge_directive::spawn_consumer` uses.
pub fn spawn(rt: &tokio::runtime::Handle, bus: Arc<MessageBus>, substrate: Substrate) {
    let mut rx = bus.receiver();
    rt.spawn(async move {
        let mut projection = ChatProjection::new(substrate);
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(input) = classify(&event.name, &event.payload) {
                        match input {
                            ProjectionInput::Message(m) => projection.apply_message(m),
                            ProjectionInput::Presence(p) => projection.apply_presence(p),
                        }
                    }
                }
                // Fell behind the broadcast buffer. The projection is a
                // last-good cache of a live stream, not guaranteed
                // delivery — skip the gap and keep folding. The next
                // event re-establishes a coherent snapshot.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn posted(room: Uuid, msg: Uuid, text: &str) -> serde_json::Value {
        json!({
            "messageId": msg,
            "roomId": room,
            "roomName": "general",
            "senderId": Uuid::from_u128(0xc),
            "senderName": "Joel",
            "senderKind": { "kind": "human" },
            "content": text,
            "timestamp": 1_700_000_000_000u64,
        })
    }

    fn current_chat(substrate: &Substrate) -> ChatViewState {
        let env = substrate
            .cache()
            .get(KnownKind::Chat.wire_name())
            .expect("a chat envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a ChatViewState")
    }

    #[test]
    fn message_event_projects_into_the_substrate() {
        // what this catches: regression where a chat:posted event does
        // not reach the substrate as a ChatViewState — the whole point
        // of the airc source wiring. Drives the pure fold (no bus) and
        // asserts the cache holds the projected message.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let m = Uuid::from_u128(0xb);
        match classify(CHAT_POSTED, &posted(room, m, "hi")).unwrap() {
            ProjectionInput::Message(msg) => p.apply_message(msg),
            _ => panic!("chat:posted must classify as a Message"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room);
        assert_eq!(view.room_name, "general");
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].id, m);
        assert_eq!(view.messages[0].content, "hi");
        assert_eq!(view.messages[0].sender_kind, SenderKind::Human);
    }

    #[test]
    fn presence_event_projects_the_roster() {
        // what this catches: regression where presence:updated does not
        // fold into the roster — the second airc stream (outlier B,
        // maximally different from the message stream) proving the
        // accumulator holds two independent airc shapes on one kind.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let payload = json!({
            "roomId": room,
            "roomName": "general",
            "roster": [
                { "personaId": Uuid::from_u128(0xd), "displayName": "Helper", "active": true },
                { "personaId": Uuid::from_u128(0xe), "displayName": "Critic", "active": false },
            ],
        });
        match classify(PRESENCE_UPDATED, &payload).unwrap() {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.roster.len(), 2);
        assert_eq!(view.roster[0].display_name, "Helper");
        assert!(view.roster[0].active);
        assert!(!view.roster[1].active);
    }

    #[test]
    fn message_and_presence_compose_on_one_chat_view() {
        // what this catches: regression where the two streams clobber
        // each other instead of composing — a presence update must
        // preserve the message ring for the same room, and vice-versa.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0xb), "hi")).unwrap()
        {
            p.apply_message(m);
        }
        let presence = json!({
            "roomId": room,
            "roomName": "general",
            "roster": [{ "personaId": Uuid::from_u128(0xd), "displayName": "Helper", "active": true }],
        });
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.messages.len(), 1, "roster update kept the message");
        assert_eq!(view.roster.len(), 1, "message left the roster intact");
    }

    #[test]
    fn redelivered_message_is_idempotent() {
        // what this catches: regression where a best-effort bus
        // redelivery double-appends the same message. Dedup by
        // message_id keeps the snapshot stable under redelivery.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let m = Uuid::from_u128(0xb);
        for _ in 0..3 {
            if let ProjectionInput::Message(msg) =
                classify(CHAT_POSTED, &posted(room, m, "hi")).unwrap()
            {
                p.apply_message(msg);
            }
        }
        assert_eq!(current_chat(&substrate).messages.len(), 1);
    }

    #[test]
    fn switching_rooms_resets_the_accumulator() {
        // what this catches: regression where the single-cache-per-kind
        // model leaks room A's messages/roster into room B. Focusing a
        // new room must clear the prior room's ring + roster.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xf);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_a, Uuid::from_u128(0xb), "a")).unwrap()
        {
            p.apply_message(m);
        }
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room_b, Uuid::from_u128(0xc), "b")).unwrap()
        {
            p.apply_message(m);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room_b);
        assert_eq!(view.messages.len(), 1, "room A's message was cleared");
        assert_eq!(view.messages[0].content, "b");
    }

    #[test]
    fn foreign_and_malformed_events_are_skipped() {
        // what this catches: regression where a non-chat event or a
        // chat:posted missing required identity fields gets partially
        // rendered (fabricated identity) instead of skipped. Per
        // [[fallbacks-are-illegal-fail-loud]]: an event that can't
        // deserialize into the contract is not-a-chat-event → None.
        assert!(classify("media:frame", &json!({ "bytes": 4 })).is_none());
        // chat:posted missing senderName / senderKind / ids → not renderable.
        assert!(classify(CHAT_POSTED, &json!({ "content": "hi" })).is_none());
    }

    #[test]
    fn revision_advances_monotonically_across_stores() {
        // what this catches: regression where successive projections
        // stamp a stale or non-monotonic revision — the session-protocol
        // last_seen replay routes by revision, so a flat revision would
        // stop the renderer from seeing updates.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x1), "one")).unwrap()
        {
            p.apply_message(m);
        }
        let r1 = substrate
            .cache()
            .get(KnownKind::Chat.wire_name())
            .unwrap()
            .revision;
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x2), "two")).unwrap()
        {
            p.apply_message(m);
        }
        let r2 = substrate
            .cache()
            .get(KnownKind::Chat.wire_name())
            .unwrap()
            .revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }
}
