//! Call identity = the AIRC room, PURELY derived — never a rogue local id (#193, slice 1+2a).
//!
//! Today the call plane keys calls by an arbitrary `call_id: String` with its own
//! `participant_calls` membership map — a coordination namespace running PARALLEL to airc
//! rooms ([[livekit-media-plane-rides-airc-not-parallel]], [[all-rooms-are-airc-rooms-no-mirrors]]).
//! #193 collapses that: a call is NOT its own room concept, it IS an airc room, and the media
//! plane (audio/video SFU) merely rides on top of one.
//!
//! ## AIRC owns the id — and derivation is PURE (BigMama, 2026-07-25)
//!
//! Calls are GLOBAL across the p2p mesh — participants can be humans/personas on other
//! machines or **foreign non-continuum systems that speak airc** — so the room id must be the
//! id AIRC assigns. Crucially, a subscribed room's id is not looked up in a directory, it is
//! DERIVED: `airc_lib::derive_room_id(identity, channel) = Uuid::new_v5(SUBSCRIPTIONS_NAMESPACE,
//! identity ++ NUL ++ channel)`. Same `(mesh identity, channel)` → same `RoomId` on every
//! machine, which is exactly how airc subscriptions compute it — so a call room IS the
//! subscribed room. We call airc's own `derive_room_id` + `ChannelName` (never a local hash),
//! so a room named "general" is the same call for every peer on that mesh identity.
//!
//! Out of scope here (coordinated slice 2b): wiring this into the `CallManager` keying (every
//! entry normalizes to the derived `RoomId`) and deriving MEMBERSHIP from the airc roster
//! (`AircRosterReader` / `Airc::room_roster`, BigMama's domain). The wire type
//! `CallMessage.call_id: String → RoomId` is a further gated step (ts-rs, browser client).

use airc_core::RoomId;
use airc_lib::{derive_room_id, ChannelName, MeshIdentity};
use uuid::Uuid;

/// Resolve a wire `call_id` to its airc [`RoomId`] IF it is already a room uuid (no identity
/// needed — a uuid is airc's id already). `None` means the `call_id` is a room NAME and must be
/// derived with the mesh identity via [`resolve_call_room`].
pub fn call_room_id_if_uuid(call_id: &str) -> Option<RoomId> {
    Uuid::parse_str(call_id.trim()).ok().map(RoomId::from_uuid)
}

/// Resolve ANY wire `call_id` to its canonical airc [`RoomId`] — PURELY, no I/O, no lookup.
///
/// - A **uuid** is airc's id already → pass through (the `identity` is irrelevant).
/// - A **name** is derived with airc's own `derive_room_id(identity, ChannelName)`, so it is
///   the SAME id airc's subscriptions produce for that `(mesh identity, channel)` — a call
///   room is literally the subscribed room, identical on every peer and foreign airc system on
///   that identity. `ChannelName::new` does the normalization (`#general`/`General`/`general`
///   → `general`); an invalid/empty name yields `None` rather than an invented id.
pub fn resolve_call_room(identity: &MeshIdentity, call_id: &str) -> Option<RoomId> {
    if let Some(id) = call_room_id_if_uuid(call_id) {
        return Some(id);
    }
    ChannelName::new(call_id)
        .ok()
        .map(|channel| derive_room_id(identity, &channel))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#193): a room uuid on the wire IS airc's id — it resolves to exactly
    // that RoomId regardless of identity, so a client already sending the airc room uuid lands
    // in the correctly-keyed call across the whole mesh.
    #[test]
    fn a_room_uuid_is_airc_id_already() {
        let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        assert_eq!(
            call_room_id_if_uuid(&uuid.to_string()),
            Some(RoomId::from_uuid(uuid)),
        );
        // Even through the full resolver, a uuid ignores the identity and passes through.
        let id = MeshIdentity::new("some-mesh");
        assert_eq!(
            resolve_call_room(&id, &format!("  {uuid} ")),
            Some(RoomId::from_uuid(uuid)),
        );
    }

    // what this catches: a room NAME resolves to AIRC's OWN derived id — resolve_call_room must
    // equal derive_room_id(identity, ChannelName::new(name)) exactly, so a call room IS the
    // subscribed room. If we ever derived it any other way, the call would land in a different
    // room than the chat subscription for the same name — the parallel-namespace bug.
    #[test]
    fn a_name_derives_airc_canonical_room_id() {
        let id = MeshIdentity::new("cambriantech");
        let expected = derive_room_id(&id, &ChannelName::new("general").unwrap());
        assert_eq!(resolve_call_room(&id, "general"), Some(expected));
    }

    // what this catches (the unification): every spelling of a room name collapses to ONE
    // RoomId (airc's ChannelName normalizes #/case/whitespace), so two peers that "join general"
    // meet in the same call.
    #[test]
    fn name_spellings_collapse_to_one_room() {
        let id = MeshIdentity::new("cambriantech");
        let canonical = resolve_call_room(&id, "general");
        assert!(canonical.is_some());
        for spelling in ["general", "General", "#general", "  #GENERAL "] {
            assert_eq!(
                resolve_call_room(&id, spelling),
                canonical,
                "'{spelling}' == general"
            );
        }
    }

    // what this catches: the id is SCOPED to the mesh identity — "general" on two different
    // mesh identities is two different rooms (they are, by design), and an invalid/empty name
    // yields None rather than an invented rogue id.
    #[test]
    fn identity_scopes_the_room_and_bad_names_are_none() {
        let a = MeshIdentity::new("mesh-a");
        let b = MeshIdentity::new("mesh-b");
        assert_ne!(
            resolve_call_room(&a, "general"),
            resolve_call_room(&b, "general"),
            "same channel, different mesh identity ⇒ different room"
        );
        assert_ne!(
            resolve_call_room(&a, "general"),
            resolve_call_room(&a, "academy")
        );
        assert_eq!(
            resolve_call_room(&a, ""),
            None,
            "empty name ⇒ no invented id"
        );
        assert_eq!(
            resolve_call_room(&a, "bad name!"),
            None,
            "invalid name ⇒ None"
        );
    }
}
