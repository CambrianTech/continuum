//! Whether an ADOPTED engine can be replaced now without losing its minds' KV. A core that
//! adopts the engine a previous core left running learns its slot pool and saved ledger only
//! after it starts serving it; until then `checkpoint_adopted_before_replacement` refuses, and
//! a replacement asked for at boot (the one move onto the shared pool, 2026-11-03 04:22Z on the
//! M5: "adopted checkpoint requires a known pool") is handed back. This is that refusal's
//! precondition as one read, so the serving owner asks again exactly when it can succeed.

use super::SlotDirectory;

impl SlotDirectory {
    /// The adopted engine at `root` could be checkpointed before a replacement: its pool is
    /// known with a verified ledger, no generation of this core owns it, and its paging is
    /// certain. An engine this core launched itself owns a generation and answers false.
    pub fn adopted_replacement_ready(&self, root: &str) -> bool {
        let endpoint = self.endpoint(root);
        let state = endpoint.state.lock();
        state.generation.is_none()
            && !state.paging_uncertain
            && state
                .current_pool()
                .is_some_and(|pool| pool.ledger.get().is_some())
    }
}
