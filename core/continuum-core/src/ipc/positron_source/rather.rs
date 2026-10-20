use super::*;

/// Bus event prefix carrying posted-message payloads. A cheap prefix
/// check keeps presence/media/transport events out of the message arm.
///
/// `pub(crate)` because the EMITTER
/// (`airc::inbound_attach::publish_transcript_event`) and this CONSUMER
/// must agree on the wire name — one string, one source of truth
/// (compression principle). The emitter imports this const rather than
/// re-typing the literal.
pub(crate) const CHAT_POSTED: &str = "chat:posted";

/// A citizen's inbound stream ADMITTED a posted line as a turn — the delivery
/// receipt folded onto the row as `heard_by` ("heard by 8").
pub(crate) const CHAT_HEARD: &str = "chat:heard";

/// Bus event carrying a room roster/presence delta.
///
/// `pub(crate)` for the same reason as [`CHAT_POSTED`]: the presence
/// EMITTER (`crate::ipc::positron_presence`) and this CONSUMER must agree
/// on the wire name — one string, one source of truth.
pub(crate) const PRESENCE_UPDATED: &str = "presence:updated";

/// Bus event carrying an EXPLICIT chat-focus switch — the `nav/select` verb's
/// projection cue. Where `chat:posted` / `presence:updated` move the focused
/// room *implicitly* (the accumulator follows events), this is the citizen
/// saying "show me THIS room" (NAVIGATION-ACROSS-MODALITIES.md §2, the
/// `switchTo`/`select` intent reaching the chat face).
///
/// `pub(crate)` for the same reason as [`CHAT_POSTED`]: the EMITTER
/// (`crate::modules::nav`, the `nav/select` command) and this CONSUMER must
/// agree on the wire name — one string, one source of truth.
pub(crate) const CHAT_FOCUSED: &str = "chat:focused";

/// Bounded message window carried in each snapshot. Matches the
/// `chat/poll` default (`ChatPollParams.limit` defaults to 50) — the
/// renderer shows a recent window; deeper history is a `chat/history`
/// pull, not a fatter snapshot (see `ChatViewState.messages` doc).
pub(super) const MAX_MESSAGES_PER_SNAPSHOT: usize = 100;

/// Bound on the act-receipt ring (#243). Acts arrive far denser than speech
/// during a work burst (a solve can run 30+ acts per attempt), and the
/// renderer collapses consecutive receipts anyway — the window exists to show
/// the CURRENT work rhythm, not to be the act ledger (that is the run
/// ledger / working memory).
pub(super) const MAX_ACTS_PER_SNAPSHOT: usize = 60;

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

/// The `chat:heard` payload (see [`CHAT_HEARD`]).
#[derive(Debug, Clone, Deserialize)]
pub(super) struct AircChatHeard {
    pub(super) message_id: Uuid,
    pub(super) room_id: Uuid,
    pub(super) persona_id: Uuid,
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
pub(crate) struct AircChatPosted {
    pub(crate) message_id: Uuid,
    pub(crate) room_id: Uuid,
    pub(crate) sender_id: Uuid,
    pub(crate) content: String,
    pub(crate) timestamp: u64,
}

/// The airc bus wraps event bodies under a `payload` key (see
/// `airc_bridge_directive::str_field`); accept a nested `payload` object,
/// else the top-level value. ONE envelope rule for every bus consumer.
pub(crate) fn bus_event_body(payload: &serde_json::Value) -> &serde_json::Value {
    payload.get("payload").unwrap_or(payload) // unwrap_or: no envelope = the value IS the body (both shapes are on the wire)
}

/// Parse a `chat:posted` bus event — shared by the chat projection and the
/// voice module's reply speaker (compression: one wire shape, one parser).
pub(crate) fn parse_chat_posted(name: &str, payload: &serde_json::Value) -> Option<AircChatPosted> {
    if name != CHAT_POSTED {
        return None;
    }
    serde_json::from_value::<AircChatPosted>(bus_event_body(payload).clone()).ok()
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

/// Typed `chat:focused` payload — an explicit focus switch to `room_id`. Same
/// `pub(crate)` + `Serialize` "agree by construction" discipline as
/// [`AircPresenceUpdate`]: the emitter (`modules::nav`'s `nav/select`) builds
/// THIS struct, never a hand JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AircChatFocused {
    pub(crate) room_id: Uuid,
}

/// Event name a persona radiates its live vitals under.
pub(crate) const PERSONA_VITALS: &str = "persona:vitals";

/// Event name a persona's HANDS radiate one executed tool act under (#243:
/// tool actions show IN chats as collapsed receipts).
pub(crate) const PERSONA_ACT: &str = "persona:act";

/// Typed `persona:act` payload — one executed tool call, radiated from the
/// act→observe seam (`apply_act`, the ONE choke point every hand action
/// passes through: live turns, directed turns, and agent/solve alike). The
/// projection folds it into [`ChatViewState::acts`] so every client renders
/// the same receipt stream. Same `pub(crate)` + `Serialize` agree-by-
/// construction discipline as [`PersonaVitalsUpdate`]: the emitter builds
/// THIS struct, never a hand JSON. Carries the resolved `actor_name`
/// because the producer (the acting body) knows it authoritatively — the
/// projection still re-resolves against the roster when the actor is
/// present, keeping one display-identity source where possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersonaActUpdate {
    pub(crate) act_id: Uuid,
    pub(crate) room_id: Uuid,
    pub(crate) actor_id: Uuid,
    pub(crate) actor_name: String,
    pub(crate) tool: String,
    pub(crate) summary: String,
    /// Legacy outcome bool, unchanged in meaning for the clients already reading
    /// it: false ONLY for a real failure.
    pub(crate) ok: bool,
    /// The act's TYPED state — "succeeded" | "failed" | "running" | "undecodable"
    /// | "unprojected". `ok` cannot express a call that was accepted but has not
    /// finished, nor a result the substrate could not decode, and rendering either
    /// as a tick is card f6c50a49. New field rather than a redefinition of `ok`, so
    /// a client that has not adopted it is not silently given a new meaning.
    #[serde(default)]
    pub(crate) state: String,
    pub(crate) timestamp: u64,
}

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
    /// The model backing this persona (`model · size · ctx`) — the slow-moving
    /// capability half of the same radiator that carries `vitals`. `None` when
    /// the persona's cycle reports no bound model. `#[serde(default)]` so a
    /// vitals payload minted before this field deserializes with no loadout.
    #[serde(default)]
    pub(crate) loadout: Option<Loadout>,
    /// NAMES of the paged-in LoRA genes, in page-in order — the label half of
    /// the `genome` vital (which carries only the normalized count). Lets the
    /// tile name each lit genome segment. Empty = base model, no genes.
    /// `#[serde(default)]` so a payload minted before this field deserializes
    /// with no names — honest-absent, never fabricated labels.
    #[serde(default)]
    pub(crate) genes: Vec<String>,
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
pub(super) fn availability_label(state: AgentAvailabilityState) -> &'static str {
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
        // The thin `room_roster` path carries no identity card — these fill
        // only on the cards path ([`roster_slot_from_card`], #262). Honest
        // absent, never fabricated.
        pronouns: None,
        role_label: None,
        bio: None,
        // airc's `RoomMember` carries no vitals — a continuum persona's live
        // `PersonaState` (energy/attention/compute) arrives on its OWN
        // `persona:vitals` event and is folded into the slot by id at `store`
        // time (see [`PersonaVitalsUpdate`] / [`ChatProjection::apply_vitals`]),
        // never through this presence path. Empty here = no vitals reported,
        // never fabricated bars ([[fallbacks-are-illegal-fail-loud]]).
        vitals: BTreeMap::new(),
        // Same design B as vitals: an AI member's loadout (`model · size · ctx`)
        // arrives on its OWN `persona:vitals` event and is folded in by id at
        // `store` time. `None` here = no bound model reported (a human, an
        // unresolved agent) — honest-absent, never a fabricated model.
        loadout: None,
        // The avatar IMAGE is a node-local disk fact the presence EMITTER
        // enriches (it owns the store scan); this shared projection stays
        // pure — no I/O here, absent by default.
        avatar_url: None,
        // Gene names arrive on the persona's OWN `persona:vitals` radiate and
        // are folded in at `store` time, same as vitals/loadout — never
        // through this presence path. Empty = none reported.
        genes: Vec::new(),
    }
}

/// The CARDS-flavored sibling of [`roster_slot_from_member`] (#262): projects
/// an airc [`airc_lib::RoomMemberCard`] — presence + the FULL published
/// identity card — into the same neutral slot. This is what makes a citizen's
/// pronouns / role / bio / integrations reach every rendered roster instead
/// of dying in the daemon's identity store. `identity: None` (present but
/// uncarded) degrades to exactly the member-path projection: provisional
/// name, honest-absent card fields.
pub(crate) fn roster_slot_from_card(member: &airc_lib::RoomMemberCard) -> RosterSlotView {
    let kind = SenderKind::from_runtime(&member.runtime);
    let identity = member.identity.as_ref();
    let non_empty = |s: &str| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_string())
    };
    let display_name = identity
        .and_then(|i| non_empty(&i.name))
        .unwrap_or_else(|| provisional_sender_name(member.peer_id.as_uuid()));
    RosterSlotView {
        member_id: member.peer_id.as_uuid(),
        display_name,
        kind,
        // The card path is where cross-system badges finally flow (the
        // member path's "no badge map" comment stops being true here).
        integrations: identity.map(|i| i.integrations.clone()).unwrap_or_default(),
        provenance: Provenance {
            runtime: member.runtime.clone(),
        },
        active: true,
        availability: member
            .availability
            .map(availability_label)
            .map(str::to_owned),
        last_seen_ms: member.last_seen_ms,
        pronouns: identity.and_then(|i| non_empty(&i.pronouns)),
        role_label: identity.and_then(|i| non_empty(&i.role)),
        bio: identity.and_then(|i| non_empty(&i.bio)),
        // Vitals / loadout / avatar / genes fold in by id downstream, same
        // as the member path — see the field notes on
        // [`roster_slot_from_member`].
        vitals: BTreeMap::new(),
        loadout: None,
        avatar_url: None,
        genes: Vec::new(),
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
        pronouns: None,
        role_label: None,
        bio: None,
        member_id: member,
        display_name: name.to_string(),
        kind,
        integrations: BTreeMap::new(),
        provenance: Provenance::unresolved(),
        active: true,
        availability: None,
        last_seen_ms: 0,
        vitals: BTreeMap::new(),
        loadout: None,
        genes: Vec::new(),
        avatar_url: None,
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
/// Re-resolve each stored message's identity from the room's current roster.
/// Only **upgrades** — a message whose sender is present in the roster adopts
/// the card; a sender who has since left keeps its already-resolved identity.
pub(super) fn reresolve_messages(accum: &mut RoomAccum) {
    let roster = &accum.roster;
    let mut resolved: Vec<(usize, RosterSlotView)> = Vec::new();
    for (i, msg) in accum.messages.iter().enumerate() {
        if let Some(slot) = roster.iter().find(|s| s.member_id == msg.sender_id) {
            resolved.push((i, slot.clone()));
        }
    }
    for (i, slot) in resolved {
        if let Some(msg) = accum.messages.get_mut(i) {
            msg.sender_name = slot.display_name.clone();
            msg.sender_kind = slot.kind;
            msg.integrations = slot.integrations.clone();
            msg.provenance = slot.provenance.clone();
        }
    }
}

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

/// Accumulates airc room events into the renderer-shaped
/// [`ChatViewState`] and writes each transition to the [`Substrate`].
///
/// Holds the single focused room's state (see the single-active-room note
/// in the module docs). Not `Clone` — one owner per projection; the
/// consume loop owns it.
pub(super) struct ChatProjection {
    pub(super) substrate: Substrate,
    /// Per-ROOM stores (#408). The node `substrate` above holds ONE room's view at
    /// a time — the focused room's — which is correct for today's web session and
    /// useless to a citizen standing in a different room. Every per-room envelope
    /// is ALSO written to its room's own store here, so a consumer that knows which
    /// room it means (a persona's grounding via `ViewStateRagSource`) can read THAT
    /// room instead of whichever wrote last.
    ///
    /// ONE fold, TWO sinks — not two folds. The projection computes the view once;
    /// the same `StateEnvelope` (same revision) lands in both. A second FOLD is what
    /// goes stale (#346); a second SINK of one fold cannot.
    ///
    /// `None` in tests/headless — the node substrate alone, today's behavior.
    pub(super) rooms: Option<std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>>,
    pub(super) builder: StateBuilder,
    /// The FOCUSED room — the one the node substrate mirrors. `None` before
    /// the first recognized event (cold boot adopts the first event's room).
    pub(super) room_id: Option<Uuid>,
    /// An EXPLICIT focus (the `nav/select` verb via [`CHAT_FOCUSED`]). Once
    /// held, only another select moves the node mirror. With per-room
    /// accumulators (2026-08-31) events NEVER move it — each room folds into
    /// its own accumulator regardless of focus, so nothing is dropped and
    /// nothing wanders (the "room B's presence yanks the view" hazard is
    /// structurally gone, not guarded against).
    pub(super) explicit_focus: Option<Uuid>,
    /// PER-ROOM accumulators (tab = content = room = activity, server half):
    /// every room this projection has seen folds its own messages / acts /
    /// roster / name and its own emit-on-change caches. The node substrate
    /// mirrors exactly one of these — the focused room's.
    pub(super) accums: HashMap<Uuid, RoomAccum>,
    /// Latest live vitals per member id, folded from `persona:vitals` events
    /// (design B). Kept SEPARATE from the presence roster — presence replaces
    /// the roster wholesale on every update, so vitals live here and are merged
    /// into each slot at `store` time, surviving roster churn. A member with no
    /// entry simply has empty vitals (no meter), never a fabricated one.
    pub(super) vitals: HashMap<Uuid, BTreeMap<String, u8>>,
    /// Latest radiated **loadout** (`model · size · ctx`) per member id, folded
    /// from the SAME `persona:vitals` events. Kept SEPARATE from the roster for
    /// the same reason as `vitals` — presence replaces the roster wholesale, so
    /// the loadout overlay lives here and is merged into each slot at `store`
    /// time, surviving roster churn. A member with no entry carries `None` (no
    /// loadout strip), never a fabricated model.
    pub(super) loadout: HashMap<Uuid, Loadout>,
    /// Latest radiated gene NAMES per member id, folded from the SAME
    /// `persona:vitals` events — the label half of the `genome` vital. Same
    /// separate-overlay discipline as `vitals`/`loadout` (survives roster
    /// churn). Empty entry = base model, no genes.
    pub(super) genes: HashMap<Uuid, Vec<String>>,
    /// Resolves this room's activity **purpose** (the `Content` dispatch key) — #6.
    /// The projection no longer hardcodes `"chat"`; it routes through this seam so a
    /// foundry / scada / academy room reports its own recipe-defined purpose with NO
    /// projection change once the real (recipe-backed) resolver is injected.
    pub(super) purpose_source: crate::ipc::room_purpose::SharedRoomPurpose,
    /// Recipe-backed source for this room's [`Experience`] manifest (the Join
    /// Contract), keyed by the SAME purpose the chat view uses. Published alongside
    /// chat so a renderer/agent sees the full room — regions, affordances, and the
    /// live membership — not just the message stream.
    /// `[[join-contract-experience-is-a-latent-space]]`.
    pub(super) experience_source: RecipeExperienceSource,
    /// Own monotonic revisions well for the `"experience"` kind — the projection is
    /// its sole writer, exactly as `builder` is for `"chat"`.
    pub(super) experience_builder: StateBuilder,
    /// Own Revisions well for the decomposed `"roster"` kind (path-3): the Experience
    /// manifest's roster region binds to this rich payload (names/kinds/vitals).
    pub(super) roster_builder: StateBuilder,
}

/// One room's fold state — everything that is a fact about A ROOM rather than
/// the node: its transcript rings, its live roster, its name, and its own
/// emit-on-change caches. Member-truth overlays (vitals/loadout/genes) stay on
/// the projection: a citizen's energy is HERS, in every room she stands in.
#[derive(Default)]
pub(super) struct RoomAccum {
    pub(super) room_name: String,
    pub(super) messages: VecDeque<ChatMessageView>,
    pub(super) acts: VecDeque<ActReceiptView>,
    pub(super) roster: Vec<RosterSlotView>,
    pub(super) last_experience: Option<Experience>,
    pub(super) last_roster: Option<Vec<RosterSlotView>>,
    pub(super) last_chat: Option<ChatViewState>,
}
