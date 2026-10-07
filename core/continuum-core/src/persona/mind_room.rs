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

/// The one record in her mind store that holds her private continuation (sink 10): a
/// stable id, so writing a new one replaces the old and clearing it removes it.
pub fn continuation_record_id(persona: Uuid) -> Uuid {
    let mut name = persona.as_bytes().to_vec();
    name.extend_from_slice(b"continuation");
    Uuid::new_v5(&MIND_ROOM_NAMESPACE, &name)
}

/// Seal her private continuation into her mind store (`Some`), or clear it (`None`).
/// Goes through her own runtime's `Airc::mind_store()`, so the core never holds her key.
/// `Ok(false)` = she has no resident airc runtime here; nothing was written.
pub fn seal_continuation(
    persona: Uuid,
    continuation: Option<&crate::persona::attention::Continuation>,
) -> Result<bool, String> {
    let Some(runtime) = crate::persona::PersonaAircRuntimeRegistry::try_global().and_then(|r| r.get(persona)) else {
        return Ok(false);
    };
    let store = runtime.airc().mind_store().map_err(|e| e.to_string())?;
    let id = continuation_record_id(persona);
    match continuation {
        Some(c) => {
            let json = serde_json::to_string(c).map_err(|e| e.to_string())?; // disk boundary: her continuation as a sealed record in her mind store
            store.put_at(id, &json).map_err(|e| e.to_string())?;
        }
        None => store.remove(id).map_err(|e| e.to_string())?,
    }
    Ok(true)
}

/// Her sealed private continuation, if she left one: read back at boot so a Resume wake
/// continues her private thread after a restart. A store that will not open or a record
/// that will not parse is probed and read as none: her open state boots regardless.
pub fn sealed_continuation(persona: Uuid) -> Option<crate::persona::attention::Continuation> {
    let runtime = crate::persona::PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
    let store = match runtime.airc().mind_store() {
        Ok(store) => store,
        Err(e) => {
            crate::probe!(class = "mind.private.store_unopened", persona = %persona, error = %e.to_string(), "her mind store did not open at boot; no private continuation restored");
            return None;
        }
    };
    let id = continuation_record_id(persona);
    if !store.list().ok()?.contains(&id) {
        return None;
    }
    match store.get(id).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) { // disk boundary: read back from her sealed mind store
        Ok(c) => Some(c),
        Err(e) => {
            crate::probe!(class = "mind.private.continuation_unreadable", persona = %persona, error = %e, "her sealed continuation did not read back; not restored");
            None
        }
    }
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

    // what this catches: her continuation record moving between boots (a restart could
    // not find what she sealed) or colliding with another citizen's.
    #[test]
    fn her_continuation_record_is_stable_and_hers() {
        let kimi = Uuid::new_v4();
        assert_eq!(continuation_record_id(kimi), continuation_record_id(kimi));
        assert_ne!(continuation_record_id(kimi), continuation_record_id(Uuid::new_v4()));
        assert_ne!(continuation_record_id(kimi), mind_room_id(kimi), "a record, not her room");
    }
}
