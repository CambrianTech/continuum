//! The names present in a room, read from the SAME `RosterViewState` the browser
//! renders and the citizen's prompt is grounded on (`persona::viewstate_rag`,
//! `PerRoomSubstrates`). One definition of "who is here"; this is a READ of it for
//! the speak gate, never a second roster.

use uuid::Uuid;

/// Display names of everyone the room's roster projection lists. `read_room`, not
/// `for_room`: a read must never mint a substrate for a room nothing projected
/// (#3879). A room with no projection, or a projection that fails to decode, is an
/// empty list — an honest absence the caller treats as "nothing to compare against".
pub fn present_names(room: Uuid) -> Vec<String> {
    let Some(substrate) = crate::ipc::global_room_substrates().read_room(room) else {
        return Vec::new();
    };
    let Some(envelope) = substrate.cache().get(continuum_positron::RosterViewState::KIND) else {
        return Vec::new();
    };
    match serde_json::from_value::<continuum_positron::RosterViewState>(envelope.payload.clone()) {
        Ok(view) => view.roster.into_iter().map(|slot| slot.display_name).collect(),
        Err(_) => Vec::new(),
    }
}
