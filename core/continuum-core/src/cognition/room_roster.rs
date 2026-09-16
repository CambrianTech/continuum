//! The names present in a room, read from the SAME `RosterViewState` the browser
//! renders and the citizen's prompt is grounded on (`persona::viewstate_rag`,
//! `PerRoomSubstrates`). One definition of "who is here"; this is a READ of it for
//! the speak gate, never a second roster.

use uuid::Uuid;

/// Display names of everyone the room's roster projection lists. `read_room`, not
/// `for_room`: a read must never mint a substrate for a room nothing projected
/// (#3879). A room with no projection, or a projection that fails to decode, is an
/// empty list — an honest absence the caller treats as "nothing to compare against".
///
/// The read EMITS on every path it takes, not only when it finds names (Cormac's review
/// of #4129): an empty list has three different causes — no substrate for the room, no
/// roster envelope in it, a payload that no longer decodes — and the third is a bug
/// wearing the first's face. `cognition.identity_claim.roster {outcome, size}` is how a
/// reader tells "armed and clean" from "had nothing to check against".
pub fn present_names(room: Uuid) -> Vec<String> {
    let (outcome, names): (&str, Vec<String>) =
        match crate::ipc::global_room_substrates().read_room(room) {
            None => ("no_substrate", Vec::new()),
            Some(substrate) => match substrate.cache().get(continuum_positron::RosterViewState::KIND) {
                None => ("no_roster_envelope", Vec::new()),
                Some(envelope) => match serde_json::from_value::<continuum_positron::RosterViewState>(
                    envelope.payload.clone(),
                ) {
                    Ok(view) => ("read", view.roster.into_iter().map(|slot| slot.display_name).collect()),
                    Err(_) => ("decode_failed", Vec::new()),
                },
            },
        };
    crate::probe!(
        class = "cognition.identity_claim.roster",
        room = %room,
        outcome = outcome,
        size = names.len() as u64,
        "the names the speak gate can compare against this turn"
    );
    names
}
