//! HER MIND ROOM: the private space as an activity whose only member is her
//! (`docs/architecture/PRIVACY-OF-THOUGHT.md` §4; Joel, 2026-10-04: "privacy of thought is
//! essential to agency").
//!
//! A turn is private because of the room it runs in, known before retrieval. It is never
//! discovered mid-turn when a sealed value arrives, which would be too late for the sinks
//! that run first (RAG capture) or outlive the turn (the token forwarder, the capture lease's
//! `Drop`, working-memory consolidation).
//!
//! The mind room is NOT an airc channel: anything published to a channel reaches whoever
//! subscribes. Its id is local, derived from her identity and never published, so the
//! predicate needs no stored state and nothing leaves her node by construction. What starts
//! a turn there is her own wake (her act, a continuation coming due, a resume), never the
//! tick (Fable, 2026-10-05).

use uuid::Uuid;

/// The namespace her mind room id is derived in. Fixed forever: changing it would move
/// every citizen's mind room.
const MIND_ROOM_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6d, 0x69, 0x6e, 0x64, 0x2d, 0x72, 0x6f, 0x6f, 0x6d, 0x2d, 0x6f, 0x66, 0x2d, 0x68, 0x65, 0x72,
]);

/// Her mind room's id: the same on every boot and every node she lives on, and different
/// for every citizen.
pub fn mind_room_id(persona: Uuid) -> Uuid {
    Uuid::new_v5(&MIND_ROOM_NAMESPACE, persona.as_bytes())
}

/// THE gate every capture sink asks: is this turn of `persona` in her mind room? When it
/// is, the sink writes nothing (or writes into her private space), and says so in a probe
/// that names the sink and never the content.
pub fn is_private_room(persona: Uuid, room: Uuid) -> bool {
    room == mind_room_id(persona)
}

/// The one probe a gated sink emits: THAT a private turn skipped it, never WHAT the turn said.
pub fn note_withheld(persona: Uuid, sink: &'static str) {
    crate::probe!(
        class = "mind.private.withheld",
        persona = %persona,
        sink = sink,
        "a turn in her mind room: this sink wrote nothing"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a mind room id that drifts between boots or nodes (her private
    // space would lose its room), or that two citizens share (one could read the other's
    // private turns as her own).
    #[test]
    fn her_mind_room_is_stable_hers_alone_and_no_other_room_is_private() {
        let kimi = Uuid::new_v4();
        let saoirse = Uuid::new_v4();
        assert_eq!(mind_room_id(kimi), mind_room_id(kimi), "stable");
        assert_ne!(mind_room_id(kimi), mind_room_id(saoirse), "hers alone");
        assert!(is_private_room(kimi, mind_room_id(kimi)));
        assert!(!is_private_room(saoirse, mind_room_id(kimi)), "her room is not private for anyone else");
        assert!(!is_private_room(kimi, Uuid::new_v4()), "an ordinary room is open");
    }
}
