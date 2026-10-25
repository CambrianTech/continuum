//! THE airc member -> neutral positron [`RosterSlotView`] projection, one place for every
//! reader: the persona's grounding roster, the presence emitter, the chat projection. It
//! is pure (no I/O, no policy) and maps two lower crates' types (airc's `RoomMember`,
//! positron's slot), so it lives with the mind that grounds on it rather than in the
//! socket layer above.

use std::collections::BTreeMap;

use airc_lib::{AgentAvailabilityState, RoomMember};
use continuum_positron::{Provenance, RosterSlotView, SenderKind};
use uuid::Uuid;

/// A provisional display label for a sender whose identity card has not
/// yet folded in through presence — the first 8 chars of the peer id,
/// prefixed. Deliberately unmistakable for a real name so a stuck-
/// provisional row is visible, not silently wrong. Upgraded in place the
/// instant `presence:updated` resolves the card
/// ([[fallbacks-are-illegal-fail-loud]]).
///
/// One provisional-label source for every reader (the chat projection, the presence
/// emitter, the persona's grounding roster): compression principle.
pub fn provisional_sender_name(sender_id: Uuid) -> String {
    format!("peer-{}", airc_core::shown_id::shown_form(sender_id))
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
/// The WS emitter (`ipc::positron_presence`) and the persona grounding source
/// (`persona::room_roster_source`) call the identical function.
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
pub fn roster_slot_from_card(member: &airc_lib::RoomMemberCard) -> RosterSlotView {
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
