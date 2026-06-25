//! `data/<verb>` — the entity data layer as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! `data/*` once lived ONLY in [`DataModule::handle_command`](crate::modules::data)'s
//! stringly `match` (via `DataState::dispatch`) — dispatchable, but with no descriptor
//! in the registry, so invisible to the persona tool surface, the grid ACL, codegen, and
//! `cu`. As typed commands they get a descriptor AND route through the O(1) lock-free
//! typed object map. The wire name mirrors the file path — `commands/data/list.rs` ⟺
//! `data/list`.
//!
//! ## The persona lens
//!
//! These contracts are authored from the persona's seat: a citizen reading rooms or
//! messages should never reason about a database handle. So `data/list` takes a clean
//! `collection (+ filter/sort/paging)` contract with NO `db_path` — the shared "main"
//! store is the default, and a power caller may target a per-persona store via the
//! optional `handle`. The storage compute stays on [`DataState`](crate::modules::data::DataState)
//! (the module owns its state); each command holds the same `Arc<DataState>` and drives it.

use std::sync::Arc;

use crate::modules::data::DataState;
use crate::sdk_codegen::DynCommand;

pub mod list;

use list::DataList;

/// The dep-holding `data/*` command objects [`DataModule`](crate::modules::data::DataModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<DataState>`. The executor routes each name straight here, winning over the
/// legacy `data/` prefix arm (which shrinks toward deletion as arms migrate).
pub fn command_objects(state: Arc<DataState>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(DataList { state })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: data/list carries its `data/list` wire name — the routing
    // key every caller (the persona tool surface, cu, the grid ACL, every SDK) binds
    // to. The name mirrors the file path; drift silently de-registers the command from
    // the typed registry and a persona loses its ability to read any collection.
    #[test]
    fn data_command_names_mirror_their_path() {
        assert_eq!(DataList::NAME, "data/list");
    }
}
