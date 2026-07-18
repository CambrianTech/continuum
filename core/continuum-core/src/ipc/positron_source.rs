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
//! Three bus streams fold onto the single existing `ChatViewState::KIND`, and
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
//! - **`persona:vitals`** — a persona radiated its live cognition readouts.
//!   Deserialized into [`PersonaVitalsUpdate`] and folded into that member's
//!   [`RosterSlotView::vitals`] by id — a thin overlay kept in its own map so
//!   it survives a roster-replacing presence snapshot (personas emit, this
//!   projection folds; the presence emitter stays persona-agnostic).
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

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use airc_lib::{AgentAvailabilityState, RoomMember};
use continuum_positron::{
    ChatMessageView, ChatViewState, Provenance, RosterSlotView, SenderKind, StateBuilder, Substrate,
};

use crate::experience::{Experience, ExperienceSource, Member, RecipeExperienceSource, Standing};
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
///
/// The roster rows ARE [`RosterSlotView`] — the neutral positron slot — not
/// a hand-copied twin. That copy (`AircPresenceSlot`) used to exist "so both
/// sides agree by construction"; but `RosterSlotView` already derives
/// `Serialize`/`Deserialize`, so the neutral view IS the wire shape and the
/// twin was pure duplication (the exact compression violation this
/// convergence removes — #8/#13). One slot type now flows from the airc
/// projection ([`roster_slot_from_member`]) all the way to the widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AircPresenceUpdate {
    pub(crate) room_id: Uuid,
    pub(crate) room_name: String,
    pub(crate) roster: Vec<RosterSlotView>,
}

/// Event name a persona radiates its live vitals under.
pub(crate) const PERSONA_VITALS: &str = "persona:vitals";

/// Typed `persona:vitals` payload — one persona's live cognition readouts.
///
/// Design B of the roster-vitals build: a persona **radiates** its own
/// `PersonaState` (energy/attention/compute, normalized `0..=100`) on the bus,
/// and this projection **folds** it into that member's [`RosterSlotView::vitals`]
/// by id — the same emit/subscribe organism messages + presence already use, so
/// the persona-agnostic presence emitter never learns about personas. `member_id`
/// is the persona's airc peer id, matching the roster slot it enriches. Same
/// `pub(crate)` + `Serialize` "agree by construction" discipline as
/// [`AircPresenceUpdate`]: the emitter builds THIS struct, never a hand JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersonaVitalsUpdate {
    pub(crate) member_id: Uuid,
    pub(crate) vitals: BTreeMap<String, u8>,
}

/// airc's `AgentAvailabilityState` → its stable neutral wire label — airc's
/// OWN `snake_case` serde repr (`"ready"` / `"busy"` / `"away"`), single-
/// sourced by mirroring that vocabulary here rather than `{:?}` (Debug's
/// CamelCase is not a contract and would drift). The exhaustive match is the
/// fail-loud seam: if airc ever adds an availability state the compiler
/// forces a deliberate decision here instead of silently coercing it.
///
/// The label is carried **verbatim** into the neutral
/// [`RosterSlotView::availability`]; positron never interprets it (same
/// transported-not-interpreted discipline as `provenance.runtime`).
fn availability_label(state: AgentAvailabilityState) -> &'static str {
    match state {
        AgentAvailabilityState::Ready => "ready",
        AgentAvailabilityState::Busy => "busy",
        AgentAvailabilityState::Away => "away",
    }
}

/// THE `RoomMember` → neutral [`RosterSlotView`] projection — one place both
/// rails build a roster slot from an airc member, so the WS widget and the
/// persona's grounding can never drop different fields (the divergence
/// #8/#13 removes: Rail A used to keep `availability`/`last_seen_ms` while
/// Rail B silently dropped them). Pure and total — no self-exclusion, no
/// budget, no ordering: those are per-consumer policies the caller applies
/// to the projected slots, not part of the shared projection.
///
/// `pub(crate)` so `crate::ipc::positron_presence` (the WS emitter) and
/// `crate::persona::room_roster_source` (the persona grounding source) call
/// the identical function.
pub(crate) fn roster_slot_from_member(member: &RoomMember) -> RosterSlotView {
    // Coarse styling hint from the free-form runtime class; the full string
    // is preserved verbatim in `provenance` (never string-match on runtime
    // to pick behavior — that is task #70's smell).
    let kind = SenderKind::from_runtime(&member.runtime);
    // airc resolved the name; a present-but-unnamed peer gets the SAME
    // provisional short-peer label the consumer uses when a card has not
    // folded in — one provisional-label decision, never a silently-invisible
    // citizen and never a second fallback form.
    let display_name = member
        .display_name
        .clone()
        .unwrap_or_else(|| provisional_sender_name(member.peer_id.as_uuid()));
    RosterSlotView {
        member_id: member.peer_id.as_uuid(),
        display_name,
        kind,
        // airc's `RoomMember` carries no cross-system badge map (the richer
        // `room_roster_cards` path would fill this); empty is the honest "no
        // badges known", not a fabricated one.
        integrations: BTreeMap::new(),
        // The accountability truth airc surfaces today, carried verbatim.
        // Trust tier + verification join here later (task #38), no wire break.
        provenance: Provenance {
            runtime: member.runtime.clone(),
        },
        // airc excludes `Leaving` peers from the roster, so every member it
        // returns is present → active.
        active: true,
        // Neutral presence facts carried straight through — the fields Rail B
        // used to drop. `availability` is airc's stable label (or `None` when
        // unreported); `last_seen_ms` is the raw recency signal.
        availability: member
            .availability
            .map(availability_label)
            .map(str::to_owned),
        last_seen_ms: member.last_seen_ms,
        // airc's `RoomMember` carries no vitals — a continuum persona's live
        // `PersonaState` (energy/attention/compute) arrives on its OWN
        // `persona:vitals` event and is folded into the slot by id at `store`
        // time (see [`PersonaVitalsUpdate`] / [`ChatProjection::apply_vitals`]),
        // never through this presence path. Empty here = no vitals reported,
        // never fabricated bars ([[fallbacks-are-illegal-fail-loud]]).
        vitals: BTreeMap::new(),
    }
}

/// Test-only: serialize a `presence:updated` bus payload from the REAL typed
/// [`AircPresenceUpdate`] (roster = neutral [`RosterSlotView`]s). Tests build
/// the payload from the struct, NEVER a hand-authored JSON literal — so a
/// test's wire can never drift from the type's field names (the "agree by
/// construction" contract, enforced instead of hoped for). Shared by the chat
/// / wall / kanban projector test mods: one wire-shape source, even in tests.
#[cfg(test)]
pub(crate) fn test_presence_payload(room: Uuid, roster: Vec<RosterSlotView>) -> serde_json::Value {
    serde_json::to_value(AircPresenceUpdate {
        room_id: room,
        room_name: "general".to_string(),
        roster,
    })
    .expect("AircPresenceUpdate serializes")
}

/// Test-only: one neutral roster slot — present + active, empty badges,
/// unresolved provenance, no availability/recency. Tests override any field
/// via struct-update (`RosterSlotView { active: false, ..test_roster_slot(..) }`).
#[cfg(test)]
pub(crate) fn test_roster_slot(member: Uuid, name: &str, kind: SenderKind) -> RosterSlotView {
    RosterSlotView {
        member_id: member,
        display_name: name.to_string(),
        kind,
        integrations: BTreeMap::new(),
        provenance: Provenance::unresolved(),
        active: true,
        availability: None,
        last_seen_ms: 0,
        vitals: BTreeMap::new(),
    }
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

// `roster_slots_from` is gone: `AircPresenceUpdate.roster` IS already
// `Vec<RosterSlotView>`, so the old slot→slot copy was an identity map on a
// duplicated type. Consumers that need the roster (chat / wall / kanban) take
// `update.roster` directly — one slot type, no conversion.

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
    /// Latest live vitals per member id, folded from `persona:vitals` events
    /// (design B). Kept SEPARATE from the presence roster — presence replaces
    /// the roster wholesale on every update, so vitals live here and are merged
    /// into each slot at `store` time, surviving roster churn. A member with no
    /// entry simply has empty vitals (no meter), never a fabricated one.
    vitals: HashMap<Uuid, BTreeMap<String, u8>>,
    /// Resolves this room's activity **purpose** (the `Content` dispatch key) — #6.
    /// The projection no longer hardcodes `"chat"`; it routes through this seam so a
    /// foundry / scada / academy room reports its own recipe-defined purpose with NO
    /// projection change once the real (recipe-backed) resolver is injected.
    purpose_source: crate::ipc::room_purpose::SharedRoomPurpose,
    /// Recipe-backed source for this room's [`Experience`] manifest (the Join
    /// Contract), keyed by the SAME purpose the chat view uses. Published alongside
    /// chat so a renderer/agent sees the full room — regions, affordances, and the
    /// live membership — not just the message stream.
    /// `[[join-contract-experience-is-a-latent-space]]`.
    experience_source: RecipeExperienceSource,
    /// Own monotonic revisions well for the `"experience"` kind — the projection is
    /// its sole writer, exactly as `builder` is for `"chat"`.
    experience_builder: StateBuilder,
    /// Last-published manifest, for emit-on-change: the Experience only shifts when
    /// membership/purpose change (presence), NOT on every message, so re-publishing
    /// an identical manifest per message is wasted work + churned revisions
    /// (`[[optimization-is-always-first]]`, `[[never-thrash-sticky-hysteresis-on-every-lane]]`).
    /// `RefCell` because `store` takes `&self`.
    last_experience: std::cell::RefCell<Option<Experience>>,
}

impl ChatProjection {
    fn new(substrate: Substrate) -> Self {
        // ONE shared purpose resolver feeds BOTH the chat view's `purpose` field and
        // the Experience manifest's recipe lookup. Default (every room → "chat") until
        // the recipe-backed source lands; injecting a real one is a one-line change
        // here, no call-site churn.
        let purpose_source = crate::ipc::room_purpose::default_source();
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
            vitals: HashMap::new(),
            experience_source: RecipeExperienceSource::builtins(purpose_source.clone()),
            experience_builder: StateBuilder::standalone(),
            last_experience: std::cell::RefCell::new(None),
            purpose_source,
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
            self.vitals.clear();
        }
    }

    /// Fold a persona's radiated vitals into the per-member map and re-store.
    /// The values are merged into the roster at `store` time (they outlive any
    /// single presence snapshot), so a persona breathing its energy updates the
    /// widget without touching the neutral presence path. A member that left the
    /// roster keeps no bearing here — `store` only merges vitals for members the
    /// current roster still lists.
    fn apply_vitals(&mut self, update: PersonaVitalsUpdate) {
        self.vitals.insert(update.member_id, update.vitals);
        self.store();
    }

    /// Fold a posted message into the view and store the new snapshot.
    /// Idempotent on `message_id` AND on `(sender, content)`: a redelivered
    /// event (the bus is best-effort) is caught by the id check, but a
    /// **multi-hop replay** re-emits the same logical message with a FRESH
    /// `message_id` (#16) — which the id check alone cannot collapse, and which
    /// showed as doubled turns in every client. Content-identity is the same
    /// stance the cognition admission dedup already takes (`content_hash`,
    /// `cognition_io.rs`): one `(sender, content)` is one message, however many
    /// ids the bus minted for it. Identity is resolved from the roster, never
    /// carried on the message (see `AircChatPosted`).
    fn apply_message(&mut self, msg: AircChatPosted) {
        self.switch_room(msg.room_id);
        let is_duplicate = self.messages.iter().any(|m| {
            m.id == msg.message_id || (m.sender_id == msg.sender_id && m.content == msg.content)
        });
        if is_duplicate {
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
        self.room_name = update.room_name;
        // The update's roster IS the neutral slot type — move it in directly,
        // no per-field copy through a twin struct.
        self.roster = update.roster;
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
        // Merge each member's latest radiated vitals into its neutral slot
        // (design B fold): presence owns the roster shape; vitals are the live
        // cognition overlay keyed by id. A member with no vitals entry keeps the
        // slot's empty map — no fabricated bars.
        let roster: Vec<RosterSlotView> = self
            .roster
            .iter()
            .map(|slot| match self.vitals.get(&slot.member_id) {
                Some(v) => RosterSlotView {
                    vitals: v.clone(),
                    ..slot.clone()
                },
                None => slot.clone(),
            })
            .collect();

        // Publish the room's Experience manifest (the Join Contract) ALONGSIDE the
        // chat view, so a renderer/agent sees the whole room — regions, affordances,
        // and the live membership — not just the message stream. Membership is
        // projected from the SAME roster (kind-agnostic: human/persona/agent are all
        // Members). `Experience` is renderer-agnostic (no positron-core dep), so it
        // rides `session_raw` under its own `KIND`. No recipe for the room's purpose
        // → nothing published (fail-quiet on this OPTIONAL surface; chat still ships).
        if let Some(exp) = self.build_experience(room_id, &roster) {
            // Emit-on-change: skip re-publishing an identical manifest (messages don't
            // move membership/purpose). Only presence-driven changes reach the wire.
            let unchanged = self.last_experience.borrow().as_ref() == Some(&exp);
            if !unchanged {
                let payload = serde_json::to_value(&exp)
                    .expect("Experience must serialize — substrate bug, not a runtime error");
                self.substrate
                    .store(self.experience_builder.session_raw(Experience::KIND, payload));
                *self.last_experience.borrow_mut() = Some(exp);
            }
        }

        let view = ChatViewState {
            room_id,
            room_name: self.room_name.clone(),
            // The room's activity purpose — resolved through the `RoomPurposeSource`
            // seam (#6), NOT hardcoded. Today the default answers "chat" for every
            // room; when the recipe-backed resolver is injected, a foundry / scada /
            // academy room reports its own purpose and the client's Content primitive
            // dispatches on it (ACTIVITY-ROOM-PATTERNS.md) with no change here.
            purpose: self.purpose_source.purpose_for(room_id),
            messages: self.messages.iter().cloned().collect(),
            roster,
        };
        self.substrate.store(self.builder.session(view));
    }

    /// Assemble this room's [`Experience`] manifest: recipe (by the room's purpose)
    /// → static manifest, live roster → `membership`. Pure over `(room_id, roster)`
    /// so it's unit-testable without a live bus. `None` when no recipe matches the
    /// room's purpose (the manifest is an OPTIONAL surface; chat still ships).
    ///
    /// Membership is kind-agnostic — every present peer becomes a plain
    /// [`Standing::Member`]; structural-role overlay (examinee/owner) is a higher
    /// concern supplied by run/room context, not this presence projection.
    fn build_experience(&self, room_id: Uuid, roster: &[RosterSlotView]) -> Option<Experience> {
        let membership: Vec<Member> = roster
            .iter()
            .map(|slot| Member {
                peer_id: slot.member_id.to_string(),
                standing: Standing::Member,
            })
            .collect();
        self.experience_source
            .experience_for(room_id)
            .map(|exp| exp.with_membership(membership))
    }
}

/// Classify a bus event into a typed projection input, or `None` when the
/// event is not a chat/presence event this projection renders. Pure — no
/// substrate side effect — so it's unit-testable without a live bus.
enum ProjectionInput {
    Message(AircChatPosted),
    Presence(AircPresenceUpdate),
    Vitals(PersonaVitalsUpdate),
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
        PERSONA_VITALS => serde_json::from_value::<PersonaVitalsUpdate>(body.clone())
            .ok()
            .map(ProjectionInput::Vitals),
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
    // Demand the current roster now (#118): the presence emitter dedups and
    // may have already fired for a stable roster before this projection
    // subscribed. Without the cue a late/restarted chat projection holds a
    // roster-empty view until presence next changes. `rx` is subscribed
    // above, so the emitter's re-publish lands in our buffer.
    crate::ipc::positron_presence::request_presence_resync(&bus);
    rt.spawn(async move {
        let mut projection = ChatProjection::new(substrate);
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(input) = classify(&event.name, &event.payload) {
                        match input {
                            ProjectionInput::Message(m) => projection.apply_message(m),
                            ProjectionInput::Presence(p) => projection.apply_presence(p),
                            ProjectionInput::Vitals(v) => projection.apply_vitals(v),
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

    /// A `presence:updated` payload for `room` with a single member carrying
    /// `kind` + `integrations` — serialized from the real typed slot via the
    /// shared [`test_presence_payload`], never a hand-authored JSON literal.
    fn presence_one(
        room: Uuid,
        member: Uuid,
        name: &str,
        kind: &str,
        integrations: serde_json::Value,
    ) -> serde_json::Value {
        let kind: SenderKind = serde_json::from_value(json!({ "kind": kind })).unwrap();
        let integrations: BTreeMap<String, String> = serde_json::from_value(integrations).unwrap();
        test_presence_payload(
            room,
            vec![RosterSlotView {
                integrations,
                ..test_roster_slot(member, name, kind)
            }],
        )
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
    fn persona_vitals_fold_into_the_member_slot_and_survive_presence() {
        // what this catches: design B's fold — a persona:vitals event must merge
        // its readouts into that member's roster slot BY ID, and (because vitals
        // live in their own map, not the presence roster) a later presence
        // snapshot that REPLACES the roster must not drop them. A member with no
        // vitals event keeps an empty map — no fabricated bars.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let asha = Uuid::from_u128(0xb);

        // Roster arrives first — no vitals yet.
        match classify(
            PRESENCE_UPDATED,
            &presence_one(room, asha, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        assert!(
            current_chat(&substrate).roster[0].vitals.is_empty(),
            "no vitals before any radiate"
        );

        // Asha radiates her vitals.
        let radiated = serde_json::to_value(PersonaVitalsUpdate {
            member_id: asha,
            vitals: BTreeMap::from([
                ("energy".to_string(), 80u8),
                ("attention".to_string(), 90u8),
            ]),
        })
        .unwrap();
        match classify(PERSONA_VITALS, &radiated).unwrap() {
            ProjectionInput::Vitals(v) => p.apply_vitals(v),
            _ => panic!("persona:vitals must classify as Vitals"),
        }
        let slot = current_chat(&substrate).roster.remove(0);
        assert_eq!(slot.vitals.get("energy"), Some(&80));
        assert_eq!(slot.vitals.get("attention"), Some(&90));

        // A fresh presence snapshot replaces the roster wholesale — vitals stay.
        match classify(
            PRESENCE_UPDATED,
            &presence_one(room, asha, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        assert_eq!(
            current_chat(&substrate).roster[0].vitals.get("energy"),
            Some(&80),
            "vitals survive a roster refresh (they are keyed by id, not carried on presence)"
        );
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
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
        {
            p.apply_message(m);
        }
        // Provisional before the card.
        assert_eq!(
            current_chat(&substrate).messages[0].sender_kind,
            SenderKind::Human
        );
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
            msg.integrations
                .get("continuum.persona_id")
                .map(String::as_str),
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
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
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
        let mut helper_badges = BTreeMap::new();
        helper_badges.insert("continuum.persona_id".to_string(), "helper-1".to_string());
        let payload = test_presence_payload(
            room,
            vec![
                RosterSlotView {
                    integrations: helper_badges,
                    ..test_roster_slot(Uuid::from_u128(0xd), "Helper", SenderKind::Agent)
                },
                RosterSlotView {
                    active: false,
                    ..test_roster_slot(Uuid::from_u128(0xe), "Joel", SenderKind::Human)
                },
            ],
        );
        match classify(PRESENCE_UPDATED, &payload).unwrap() {
            ProjectionInput::Presence(u) => p.apply_presence(u),
            _ => panic!("presence:updated must classify as Presence"),
        }
        let view = current_chat(&substrate);
        assert_eq!(view.roster.len(), 2);
        assert_eq!(view.roster[0].display_name, "Helper");
        assert_eq!(view.roster[0].kind, SenderKind::Agent);
        assert_eq!(
            view.roster[0]
                .integrations
                .get("continuum.persona_id")
                .map(String::as_str),
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
        if let ProjectionInput::Message(m) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0xb), sender, "hi"),
        )
        .unwrap()
        {
            p.apply_message(m);
        }
        assert_eq!(
            current_chat(&substrate).messages[0].provenance.runtime,
            "",
            "unresolved provenance is empty, not fabricated"
        );
        // Presence folds the card carrying a runtime origin.
        let presence = test_presence_payload(
            room,
            vec![RosterSlotView {
                provenance: Provenance {
                    runtime: "claude".to_string(),
                },
                ..test_roster_slot(sender, "Helper", SenderKind::Agent)
            }],
        );
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
    fn multi_hop_replay_with_fresh_id_does_not_double_render() {
        // what this catches: #16 — a multi-hop replay re-emits the SAME logical
        // message with a FRESH message_id, which dedup-by-id alone cannot collapse.
        // It showed as doubled turns in every client (web/terminal/RAG). Content
        // identity (sender + content) collapses it, matching the cognition
        // content_hash admission dedup; a genuinely new message still appends.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room = Uuid::from_u128(0xa);
        let sender = Uuid::from_u128(0xc);
        for id in [0x10u128, 0x11, 0x12] {
            if let ProjectionInput::Message(msg) = classify(
                CHAT_POSTED,
                &posted_from(room, Uuid::from_u128(id), sender, "hello team"),
            )
            .unwrap()
            {
                p.apply_message(msg);
            }
        }
        assert_eq!(
            current_chat(&substrate).messages.len(),
            1,
            "replay must collapse"
        );
        if let ProjectionInput::Message(msg) = classify(
            CHAT_POSTED,
            &posted_from(room, Uuid::from_u128(0x13), sender, "a new thought"),
        )
        .unwrap()
        {
            p.apply_message(msg);
        }
        assert_eq!(
            current_chat(&substrate).messages.len(),
            2,
            "new content still appends"
        );
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
    fn switching_rooms_clears_vitals() {
        // what this catches: switch_room must clear the per-member vitals map,
        // or a member with the SAME id in a new room would inherit the prior
        // room's meters — a cross-room leak. The vitals twin of
        // switching_rooms_resets_the_accumulator.
        let substrate = Substrate::new();
        let mut p = ChatProjection::new(substrate.clone());
        let room_a = Uuid::from_u128(0xa);
        let room_b = Uuid::from_u128(0xf);
        let m = Uuid::from_u128(0xb);

        // Room A: member m present + radiating vitals.
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_a, m, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let radiated = serde_json::to_value(PersonaVitalsUpdate {
            member_id: m,
            vitals: BTreeMap::from([("energy".to_string(), 70u8)]),
        })
        .unwrap();
        if let ProjectionInput::Vitals(v) = classify(PERSONA_VITALS, &radiated).unwrap() {
            p.apply_vitals(v);
        }
        assert_eq!(
            current_chat(&substrate).roster[0].vitals.get("energy"),
            Some(&70)
        );

        // Same member id focuses room B → the vitals map is cleared, no leak.
        if let ProjectionInput::Presence(u) = classify(
            PRESENCE_UPDATED,
            &presence_one(room_b, m, "Asha", "agent", json!({})),
        )
        .unwrap()
        {
            p.apply_presence(u);
        }
        let view = current_chat(&substrate);
        assert_eq!(view.room_id, room_b);
        assert!(
            view.roster[0].vitals.is_empty(),
            "vitals leaked from room A into room B"
        );
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
        let r1 = substrate.cache().get(ChatViewState::KIND).unwrap().revision;
        if let ProjectionInput::Message(m) =
            classify(CHAT_POSTED, &posted(room, Uuid::from_u128(0x2), "two")).unwrap()
        {
            p.apply_message(m);
        }
        let r2 = substrate.cache().get(ChatViewState::KIND).unwrap().revision;
        assert!(r2 > r1, "revision must advance: {r1:?} -> {r2:?}");
    }
}
