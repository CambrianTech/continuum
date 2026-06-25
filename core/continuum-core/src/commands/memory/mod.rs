//! `memory/<verb>` — the persona's MEMORY hands as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one command per file.
//!
//! ## Why these are typed commands (not a `match` arm)
//!
//! The memory family once lived ONLY in [`MemoryModule::handle_command`](crate::modules::memory)'s
//! stringly `match` — dispatchable, but with no descriptor in the registry, so a
//! persona was never OFFERED recall as a tool. As typed commands each gets a
//! descriptor (so it appears in the persona tool surface, the grid ACL, codegen,
//! `cu`) AND routes through the O(1) lock-free typed path. The wire name mirrors the
//! file path — `commands/memory/multi_layer_recall.rs` ⟺ `memory/multi-layer-recall`.
//!
//! ## Identity note
//!
//! These commands take `persona_id` on the WIRE (snake_case, the unchanged ORM
//! contract): `load-corpus`/`append-*` are infrastructure/consolidation writes that
//! name the target persona explicitly, so this is a faithful 1:1 port of the legacy
//! arms — not the `ctx.caller`-derived identity the git family uses. Tightening the
//! identity axis is deliberately out of scope for the registry collapse; the writes
//! are gated `Privileged` and reads `AiSafe` as the trust boundary.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::memory::MemoryState;
use crate::sdk_codegen::DynCommand;

pub mod append_event;
pub mod append_memory;
pub mod consciousness_context;
pub mod load_corpus;
pub mod multi_layer_recall;

use append_event::MemoryAppendEvent;
use append_memory::MemoryAppendMemory;
use consciousness_context::MemoryConsciousnessContext;
use load_corpus::MemoryLoadCorpus;
use multi_layer_recall::MemoryMultiLayerRecall;

/// Result of an incremental append (`memory/append-memory`, `memory/append-event`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/AppendResult.ts")]
pub struct AppendResult {
    /// Always true on success (the call fails loud rather than returning false).
    pub appended: bool,
}

/// The dep-holding memory command objects [`MemoryModule`](crate::modules::memory::MemoryModule)
/// contributes to the kernel's typed object map, each sharing the module's
/// `Arc<MemoryState>`. The executor routes each name straight here, winning over the
/// (now-deleted) legacy `memory/` prefix arm.
pub fn command_objects(state: Arc<MemoryState>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(MemoryLoadCorpus { state: state.clone() }),
        Arc::new(MemoryMultiLayerRecall { state: state.clone() }),
        Arc::new(MemoryConsciousnessContext { state: state.clone() }),
        Arc::new(MemoryAppendMemory { state: state.clone() }),
        Arc::new(MemoryAppendEvent { state }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the five memory commands carry their `memory/<verb>` wire
    // names — the routing keys every caller (cu, the persona tool surface, the grid)
    // binds to. The name mirrors the file path; drift silently breaks the "file tree
    // IS the namespace" contract and de-registers a command from the persona surface.
    #[test]
    fn memory_command_names_mirror_their_path() {
        assert_eq!(MemoryLoadCorpus::NAME, "memory/load-corpus");
        assert_eq!(MemoryMultiLayerRecall::NAME, "memory/multi-layer-recall");
        assert_eq!(
            MemoryConsciousnessContext::NAME,
            "memory/consciousness-context"
        );
        assert_eq!(MemoryAppendMemory::NAME, "memory/append-memory");
        assert_eq!(MemoryAppendEvent::NAME, "memory/append-event");
    }
}
