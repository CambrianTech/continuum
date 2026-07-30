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
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/ChatMessageView.ts"
)]
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
    /// Verifiable provenance of the author — the accountability half of
    /// identity (see [`Provenance`]). Resolved from the roster alongside
    /// `sender_name` / `sender_kind`, so a message row is attributable
    /// on its face. Woven in from day one per
    /// `[[positron-identity-security-first-class]]`; grows to carry trust
    /// tier + verification with no wire break.
    pub provenance: Provenance,
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/SenderKind.ts"
)]
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

impl SenderKind {
    /// Project an airc presence `runtime` class onto the neutral author
    /// kind — a **coarse styling hint**, not the accountability truth.
    ///
    /// airc's `runtime` is a free-form self-reported client class
    /// (`"claude"`, `"codex"`, `"interactive"`, `"automation"`, …); it
    /// does NOT cleanly encode human-vs-AI. So this mapping is
    /// deliberately minimal: airc's documented human-driven runtime
    /// (`"interactive"`) → [`SenderKind::Human`]; every other running
    /// actor (an AI of any framework, an automation) → [`SenderKind::Agent`].
    /// A roster member is a *present citizen*, so this never yields
    /// `System` (system events are message-only, never roster entries).
    ///
    /// This is a projection between two **neutral** vocabularies (a
    /// runtime class → an author kind), NOT identity-sniffing a model
    /// name to pick behavior (the `model.starts_with("qwen")` smell this
    /// codebase kills). The precise accountability signal is carried
    /// verbatim in [`Provenance::runtime`]; `kind` only drives the
    /// avatar/styling discriminant. The default-to-`Agent` is not a
    /// bug-hiding fallback — an unclassified present actor genuinely is
    /// "some running agent" until its card refines it via `provenance`.
    pub fn from_runtime(runtime: &str) -> Self {
        match runtime {
            "interactive" => SenderKind::Human,
            _ => SenderKind::Agent,
        }
    }
}

/// Verifiable provenance of the citizen behind a roster slot or message
/// — the **accountability** half of identity, distinct from the
/// *display* half (name / kind / badges).
///
/// Woven in from day one per `[[positron-identity-security-first-class]]`:
/// the slot costs nothing now and is ruinous to retrofit. Every unit of
/// rendered data (a roster member, a message row) carries "who/what
/// produced this, verifiably" so a human or a security persona can
/// attribute it — the substrate-side seam the zero-trust flow doctrine
/// (`docs/architecture/ZERO-TRUST-IDENTITY-AND-FLOW.md`) checks on the
/// way out.
///
/// **What it carries TODAY:** the one axis airc authoritatively surfaces
/// through presence — the peer's self-reported `runtime` origin. **What
/// joins it NEXT (no wire break — the whole point of reserving the typed
/// home now):** the per-peer trust tier + a cryptographic-verification
/// bit, once the airc peer-trust bridge (task #38) lands. Growable
/// struct-carrier, same discipline as the adapter capability surface.
///
/// Neutral like the rest of positron: `runtime` is transported verbatim,
/// not interpreted here — the app layer decides what a given runtime
/// means for trust, exactly as it does for `integrations`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/Provenance.ts"
)]
pub struct Provenance {
    /// The producer's self-reported runtime class, verbatim from airc
    /// presence (`"claude"` / `"codex"` / `"interactive"` / …). The
    /// neutral ORIGIN axis — WHO/WHAT produced this, before any trust
    /// judgment. Empty string = present but unresolved (the card has
    /// not folded a runtime in yet) — an honest "unknown", never a
    /// fabricated origin (`[[fallbacks-are-illegal-fail-loud]]`).
    pub runtime: String,
}

impl Provenance {
    /// A provisional provenance for a producer whose identity card has
    /// not resolved yet — empty runtime = honestly unknown, upgraded in
    /// place the instant presence lands (mirrors the provisional
    /// display-name path).
    pub fn unresolved() -> Self {
        Self {
            runtime: String::new(),
        }
    }
}

/// A member's **loadout** — the model backing this roster slot, as the
/// display facts a tile renders (`model · size · ctx`). Neutral and
/// OPTIONAL the same way `vitals` is: an AI member's substrate MAY attach
/// it; positron carries it, never interprets it. Every field is
/// `Option` — an honest absent when the substrate hasn't resolved it
/// yet, never a fabricated capability ([[fallbacks-are-illegal-fail-loud]]).
///
/// Distinct from `vitals` on purpose: vitals are fast-moving normalized
/// `0..=100` METERS (the live cognition pulse); a loadout is a slow-moving
/// CAPABILITY LABEL (what the member is running). A number-that-is-a-meter
/// and a number-that-is-a-token-count are different data with different
/// render rules, so they ride different fields, not one overloaded map.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/Loadout.ts"
)]
pub struct Loadout {
    /// The served model id, verbatim from the binding (`"devstral-24b"` /
    /// `"claude-opus-4-8"` / …). `None` = the member reports no bound model
    /// (unresolved) — an honest unknown, never a placeholder name.
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// Total parameter count of the served model, RAW (e.g. `24_000_000_000`
    /// for a 24B model) — the app formats the unit (`24B` / `671B` / `300M`).
    /// Sourced from the model registry row's GGUF-hydrated
    /// `parameter_count` (#74); NEVER sniffed from the model NAME
    /// ([[models-are-infinite-decide-on-capability-not-name]]). `None` when
    /// the row is unhydrated (a `0` count) — honest-absent, not `0B`. `u64`
    /// → `number` (param counts are < 2^53, so no bigint drift, #120).
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub params: Option<u64>,
    /// The EFFECTIVE served context window in tokens (`32768` → the app
    /// renders `32k`). The live binding's window (#50 single-sourced it),
    /// not the model row's nominal max, so a re-home to a smaller window
    /// reads true. `None`/`Some(0)` collapse to absent. `u32` → `number`.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub context_window: Option<u32>,
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/RosterSlotView.ts"
)]
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
    /// Verifiable provenance of this member — the accountability half of
    /// identity (see [`Provenance`]). The roster IS the source of truth
    /// the projection resolves message provenance from, so it lives here
    /// first and rides through to each `ChatMessageView`.
    pub provenance: Provenance,
    /// `true` if the member is currently attached and ready to receive
    /// turns. `false` for paged-out or spawning. The widget shows a
    /// presence indicator off this bit — single source of truth in the
    /// substrate.
    pub active: bool,
    /// Self-reported availability, transported **verbatim** from the
    /// producing substrate (`"ready"` / `"busy"` / `"away"` for the airc
    /// adopter) — the same neutral, not-interpreted discipline as
    /// [`Provenance::runtime`] and `integrations`. positron does NOT
    /// enumerate availability states (that would bake one framework's
    /// vocabulary into the generic package); it carries whatever the source
    /// reports and the app layer decides what it means for the roster line /
    /// UI. `None` = the member reported no availability — an honest unknown,
    /// never a fabricated state ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default)]
    #[ts(optional)]
    pub availability: Option<String>,
    /// Unix-ms of the member's most recent presence heartbeat — the recency
    /// signal a roster uses to sort or age out members. `#[serde(default)]`
    /// so a slot serialized before this field folds as `0` (honest "recency
    /// unknown"), never a dropped or fabricated timestamp. `u64` → `number`
    /// (not `bigint`) to match the rest of the substrate's ms timestamps
    /// (`ChatMessageView.timestamp`).
    #[serde(default)]
    #[ts(type = "number")]
    pub last_seen_ms: u64,
    /// Opaque per-member **vitals** — normalized `0..=100` percentage readouts a
    /// source MAY attach for the app to draw as live meters (e.g. energy, attention,
    /// compute). Transported, NOT interpreted — the same neutral discipline as
    /// `integrations` and `availability`: positron carries whatever keys the source
    /// reports; the app layer decides what each means and how to render it (a continuum
    /// persona surfaces its `PersonaState`; a different adopter surfaces its own). `u8`
    /// (0..=100) keeps the slot `Eq`/hashable and is display-precise. Empty = no vitals
    /// reported — an honest unknown, never fabricated bars; `#[serde(default)]` so an
    /// older slot folds as empty, never dropped.
    #[serde(default)]
    #[ts(type = "Record<string, number>")]
    pub vitals: BTreeMap<String, u8>,
    /// The model backing this member — its display loadout (`model · size ·
    /// ctx`). Folded in from the same per-persona radiator that carries
    /// `vitals` (design B), keyed by id; a member with no bound model (a
    /// human, an unresolved agent) carries `None`. `#[serde(default)]` so a
    /// slot serialized before this field folds as absent, never dropped.
    #[serde(default)]
    #[ts(optional)]
    pub loadout: Option<Loadout>,
    /// URL of this member's avatar IMAGE, when the producing node has one
    /// stored (`~/.continuum/avatars/<peer-id>.png`, served under
    /// `/avatars/…` by the client's static tier). Neutral like every other
    /// slot field: positron transports the URL, never the pixels, and never
    /// interprets it. `None` = no stored avatar — the renderer draws its
    /// glyph fallback, never a broken image or a fabricated face
    /// ([[fallbacks-are-illegal-fail-loud]]). `#[serde(default)]` so a slot
    /// serialized before this field folds as absent, never dropped.
    #[serde(default)]
    #[ts(optional)]
    pub avatar_url: Option<String>,
    /// NAMES of the member's loaded skill overlays (a continuum persona's
    /// paged-in LoRA genes), in load order — the label half of a `genome`
    /// vital that carries only a normalized count. Transported, NOT
    /// interpreted (same neutral discipline as `vitals`): the app decides how
    /// to render them (segment tooltips). Empty = none loaded/reported —
    /// honest-absent, never fabricated labels. `#[serde(default)]` so a slot
    /// serialized before this field folds as empty, never dropped.
    #[serde(default)]
    pub genes: Vec<String>,
    /// Pronouns from the member's published airc identity card (e.g. "she",
    /// "they"). Transported verbatim, never derived by positron. `None` =
    /// no card published — honest-absent (#262).
    #[serde(default)]
    #[ts(optional)]
    pub pronouns: Option<String>,
    /// One-tag role from the identity card (e.g. "continuum-persona-helper",
    /// "continuum-substrate-eng", "human"). Free-form, transported verbatim —
    /// distinct from the coarse styling `kind`. `None` = no card.
    #[serde(default)]
    #[ts(optional)]
    pub role_label: Option<String>,
    /// One-sentence bio from the identity card — the "who is this citizen"
    /// line a roster hover / citizen page renders. Transported verbatim.
    /// `None` = no card published; never a fabricated blurb.
    #[serde(default)]
    #[ts(optional)]
    pub bio: Option<String>,
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/ChatViewState.ts"
)]
pub struct ChatViewState {
    /// The room this snapshot describes.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Human-readable room name (e.g. `"general"`). Substrate-resolved;
    /// widget must not derive from URL slug.
    pub room_name: String,
    /// The room's **activity purpose** — the content-dispatch key
    /// (`"chat"`, `"foundry"`, `"scada"`, …). Per
    /// `docs/architecture/ACTIVITY-ROOM-PATTERNS.md`, `activity == room ==
    /// content == tab`: a client's `Content` primitive dispatches on this to
    /// pick the room's central widget(s), so adding an activity is a purpose
    /// value + a registered renderer, never a shell change. Neutral/opaque
    /// here — positron transports the value, continuum sets it (from the
    /// room's recipe/nature via `RoomPurposeSource`, task #6); it defaults to
    /// `"chat"` until a recipe names another purpose.
    pub purpose: String,
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

/// `ChatViewState` IS a positron `ViewState` — the type-level bridge
/// that lets renderers (positron-lit's `LitHost`) and AI observers
/// (the O6 perception bridge) key off the SAME contract, not a
/// continuum-private shape. This is the "first real `ViewState`" the
/// positron roadmap's O5 names: until now the crate only *framed*
/// payloads into `StateEnvelope`s; here the payload gains its identity
/// under positron's own trait.
///
/// The `Clone + Send + Sync + Debug + 'static` bound the trait requires
/// is already satisfied by the struct's derives + its owned-data fields
/// (`Uuid` / `String` / `Vec`), so no new bounds are introduced.
impl ChatViewState {
    /// The on-wire `kind` string this view is published under — the value
    /// `StateEnvelope.kind` carries and the renderer side routes on.
    /// Owned by the view itself (open self-registration, like a
    /// self-routing command), NOT enumerated in a central catalog: adding
    /// a view kind adds a file, never edits a shared `enum`. An unknown
    /// kind on the wire fails loud at the dispatch seam ("no
    /// renderer/builder registered for kind X"), never coerced to a
    /// default — `[[fallbacks-are-illegal-fail-loud]]` preserved without a
    /// closed enum.
    pub const KIND: &'static str = "chat";
}

impl positron_core::ViewState for ChatViewState {
    fn kind(&self) -> &'static str {
        // Single-source the wire string through the view's own `KIND`
        // const — the same "chat" `StateEnvelope.kind` carries — so the
        // trait's view of the kind can never drift from the envelope's
        // ([[strong-typing-across-boundaries]]: encoded once, on the type).
        Self::KIND
    }

    // `revision()` is intentionally the trait default (`None`): in
    // continuum's substrate the monotonic chat revision is an
    // ENVELOPE-level counter (`Revisions` keyed by the kind string, framed
    // in by `StateBuilder`), NOT a payload field. The `ViewState`
    // trait's revision is satisfied at the envelope layer where the
    // counter actually lives; carrying a copy on the payload struct
    // would be two sources of truth for one counter (the exact drift
    // `[[compression]]` forbids). So the standalone payload honestly
    // reports "no self-carried revision" and the envelope supplies the
    // real one. See `Revisions` for the one-counter-per-kind semantics.
}

/// Top-level state for the `"roster"` widget kind — a room's live participant roster
/// as rich `RosterSlotView`s (name, kind, vitals meters), DECOMPOSED out of
/// `ChatViewState` so the Join Contract's roster REGION binds to its own payload kind
/// (path-3 per-region ViewStates). The experience renderer subscribes to THIS for the
/// display data the manifest's minimal `Member` intentionally omits; the room's
/// message stream stays on `ChatViewState` (`"chat"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/positron/RosterViewState.ts"
)]
pub struct RosterViewState {
    /// The room this roster describes.
    #[ts(type = "string")]
    pub room_id: Uuid,
    /// Members present, in richest form — the same slots the chat view carries,
    /// published under their own kind so a region renderer draws them alone.
    pub roster: Vec<RosterSlotView>,
}

impl RosterViewState {
    /// The on-wire `kind` — the Experience manifest's roster region binds to this.
    pub const KIND: &'static str = "roster";
}

impl positron_core::ViewState for RosterViewState {
    fn kind(&self) -> &'static str {
        Self::KIND
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::ViewState as _;

    // what this catches: RosterViewState is the path-3 decomposed roster kind — its
    // KIND must be "roster" (the Experience roster region binds to it) and it must
    // round-trip, since a renderer keys off both.
    #[test]
    fn roster_view_state_kind_and_round_trip() {
        let rv = RosterViewState {
            room_id: Uuid::nil(),
            roster: vec![],
        };
        assert_eq!(rv.kind(), "roster");
        assert_eq!(RosterViewState::KIND, "roster");
        let json = serde_json::to_string(&rv).expect("serializes");
        let back: RosterViewState = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back, rv);
    }

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
            provenance: Provenance {
                runtime: "claude".into(),
            },
            content: "hi".into(),
            timestamp: 1_700_000_000_000,
        };
        let back: ChatMessageView =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(back.integrations, integrations);
        assert_eq!(back.sender_kind, SenderKind::Agent);
        assert_eq!(back.provenance.runtime, "claude");
    }

    #[test]
    fn from_runtime_is_coarse_and_neutral() {
        // what this catches: the runtime→kind projection is a COARSE
        // styling hint, not identity-sniffing. airc's documented
        // human-driven runtime maps to Human; every other running actor
        // (any AI framework, an automation) maps to Agent — the honest
        // "some running agent" default, NOT a bug-hiding fallback. A
        // regression that grew a per-framework match table here
        // (claude→X, codex→Y) would be the `starts_with("qwen")` smell
        // this codebase kills; the precise signal rides Provenance.
        assert_eq!(SenderKind::from_runtime("interactive"), SenderKind::Human);
        assert_eq!(SenderKind::from_runtime("claude"), SenderKind::Agent);
        assert_eq!(SenderKind::from_runtime("codex"), SenderKind::Agent);
        assert_eq!(SenderKind::from_runtime("automation"), SenderKind::Agent);
        // A roster member is a present citizen — never a System row.
        assert_ne!(SenderKind::from_runtime("anything"), SenderKind::System);
        // Unknown/empty is honest-Agent, never a panic or a fabricated kind.
        assert_eq!(SenderKind::from_runtime(""), SenderKind::Agent);
    }

    #[test]
    fn provenance_rides_the_wire_and_unresolved_is_empty() {
        // what this catches: the accountability slot round-trips and its
        // "not yet resolved" state is an honest empty runtime, never a
        // fabricated origin ([[fallbacks-are-illegal-fail-loud]]). If a
        // refactor dropped `provenance` or gave `unresolved()` a
        // made-up runtime, a security persona / human would attribute a
        // message to the wrong origin.
        assert_eq!(Provenance::unresolved().runtime, "");
        let slot = RosterSlotView {
            pronouns: None,
            role_label: None,
            bio: None,
            member_id: Uuid::from_u128(0xd),
            display_name: "Helper".into(),
            kind: SenderKind::Agent,
            integrations: BTreeMap::new(),
            provenance: Provenance {
                runtime: "claude".into(),
            },
            active: true,
            availability: Some("busy".into()),
            last_seen_ms: 1_700_000_000_000,
            vitals: BTreeMap::new(),
            loadout: None,
            avatar_url: None,
            genes: Vec::new(),
        };
        let back: RosterSlotView =
            serde_json::from_str(&serde_json::to_string(&slot).unwrap()).unwrap();
        assert_eq!(back.provenance.runtime, "claude");
        // what this catches: the neutral presence facts (availability +
        // last_seen_ms) ride the wire verbatim. A regression dropping them —
        // or interpreting availability into a positron-side enum — would
        // silently degrade the persona's roster grounding, which is the whole
        // point of carrying them on the neutral slot.
        assert_eq!(back.availability.as_deref(), Some("busy"));
        assert_eq!(back.last_seen_ms, 1_700_000_000_000);
    }

    #[test]
    fn availability_absent_folds_to_none() {
        // what this catches: a slot serialized before availability/last_seen
        // existed must deserialize (serde default) — availability = None
        // (honest unknown), last_seen_ms = 0 — never a deserialize failure
        // that would blink the whole roster empty.
        let legacy = r#"{"member_id":"00000000-0000-0000-0000-00000000000d","display_name":"Helper","kind":{"kind":"agent"},"integrations":{},"provenance":{"runtime":"claude"},"active":true}"#;
        let slot: RosterSlotView = serde_json::from_str(legacy).unwrap();
        assert_eq!(slot.availability, None);
        assert_eq!(slot.last_seen_ms, 0);
    }

    #[test]
    fn chat_view_state_round_trips() {
        // what this catches: regression where a field rename / type
        // tweak breaks the serde shape. Minimum bar for a wire type.
        let room_id = Uuid::from_u128(0xa);
        let state = ChatViewState {
            room_id,
            room_name: "general".into(),
            purpose: "chat".into(),
            messages: vec![ChatMessageView {
                id: Uuid::from_u128(0xb),
                room_id,
                sender_id: Uuid::from_u128(0xc),
                sender_name: "Joel".into(),
                sender_kind: SenderKind::Human,
                integrations: BTreeMap::new(),
                provenance: Provenance {
                    runtime: "interactive".into(),
                },
                content: "hi".into(),
                timestamp: 1_700_000_000_000,
            }],
            roster: vec![RosterSlotView {
            pronouns: None,
            role_label: None,
            bio: None,
                member_id: Uuid::from_u128(0xd),
                display_name: "Helper".into(),
                kind: SenderKind::Agent,
                integrations: BTreeMap::new(),
                provenance: Provenance {
                    runtime: "claude".into(),
                },
                active: true,
                availability: Some("ready".into()),
                last_seen_ms: 1_700_000_000_000,
                vitals: BTreeMap::new(),
                loadout: None,
                avatar_url: None,
            genes: Vec::new(),
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: ChatViewState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }

    #[test]
    fn chat_view_state_is_a_positron_view_state() {
        // what this catches: regression where `ChatViewState` stops
        // being a positron `ViewState` (the impl deleted, or `kind()`
        // hand-rolled to a literal that drifts from the wire name).
        // Renderers (positron-lit's `LitHost`) and the O6 observer
        // bridge route/subscribe off `ViewState::kind()`; if it stops
        // equalling the `StateEnvelope.kind` the substrate emits
        // (`ChatViewState::KIND`), a widget silently receives
        // state it can't match to a renderer. Also pins `revision()`
        // to the trait default (`None`): the chat revision is an
        // envelope-level counter, never a payload field — a future
        // `revision` field on the struct would be a second source of
        // truth for one counter. The kind is the view's own `KIND`
        // const (open self-registration), never a central enum.
        let state = ChatViewState {
            room_id: Uuid::from_u128(0xa),
            room_name: "general".into(),
            purpose: "chat".into(),
            messages: vec![],
            roster: vec![],
        };
        assert_eq!(state.kind(), "chat");
        assert_eq!(
            state.kind(),
            ChatViewState::KIND,
            "ViewState::kind() must single-source the view's own KIND const, never a drifting literal"
        );
        assert_eq!(
            state.revision(),
            None,
            "revision is an envelope-level counter, not a payload field"
        );
    }
}
