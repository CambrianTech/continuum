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
//! cache footgun #794 exists to kill) — the projection resolves a
//! sender's `sender_name`/`sender_kind`/`integrations` ONCE here, by
//! looking the id up in the roster, exactly as `ChatViewState` documents.
//!
//! ## Which airc streams map to `kind="chat"`
//!
//! Two bus streams fold onto the single existing `ChatViewState::KIND`, and
//! the split between them mirrors airc's own message/identity split:
//!
//! - **`chat:posted`** — a posted message. Deserialized into the **thin**
//!   [`AircChatPosted`] (core message facts only — no identity), appended
//!   to the room's bounded message ring. Identity is resolved from the
//!   roster at fold time.
//! - **`presence:updated`** — the room roster changed. Deserialized into
//!   [`AircPresenceUpdate`], each entry an airc member joined with its
//!   identity card (neutral `kind` + opaque `integrations`), replacing
//!   the room's roster. This is the identity lookup table for messages.
//!
//! Wall / coordination / kanban / widget state (task #89) are *different*
//! kinds and are deliberately out of scope here — they get their own
//! `KIND` const + payload structs when those renderers land. This slice
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
//! — the revision key would extend from the bare kind string to a
//! `(room_id, kind)` tuple (see `continuum-positron/src/revisions.rs`).

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use continuum_positron::{
    ChatMessageView, ChatViewState, Provenance, RosterSlotView, SenderKind, StateBuilder,
    Substrate,
};

use crate::runtime::MessageBus;

/// Bus event prefix carrying posted-message payloads. A cheap prefix
/// check keeps presence/media/transport events out of the message arm.
///
/// `pub(crate)` because the EMITTER
/// (`airc::inbound_attach::publish_transcript_event`) and this CONSUMER
/// must agree on the wire name — one string, one source of truth
/// (compression principle). The emitter imports this const rather than
/// re-typing the literal.
pub(crate) const CHAT_POSTED: &str = "chat:posted";
/// Bus event carrying a room roster/presence delta.
///
/// `pub(crate)` for the same reason as [`CHAT_POSTED`]: the presence
/// EMITTER (`crate::ipc::positron_presence`) and this CONSUMER must agree
/// on the wire name — one string, one source of truth.
pub(crate) const PRESENCE_UPDATED: &str = "presence:updated";
/// Bounded message window carried in each snapshot. Matches the
/// `chat/poll` default (`ChatPollParams.limit` defaults to 50) — the
/// renderer shows a recent window; deeper history is a `chat/history`
/// pull, not a fatter snapshot (see `ChatViewState.messages` doc).
const MAX_MESSAGES_PER_SNAPSHOT: usize = 50;

/// A provisional display label for a sender whose identity card has not
/// yet folded in through presence — the first 8 chars of the peer id,
/// prefixed. Deliberately unmistakable for a real name so a stuck-
/// provisional row is visible, not silently wrong. Upgraded in place the
/// instant `presence:updated` resolves the card
/// ([[fallbacks-are-illegal-fail-loud]]).
///
/// `pub(crate)` so the presence EMITTER
/// (`crate::ipc::positron_presence`) labels a present-but-unnamed peer
/// with the SAME short-peer form — one provisional-label source, not two
/// (compression principle).
pub(crate) fn provisional_sender_name(sender_id: Uuid) -> String {
    let simple = sender_id.simple().to_string();
    format!("peer-{}", &simple[..8])
}

/// Typed `chat:posted` payload — **thin by design**. It carries only the
/// authoritative core facts airc owns about a posted message:
/// `message_id` (airc's `event_id`), `room_id`, `sender_id`, `content`,
/// `timestamp`. camelCase matches the bus JSON convention.
///
/// Identity (sender name / kind / badges) is NOT a message fact — it is
/// an identity-card fact that lives on the airc `Identity`, surfaced
/// through `presence:updated`. The projection resolves it downstream by
/// looking `sender_id` up in the roster (see `apply_message`). A message
/// whose sender has no card yet renders provisionally
/// ([[fallbacks-are-illegal-fail-loud]]: a provisional projection
/// pending authoritative truth, never a fabricated identity). Keeping
/// the emitter thin is what lets Hermes / openclaw / a python foundry
/// emit `chat:posted` without knowing continuum's identity model.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AircChatPosted {
    message_id: Uuid,
    room_id: Uuid,
    sender_id: Uuid,
    content: String,
    timestamp: u64,
}

/// Typed `presence:updated` payload — a full roster snapshot for a room.
/// The roster is airc presence joined with each member's identity card;
/// the projection replaces (not merges) the room's roster from this
/// snapshot so a leave is reflected by absence, never a stale merged
/// entry. This is also the lookup table `apply_message` resolves a
/// sender's name / kind / badges from.
///
/// `pub(crate)` + `Serialize` because the EMITTER
/// (`crate::ipc::positron_presence`) builds and serializes THIS SAME
/// struct — one wire shape defined once, both sides agree by
/// construction rather than by a hand-copied JSON literal (compression
/// principle). The emitter/consumer round-trip is pinned by a test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AircPresenceUpdate {
    pub(crate) room_id: Uuid,
    pub(crate) room_name: String,
    pub(crate) roster: Vec<AircPresenceSlot>,
}

/// One roster entry inside an [`AircPresenceUpdate`] — a member joined
/// with its airc identity card. `kind` is the neutral author kind
/// (`Human` / `Agent` / `System`) the presence emitter derives from the
/// member's `runtime`; `integrations` is the opaque cross-system badge
/// map from `Identity.integrations`, transported straight through and
/// interpreted only at continuum's app layer (renderer reads
/// `continuum.persona*`); `provenance` is the accountability slot (the
/// member's verifiable origin — `runtime` today, trust tier +
/// verification later, no wire break).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AircPresenceSlot {
    pub(crate) member_id: Uuid,
    pub(crate) display_name: String,
    pub(crate) kind: SenderKind,
    #[serde(default)]
    pub(crate) integrations: BTreeMap<String, String>,
    /// The accountability half of the identity card. `#[serde(default)]`
    /// so a slot predating the field folds as `Provenance::unresolved()`
    /// (honest empty), symmetric with `integrations` — never a fabricated
    /// origin ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default)]
    pub(crate) provenance: Provenance,
    pub(crate) active: bool,
}

/// A sender's identity resolved from the roster — name + neutral kind +
/// opaque badges + accountability provenance. A struct (not an
/// ever-widening tuple) so adding the next identity axis is a field, not
/// a re-thread of every call site.
///
/// `pub(crate)` because the wall projection
/// (`crate::ipc::positron_wall_source`) resolves a pinned post's AUTHOR
/// the same way this projection resolves a message's SENDER — one
/// identity-resolution decision, one place ([[compression]]).
pub(crate) struct ResolvedSender {
    pub(crate) name: String,
    pub(crate) kind: SenderKind,
    pub(crate) integrations: BTreeMap<String, String>,
    pub(crate) provenance: Provenance,
}

/// Resolve an identity (a message sender OR a wall-post author) from the
/// room roster. A member present in the roster (its `presence:updated`
/// card folded in) resolves richly — name, neutral kind, opaque badges,
/// accountability provenance. A member whose card has not folded in yet
/// resolves **provisionally**: a short peer-id label, neutral `Human`,
/// empty badges, unresolved provenance — a provisional projection pending
/// authoritative truth, never a fabricated identity
/// ([[fallbacks-are-illegal-fail-loud]]). Upgraded in place the instant
/// the card lands (see `reresolve_messages`, and the wall projector's
/// re-render on presence).
///
/// A free function over `&[RosterSlotView]` (not a method) so both the
/// chat and wall projections share the exact resolution — the compression
/// point of extracting it.
pub(crate) fn resolve_identity(roster: &[RosterSlotView], id: Uuid) -> ResolvedSender {
    match roster.iter().find(|s| s.member_id == id) {
        Some(slot) => ResolvedSender {
            name: slot.display_name.clone(),
            kind: slot.kind,
            integrations: slot.integrations.clone(),
            provenance: slot.provenance.clone(),
        },
        None => ResolvedSender {
            name: provisional_sender_name(id),
            kind: SenderKind::Human,
            integrations: BTreeMap::new(),
            provenance: Provenance::unresolved(),
        },
    }
}

/// Project a presence snapshot's roster into the renderer-shaped
/// [`RosterSlotView`] rows. Shared by the chat projection (which stores
/// them on the `ChatViewState`) and the wall projection (which holds them
/// only as its author-resolution lookup table) — one wire-shape
/// conversion, one place ([[compression]]).
pub(crate) fn roster_slots_from(update: &AircPresenceUpdate) -> Vec<RosterSlotView> {
    update
        .roster
        .iter()
        .map(|s| RosterSlotView {
            member_id: s.member_id,
            display_name: s.display_name.clone(),
            kind: s.kind,
            integrations: s.integrations.clone(),
            provenance: s.provenance.clone(),
            active: s.active,
        })
        .collect()
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
    roster: Vec<RosterSlotView>,
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

    /// Switch the accumulator to `room_id` if it differs from the current
    /// room, clearing the prior room's ring + roster + stale name. The
    /// single-cache-per-kind substrate holds one room's view at a time.
    /// The room *name* is an identity-card fact carried by presence, not
    /// by messages, so this does NOT set it — a message that focuses a
    /// fresh room leaves `room_name` empty until presence resolves it.
    fn switch_room(&mut self, room_id: Uuid) {
        if self.room_id != Some(room_id) {
            self.room_id = Some(room_id);
            self.room_name.clear();
            self.messages.clear();
            self.roster.clear();
        }
    }

    /// Fold a posted message into the view and store the new snapshot.
    /// Idempotent on `message_id`: a redelivered event (the bus is
    /// best-effort) does not double-append. Identity is resolved from the
    /// roster, never carried on the message (see `AircChatPosted`).
    fn apply_message(&mut self, msg: AircChatPosted) {
        self.switch_room(msg.room_id);
        if self.messages.iter().any(|m| m.id == msg.message_id) {
            return;
        }
        let resolved = resolve_identity(&self.roster, msg.sender_id);
        self.messages.push_back(ChatMessageView {
            id: msg.message_id,
            room_id: msg.room_id,
            sender_id: msg.sender_id,
            sender_name: resolved.name,
            sender_kind: resolved.kind,
            integrations: resolved.integrations,
            provenance: resolved.provenance,
            content: msg.content,
            timestamp: msg.timestamp,
        });
        while self.messages.len() > MAX_MESSAGES_PER_SNAPSHOT {
            self.messages.pop_front();
        }
        self.store();
    }

    /// Replace the focused room's roster from a presence snapshot and
    /// store the new view. Presence is the room-name authority, so this
    /// (unlike a message) adopts the resolved name. It also **upgrades**
    /// any message whose sender was provisional (posted before its card
    /// arrived) now that the card is known.
    fn apply_presence(&mut self, update: AircPresenceUpdate) {
        self.switch_room(update.room_id);
        self.roster = roster_slots_from(&update);
        self.room_name = update.room_name;
        self.reresolve_messages();
        self.store();
    }

    /// Re-resolve each stored message's identity from the current roster.
    /// Only **upgrades** — a message whose sender is present in the roster
    /// adopts the card; a sender who has since left keeps its already-
    /// resolved identity (presence is authoritative for who is here NOW,
    /// not for who authored a past message). This is what makes the
    /// provisional-until-card projection settle to the truth in place.
    fn reresolve_messages(&mut self) {
        let roster = &self.roster;
        for msg in self.messages.iter_mut() {
            if let Some(slot) = roster.iter().find(|s| s.member_id == msg.sender_id) {
                msg.sender_name = slot.display_name.clone();
                msg.sender_kind = slot.kind;
                msg.integrations = slot.integrations.clone();
                msg.provenance = slot.provenance.clone();
            }
        }
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
            .store(self.builder.session(view));
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

    /// A thin `chat:posted` payload — core message facts only, sender
    /// hardcoded to `0xc`. Mirrors the emitter contract (identity is NOT
    /// on the message; it resolves from the roster).
    fn posted(room: Uuid, msg: Uuid, text: &str) -> serde_json::Value {
        posted_from(room, msg, Uuid::from_u128(0xc), text)
    }

    /// A thin `chat:posted` payload with an explicit `sender_id` — used to
    /// prove roster-based identity resolution.
    fn posted_from(room: Uuid, msg: Uuid, sender: Uuid, text: &str) -> serde_json::Value {
        json!({
            "messageId": msg,
            "roomId": room,
            "senderId": sender,
            "content": text,
            "timestamp": 1_700_000_000_000u64,
        })
    }

    /// A `presence:updated` payload for `room` with a single member
    /// carrying `kind` + `integrations`.
    fn presence_one(
        room: Uuid,
        member: Uuid,
        name: &str,
        kind: &str,
        integrations: serde_json::Value,
    ) -> serde_json::Value {
        json!({
            "roomId": room,
            "roomName": "general",
            "roster": [
                {
                    "memberId": member,
                    "displayName": name,
                    "kind": { "kind": kind },
                    "integrations": integrations,
                    "active": true,
                }
            ],
        })
    }

    fn current_chat(substrate: &Substrate) -> ChatViewState {
        let env = substrate
            .cache()
            .get(ChatViewState::KIND)
            .expect("a chat envelope must be stored");
        serde_json::from_value(env.payload.clone()).expect("payload is a ChatViewState")
    }

    #[test]
    fn message_event_projects_into_the_substrate() {
        // what this catches: regression where a chat:posted event does
        // not reach the substrate as a ChatViewState — the whole point
        // of the airc source wiring. Drives the pure fold (no bus) and
        // asserts the cache holds the projected message. With no presence
        // yet, the sender renders provisionally (short peer-id label,
        // neutral Human) and the room_name is unresolved.
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
        assert_eq!(view.room_name, "", "room_name is unresolved until presence");
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].id, m);
        assert_eq!(view.messages[0].content, "hi");
        assert_eq!(view.messages[0].sender_kind, SenderKind::Human);
        assert!(
            view.messages[0].sender_name.starts_with("peer-"),
            "provisional sender label, got {}",
            view.messages[0].sender_name
        );
        assert!(view.messages[0].integrations.is_empty());
    }

    #[test]
    fn message_before_presence_is_provisional_then_upgrades() {
        // what this catches: the provisional-until-card contract — a
        // message posted before its sender's identity card arrives
        // renders provisionally, then UPGRADES in place (name + kind +
        // badges) the instant presence folds the card. A regression that
        // dropped the re-resolution would leave the sender stuck as
        // "peer-xxxx"/Human forever. [[fallbacks-are-illegal-fail-loud]].
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted_from(room, Uuid::from_u128(0xb), sender, "hi")).unwrap()
        {
            p.apply_message(m);
        }
        // Provisional before the card.
        assert_eq!(current_chat(&substrate).messages[0].sender_kind, SenderKind::Human);
        // Card arrives via presence: Agent named Helper carrying a badge.
        let presence = presence_one(
            room,
            sender,
            "Helper",
            "agent",
            json!({ "continuum.persona_id": "helper-1" }),
        );
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let msg = &current_chat(&substrate).messages[0];
        assert_eq!(msg.sender_name, "Helper");
        assert_eq!(msg.sender_kind, SenderKind::Agent);
        assert_eq!(
            msg.integrations.get("continuum.persona_id").map(String::as_str),
            Some("helper-1"),
            "opaque badge resolved from the card"
        );
    }

    #[test]
    fn message_after_presence_resolves_identity_from_the_roster() {
        // what this catches: regression where sender identity is not
        // resolved from the roster (the whole point of the thin emitter).
        // Presence first establishes the card, then a message from that
        // sender must render richly with no provisional phase.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        let presence = presence_one(room, sender, "Helper", "agent", json!({}));
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted_from(room, Uuid::from_u128(0xb), sender, "hi")).unwrap()
        {
            p.apply_message(m);
        }
        let msg = &current_chat(&substrate).messages[0];
        assert_eq!(msg.sender_name, "Helper");
        assert_eq!(msg.sender_kind, SenderKind::Agent);
    }

    #[test]
    fn presence_event_projects_the_roster() {
        // what this catches: regression where presence:updated does not
        // fold into the roster — the second airc stream (outlier B,
        // maximally different from the message stream) proving the
        // accumulator holds two independent airc shapes on one kind, and
        // carries each member's neutral kind + opaque badges.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let payload = json!({
            "roomId": room,
            "roomName": "general",
            "roster": [
                {
                    "memberId": Uuid::from_u128(0xd),
                    "displayName": "Helper",
                    "kind": { "kind": "agent" },
                    "integrations": { "continuum.persona_id": "helper-1" },
                    "active": true,
                },
                {
                    "memberId": Uuid::from_u128(0xe),
                    "displayName": "Joel",
                    "kind": { "kind": "human" },
                    "integrations": {},
                    "active": false,
                },
            ],
        });
        match classify(PRESENCE_UPDATED, &payload).unwrap() {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.roster.len(), 2);
        assert_eq!(view.roster[0].display_name, "Helper");
        assert_eq!(view.roster[0].kind, SenderKind::Agent);
        assert_eq!(
            view.roster[0].integrations.get("continuum.persona_id").map(String::as_str),
            Some("helper-1")
        );
        assert!(view.roster[0].active);
        assert_eq!(view.roster[1].kind, SenderKind::Human);
        assert!(!view.roster[1].active);
    }

    #[test]
    fn provenance_flows_from_presence_through_roster_to_message() {
        // what this catches: the accountability slot (task #38's
        // substrate seam) must ride the identity card end to end — a
        // presence slot's `provenance.runtime` lands on the roster entry
        // AND upgrades a provisional message's provenance in place. A
        // regression that dropped provenance from `apply_presence` /
        // `reresolve_messages` would leave every rendered row
        // unattributable — the exact leak the zero-trust flow doctrine
        // exists to close. A message with no card yet is honestly
        // unresolved (empty runtime), never a fabricated origin.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xd);
        // Message before the card → provenance honestly unresolved.
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted_from(room, Uuid::from_u128(0xb), sender, "hi")).unwrap()
        {
            p.apply_message(m);
        }
        assert_eq!(
            current_chat(&substrate).messages[0].provenance.runtime, "",
            "unresolved provenance is empty, not fabricated"
        );
        // Presence folds the card carrying a runtime origin.
        let presence = json!({
            "roomId": room,
            "roomName": "general",
            "roster": [
                {
                    "memberId": sender,
                    "displayName": "Helper",
                    "kind": { "kind": "agent" },
                    "integrations": {},
                    "provenance": { "runtime": "claude" },
                    "active": true,
                }
            ],
        });
        if let ProjectionInput::Presence(u) = classify(PRESENCE_UPDATED, &presence).unwrap() {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(
            view.roster[0].provenance.runtime, "claude",
            "roster carries the member's origin"
        );
        assert_eq!(
            view.messages[0].provenance.runtime, "claude",
            "the provisional message's provenance upgraded from the card"
        );
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
        let presence = presence_one(room, Uuid::from_u128(0xd), "Helper", "agent", json!({}));
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
        // chat:posted missing the core message facts (ids / timestamp) →
        // not a renderable message.
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
            .get(ChatViewState::KIND)
            .unwrap()
            .revision;
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x2), "two")).unwrap()
        {
            p.apply_message(m);
        }
        let r2 = substrate
            .cache()
            .get(ChatViewState::KIND)
            .unwrap()
            .revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }
}
