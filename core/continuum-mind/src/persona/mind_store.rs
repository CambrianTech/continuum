//! Her sealed mind store, reached through her own airc runtime so the core never holds her
//! key: where her private continuation (sink 10 of `cognition::mind_room`) is kept across a
//! restart.

use uuid::Uuid;

use crate::persona::PersonaAircRuntimeRegistry;

/// Seal her private continuation into her mind store (`Some`), or clear it (`None`).
/// Goes through her own runtime's `Airc::mind_store()`, so the core never holds her key.
/// `Ok(false)` = she has no resident airc runtime here; nothing was written.
pub fn seal_continuation(
    persona: Uuid,
    continuation: Option<&crate::cognition::attention::Continuation>,
) -> Result<bool, String> {
    let Some(runtime) = PersonaAircRuntimeRegistry::try_global().and_then(|r| r.get(persona)) else {
        return Ok(false);
    };
    let store = runtime.airc().mind_store().map_err(|e| e.to_string())?;
    let id = continuum_capture::mind_room::continuation_record_id(persona);
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
pub fn sealed_continuation(persona: Uuid) -> Option<crate::cognition::attention::Continuation> {
    let runtime = PersonaAircRuntimeRegistry::try_global()?.get(persona)?;
    let store = match runtime.airc().mind_store() {
        Ok(store) => store,
        Err(e) => {
            continuum_base::probe!(class = "mind.private.store_unopened", persona = %persona, error = %e.to_string(), "her mind store did not open at boot; no private continuation restored");
            return None;
        }
    };
    let id = continuum_capture::mind_room::continuation_record_id(persona);
    if !store.list().ok()?.contains(&id) {
        return None;
    }
    match store.get(id).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) { // disk boundary: read back from her sealed mind store
        Ok(c) => Some(c),
        Err(e) => {
            continuum_base::probe!(class = "mind.private.continuation_unreadable", persona = %persona, error = %e, "her sealed continuation did not read back; not restored");
            None
        }
    }
}
