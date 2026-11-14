//! Whether a citizen holds durable memory, by id: the store `data/*` opens for a UUID handle.

use super::DataState;

impl DataState {
    /// Whether `id` has a per-citizen store, resolved by the one handle rule `data/*` opens. A
    /// directory holding one is memory, whoever wrote it (`memory/*` maps any UUID here). A store
    /// that cannot be located counts as held: nothing is reclaimed on a guess.
    pub fn holds_citizen_store(id: uuid::Uuid) -> bool {
        match Self::resolve_handle_with(&id.to_string(), |key| std::env::var(key).ok(), dirs::home_dir) {
            Ok(path) => std::path::Path::new(&path).exists(),
            Err(reason) => {
                continuum_base::probe!(class = "data.citizen_store_unresolved", id = %id, reason = %reason);
                true
            }
        }
    }
}
