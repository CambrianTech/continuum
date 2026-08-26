//! `ActivityRoom` — the compile-time witness that a cognition turn belongs to a
//! real activity.
//!
//! THE LAW (Joel, 2026-08-26): *"shouldn't be possible to create activities
//! without rooms"* — and its corollary for benchmarks, *"benchmarks without new
//! activities (unless rejoining)"*. Rooms and activities are the same thing seen
//! from two angles (docs/activities/ROOMS-AND-ACTIVITIES.md), so a turn that
//! cannot name its room is work no room can see: no act receipts, no peer, no
//! human, no ViewState, no curriculum attribution. That is #425, and it was
//! measured at 13,209 roomless turns — 35% of one citizen's cognition —
//! invisible to every surface.
//!
//! The recurring defect shape was never a missing value; it was `Uuid::nil()`
//! FLOWING AS IF IT WERE A ROOM: the literal string
//! `"00000000-0000-0000-0000-000000000000"` on the inference wire (where it
//! became a live KV slot-lease key), a phantom engram `context_id`, a
//! `[room 000…0]` prompt header. An `Option<Uuid>` cannot fix that, because the
//! optional form makes "no room" and "nobody wired this up" the same value —
//! the exact argument [`crate::cognition::workspace::Cause`] already records
//! for causes. This newtype closes the door at construction instead: the inner
//! field is private, [`ActivityRoom::new`] refuses nil, and the only roomless
//! constructor is `#[cfg(test)]`.
//!
//! What is NOT an activity, and therefore not forced through this type:
//! background non-activity inference (dream consolidation and its kin, blessed
//! roomless in `persona/unified.rs`) stays persona-attributed with `purpose`
//! set and `room_id: None` on the wire — an honest absence, never a nil
//! pretending to be a room.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A turn tried to enter cognition without a real room. Refused at
/// construction — the caller must name (mint or rejoin) an activity first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("roomless turn refused: an activity without a room is unrepresentable (#425)")]
pub struct RoomlessTurn;

/// Witness that a turn belongs to a real activity room.
///
/// Wraps the canonical typed id ([`airc_core::RoomId`]) — never a string, never
/// a bare `Uuid` that nil can impersonate. `Copy` because it is an id, and
/// `Hash`/`Eq` because it keys typed maps (the KV slot lease keys on
/// `(persona, room)` structs — Joel: *"we do NOT use strings for keys"*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActivityRoom(airc_core::RoomId);

impl ActivityRoom {
    /// The one production constructor: a real room id, refused if nil.
    pub fn new(room: airc_core::RoomId) -> Result<Self, RoomlessTurn> {
        if room.as_uuid().is_nil() {
            return Err(RoomlessTurn);
        }
        Ok(Self(room))
    }

    /// Convenience over the `Uuid` most cognition call sites already hold.
    pub fn from_uuid(room: Uuid) -> Result<Self, RoomlessTurn> {
        Self::new(airc_core::RoomId::from_u128(room.as_u128()))
    }

    /// Mint a fresh activity identity. This is the "benchmarks mint NEW
    /// activities (unless rejoining)" half of the law: a run/exam/solve that
    /// has no room yet NAMES one here rather than running invisibly under nil.
    /// The id is real and unique from birth; spawning the joinable airc room
    /// entity around it is the activity layer's job, not a precondition for
    /// the turn to be attributable.
    pub fn mint() -> Self {
        Self(airc_core::RoomId::from_u128(Uuid::new_v4().as_u128()))
    }

    pub fn room_id(&self) -> airc_core::RoomId {
        self.0
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0.as_uuid()
    }

    /// A real-shaped, non-nil room for tests. Deterministic so fixtures and
    /// byte-diff assertions stay reproducible.
    #[cfg(test)]
    pub fn test_room() -> Self {
        Self(airc_core::RoomId::from_u128(0x7e57_0000_0000_0000_0000_0000_0000_0001))
    }
}

impl std::fmt::Display for ActivityRoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_uuid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the #425 door itself — nil can never become an
    // ActivityRoom through ANY constructor, so a roomless turn is a compile/
    // construction failure instead of a phantom "000…0" room on the wire.
    #[test]
    fn nil_is_refused_by_every_constructor() {
        assert_eq!(
            ActivityRoom::new(airc_core::RoomId::from_u128(0)),
            Err(RoomlessTurn)
        );
        assert_eq!(ActivityRoom::from_uuid(Uuid::nil()), Err(RoomlessTurn));
        assert!(!ActivityRoom::mint().as_uuid().is_nil());
        assert!(!ActivityRoom::test_room().as_uuid().is_nil());
    }

    // what this catches: mint() must produce DISTINCT activities — two minted
    // solves colliding on one id would recreate the shared-slot KV thrash the
    // per-activity lease exists to prevent.
    #[test]
    fn minted_activities_are_distinct() {
        assert_ne!(ActivityRoom::mint(), ActivityRoom::mint());
    }
}
