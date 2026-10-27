//! `RoomPurposeSource` — the ONE seam that resolves a room's **purpose** (#6).
//!
//! A room's purpose is its activity nature — `"chat"`, `"foundry"`, `"scada"`, … — and
//! it is **recipe-defined, never an enum** ([[room-purpose-is-per-recipe-not-an-enum]]):
//! a room instantiates a recipe (`RecipeDefinitionShape`), and the recipe's nature IS the
//! purpose. The positron chat projection dispatches its `Content` on this string, so this
//! is the seam that makes `activity = room = content = tab` real.
//!
//! This is deliberately a **trait**, resolved in ONE place, so the projection never
//! hardcodes a purpose (it did — `positron_source.rs` said `"chat"` for every room). Today
//! the default honestly answers `"chat"` (the only recipe live end-to-end); when the
//! room→recipe store lands, a `RecipeRoomPurpose` impl plugs in here and *every* room —
//! foundry, scada, academy — reports its own purpose with **zero projection change**. That
//! is the engine move: de-hardcode once, and the whole dispatch follows the data.

use std::sync::Arc;
use uuid::Uuid;

/// Resolves a room id → its activity purpose (the `Content` dispatch key).
pub trait RoomPurposeSource: Send + Sync {
    /// The room's purpose. MUST be total — an unknown room resolves to the honest
    /// default (`"chat"`), never a fabricated or panicking value; a room the resolver
    /// has never seen is simply a plain chat room until its recipe says otherwise.
    fn purpose_for(&self, room_id: Uuid) -> String;

    /// The activity this room was spawned UNDER (the binding's `parent`), if any —
    /// what lets a navigator nest a run room under the room it was dispatched from
    /// instead of listing every activity flat. Default `None`: a source that only
    /// knows purposes nests nothing.
    fn parent_for(&self, _room_id: Uuid) -> Option<Uuid> {
        None
    }
}

/// The default until the room→recipe store exists: every room is a chat room. Honest,
/// not fabricated — `"chat"` is the one recipe wired end-to-end today. Swapped for a
/// `RecipeRoomPurpose` (reads the room's recipe) in the next brick, no call-site change.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultRoomPurpose;

impl RoomPurposeSource for DefaultRoomPurpose {
    fn purpose_for(&self, _room_id: Uuid) -> String {
        "chat".to_string()
    }
}

/// The shared handle the projection holds. `Arc<dyn …>` so the concrete resolver is
/// injected at boot and swapped without touching the projection.
pub type SharedRoomPurpose = Arc<dyn RoomPurposeSource>;

/// The process default resolver.
pub fn default_source() -> SharedRoomPurpose {
    Arc::new(DefaultRoomPurpose)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the default is total + honest — ANY room resolves to "chat"
    // (never a panic, never a fabricated purpose), so the projection can drop its
    // hardcode and route through the seam with identical behavior until recipes land.
    #[test]
    fn default_resolves_every_room_to_chat() {
        let src = default_source();
        assert_eq!(src.purpose_for(Uuid::from_u128(0)), "chat");
        assert_eq!(src.purpose_for(Uuid::from_u128(42)), "chat");
    }
}
