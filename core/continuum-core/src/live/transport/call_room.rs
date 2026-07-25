//! Call identity = the AIRC room, resolved by AIRC — never a rogue local id (#193, slice 1).
//!
//! Today the call plane keys calls by an arbitrary `call_id: String` with its own
//! `participant_calls` membership map — a coordination namespace running PARALLEL to airc
//! rooms ([[livekit-media-plane-rides-airc-not-parallel]], [[all-rooms-are-airc-rooms-no-mirrors]]).
//! #193 collapses that: a call is NOT its own room concept, it IS an airc room, and the media
//! plane (audio/video SFU) merely rides on top of one.
//!
//! ## Why AIRC must own the id (Joel 2026-07-25)
//!
//! Calls are GLOBAL across the p2p mesh, exactly like chats — a call's participants can be
//! humans and personas from other machines, other neighborhoods, even **totally different
//! non-continuum systems that speak airc**. So the room id has to be the id AIRC assigns,
//! shared identically everywhere. A continuum-LOCAL name→id scheme (e.g. hashing "general" to
//! a uuid) is a NEW rogue id: our hash of "general" would not equal a foreign system's RoomId
//! for the same room, so the two would fragment into separate calls — the exact "cancer of
//! the legacy node" duplicate-id disease #193 exists to cure. The authority for name↔id is
//! `airc_lib::Airc`'s room registry (surfaced through the roster, `room_roster` / airc#1232),
//! consumed via the SAME `AircRosterReader` pattern the persona `RoomRosterSource` already
//! uses — never a parallel reader.
//!
//! ## What is pure here vs airc-backed
//!
//! - **Pure (this slice):** a `call_id` that is ALREADY a room uuid parses to its
//!   [`airc_core::RoomId`] directly — no lookup, it is already airc's id.
//! - **Airc-backed (slice 2):** a `call_id` that is a room NAME is resolved to its `RoomId`
//!   BY AIRC (the roster reader). Deliberately not done here — it needs the async `Airc`
//!   handle and is the coordinated cutover, so this module invents nothing.

use airc_core::RoomId;
use uuid::Uuid;

/// Resolve a wire `call_id` to its airc [`RoomId`] IF it is already a room uuid.
///
/// Returns `Some` for a well-formed room uuid (it is airc's id already — pass it straight
/// through). Returns `None` for a room NAME: names are not ids and MUST be resolved by airc
/// (`AircRosterReader`), never invented locally, so the same room is the same id on every peer
/// and every foreign airc system. The caller that gets `None` routes the name through airc.
pub fn call_room_id_if_uuid(call_id: &str) -> Option<RoomId> {
    Uuid::parse_str(call_id.trim()).ok().map(RoomId::from_uuid)
}

/// Whether a wire `call_id` still needs airc name-resolution (it is not already a room uuid).
/// A thin readability wrapper over [`call_room_id_if_uuid`] for the call sites that branch on
/// "resolve locally vs ask airc".
pub fn needs_airc_resolution(call_id: &str) -> bool {
    call_room_id_if_uuid(call_id).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#193): a room uuid on the wire IS airc's id — it resolves to exactly
    // that RoomId with no lookup, so a client already sending the airc room uuid as its call_id
    // lands in the correctly-keyed call across the whole mesh.
    #[test]
    fn a_room_uuid_is_airc_id_already() {
        let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        assert_eq!(
            call_room_id_if_uuid(&uuid.to_string()),
            Some(RoomId::from_uuid(uuid)),
            "a wire uuid maps to exactly that RoomId"
        );
        // Surrounding whitespace doesn't change the id.
        assert_eq!(
            call_room_id_if_uuid(&format!("  {uuid} ")),
            Some(RoomId::from_uuid(uuid)),
        );
    }

    // what this catches (the anti-rogue-id contract): a room NAME is NOT resolved locally — the
    // function refuses to invent an id, returning None so the caller must ask airc. If this
    // ever started hashing names to ids again, two systems would compute different ids for the
    // same room and fragment the call — the bug this whole slice exists to prevent.
    #[test]
    fn a_room_name_is_never_resolved_locally() {
        assert_eq!(call_room_id_if_uuid("general"), None);
        assert_eq!(call_room_id_if_uuid("academy"), None);
        assert!(needs_airc_resolution("general"));
        assert!(!needs_airc_resolution(&Uuid::from_u128(1).to_string()));
    }

    // what this catches: distinct room uuids stay distinct ids (no over-collapse that would
    // bleed two calls together).
    #[test]
    fn distinct_uuids_stay_distinct() {
        assert_ne!(
            call_room_id_if_uuid(&Uuid::from_u128(1).to_string()),
            call_room_id_if_uuid(&Uuid::from_u128(2).to_string()),
        );
    }
}
