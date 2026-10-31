//! THE airc member -> neutral positron [`RosterSlotView`] projection, one place for every
//! reader: the persona's grounding roster, the presence emitter, the chat projection. It
//! is pure (no I/O, no policy) and maps two lower crates' types (airc's `RoomMember`,
//! positron's slot), so it lives with the mind that grounds on it rather than in the
//! socket layer above.

use std::collections::BTreeMap;

use airc_lib::AgentAvailabilityState;
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
    let simple = sender_id.simple().to_string();
    format!("peer-{}", &simple[..8]) // head-cut-ok: the board's 8-char id rendering of a label, never prompt content cut short
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

/// The CARDS-flavored sibling of the member projection the legacy roster source used (#262): projects
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
        // the member projection the legacy roster source used.
        vitals: BTreeMap::new(),
        loadout: None,
        avatar_url: None,
        genes: Vec::new(),
    }
}
