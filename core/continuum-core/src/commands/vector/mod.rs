//! `vector/<verb>` — the embedding/vector-search surface of the data layer as
//! typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one per file.
//!
//! These once lived ONLY in [`DataModule::handle_command`](crate::modules::data)'s
//! stringly `match` (the `vector/` prefix arms) — dispatchable, but with no descriptor
//! in the registry, so invisible to the persona tool surface, the grid ACL, codegen,
//! and `uu`. As typed commands they get a descriptor AND route through the O(1)
//! lock-free typed object map. The wire name mirrors the file path —
//! `commands/vector/search.rs` ⟺ `vector/search`.
//!
//! ## The persona lens
//!
//! Same contract discipline as `data/*`: a citizen searching its own memory should
//! never reason about a database handle. Every command takes a clean collection-level
//! contract; the shared "main" store is the default, and a power caller may target a
//! per-persona store via the optional `handle` (legacy `dbPath` accepted as an alias).
//! The vector compute stays on [`DataState`](crate::modules::data::DataState); each
//! command holds the same `Arc<DataState>` and drives it.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::sdk_codegen::DynCommand;

pub mod backfill;
pub mod index;
pub mod invalidate_cache;
pub mod search;
pub mod stats;

use backfill::VectorBackfill;
use index::VectorIndex;
use invalidate_cache::VectorInvalidateCache;
use search::VectorSearch;
use stats::VectorStatsCommand;

/// The dep-holding `vector/*` command objects [`DataModule`](crate::modules::data::DataModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<DataState>`. The executor routes each name straight here, winning over the
/// legacy `vector/` prefix arm (now deleted).
pub fn command_objects(state: Arc<DataState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(VectorSearch {
            state: state.clone(),
        }),
        Arc::new(VectorIndex {
            state: state.clone(),
        }),
        Arc::new(VectorStatsCommand {
            state: state.clone(),
        }),
        Arc::new(VectorInvalidateCache {
            state: state.clone(),
        }),
        Arc::new(VectorBackfill { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: each vector command carries its `vector/<verb>` wire name —
    // the routing key the persona tool surface, cu, the grid ACL, and every SDK bind
    // to. The name mirrors the file path; drift silently de-registers the command and
    // a persona loses semantic search over its own memory.
    #[test]
    fn vector_command_names_mirror_their_path() {
        assert_eq!(VectorSearch::NAME, "vector/search");
        assert_eq!(VectorIndex::NAME, "vector/index");
        assert_eq!(VectorStatsCommand::NAME, "vector/stats");
        assert_eq!(VectorInvalidateCache::NAME, "vector/invalidate-cache");
        assert_eq!(VectorBackfill::NAME, "vector/backfill");
    }
}
