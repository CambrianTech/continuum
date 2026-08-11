//! `rag/<verb>` — RAG context composition as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! `rag/compose` once lived ONLY in [`RagModule::handle_command`](crate::modules::rag)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry. As a typed
//! command it gets a descriptor (so it appears in the persona tool surface, the grid ACL,
//! codegen, `uu`) AND routes through the O(1) lock-free typed path. The wire name mirrors
//! the file path — `commands/rag/compose.rs` ⟺ `rag/compose`.
//!
//! The source loaders stay on [`RagState`](crate::modules::rag::RagState) (the module owns
//! its compute); the command holds the same `Arc<RagState>` and drives them. `persona_id`
//! and `room_id` ride on the WIRE (the unchanged ORM contract) — a faithful 1:1 port of
//! the legacy arm. Composing a persona's own context is a read, gated `AiSafe`.

use std::sync::Arc;

use crate::modules::rag::RagState;
use crate::sdk_codegen::DynCommand;

pub mod compose;

use compose::RagCompose;

/// The dep-holding `rag/*` command objects [`RagModule`](crate::modules::rag::RagModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<RagState>`. The executor routes each name straight here, winning over the
/// (now-deleted) legacy `rag/` prefix arm.
pub fn command_objects(state: Arc<RagState>) -> Vec<Arc<dyn DynCommand>> {
    vec![Arc::new(RagCompose { state })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: rag/compose carries its `rag/compose` wire name — the routing
    // key every caller (the RAG builder, cu, the persona tool surface, the grid) binds
    // to. The name mirrors the file path; drift silently de-registers the command from
    // the typed registry and breaks context composition for every persona turn.
    #[test]
    fn rag_command_names_mirror_their_path() {
        assert_eq!(RagCompose::NAME, "rag/compose");
    }
}
